use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Duration;

use madrigal_account::browser::{
    choose_browser, detect_browsers, import_candidate, BrowserCandidate, BrowserChoice,
    BrowserError,
};
use madrigal_account::cookies::{
    has_login_cookie, has_primary_sapisid, merge_login_cookies, parse_cookie_header, CookieRecord,
};
use madrigal_account::session::Session;
use madrigal_account::store::SessionStore;
use madrigal_innertube::{AccountInfo, CatalogClient};
use madrigal_ui::{refresh_settings, MainShell, SettingsPresenter};
use slint::ComponentHandle;

use crate::catalog::Wiring;

pub const ERR_NO_BROWSER: &str = "settings.errNoBrowser";
pub const ERR_CHOICE: &str = "settings.errNoChoice";
pub const ERR_MANUAL: &str = "settings.errManual";
pub const ERR_PROFILE: &str = "settings.errProfileNotFound";
pub const ERR_NO_LOGIN: &str = "settings.errNoLogin";
pub const ERR_READ: &str = "settings.errRead";
pub const ERR_AUTH: &str = "settings.errAuth";
pub const ERR_KEYRING: &str = "settings.errKeyring";
pub const ERR_UNKNOWN: &str = "settings.errUnknown";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginOutcome {
    Connected { name: String, email: String },
    Failed { error: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginStart {
    Done(LoginOutcome),
    Choose(Vec<BrowserCandidate>),
}

enum AccountEvent {
    Restore(Option<Session>),
    Login {
        outcome: LoginOutcome,
        session: Option<Session>,
    },
    Choices(Vec<BrowserCandidate>),
}

fn error_key(err: &BrowserError) -> &'static str {
    match err {
        BrowserError::Unknown(_) => ERR_UNKNOWN,
        BrowserError::NotFound(_) => ERR_NO_BROWSER,
        BrowserError::ProfileNotFound(_) => ERR_PROFILE,
        BrowserError::NoLogin(_) => ERR_NO_LOGIN,
        BrowserError::ReadFailed(_) => ERR_READ,
    }
}

pub fn validate_cookie(cookie: &str) -> Result<AccountInfo, String> {
    let client = CatalogClient::with_cookies(cookie).map_err(|err| err.to_string())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| err.to_string())?;
    runtime
        .block_on(client.account_info())
        .map_err(|err| err.to_string())
}

pub fn import_login(candidate: &BrowserCandidate) -> Result<Session, &'static str> {
    let records = import_candidate(candidate).map_err(|err| error_key(&err))?;
    Ok(Session {
        cookie: merge_login_cookies(&records),
        source_browser: candidate.key.clone(),
        source_profile: candidate.profile.clone(),
        ..Session::default()
    })
}

fn identity(session: &Session) -> String {
    if session.account_email.is_empty() {
        session.channel_handle.clone()
    } else {
        session.account_email.clone()
    }
}

fn finish_login(
    mut session: Session,
    validator: &dyn Fn(&str) -> Result<AccountInfo, String>,
) -> LoginOutcome {
    let info = match validator(&session.cookie) {
        Ok(info) => info,
        Err(_) => {
            return LoginOutcome::Failed {
                error: ERR_AUTH.to_owned(),
            };
        }
    };
    session.account_name = info.name;
    session.account_email = info.email.unwrap_or_default();
    session.channel_handle = info.handle.unwrap_or_default();
    session.channel_id = info.channel_id.unwrap_or_default();
    session.avatar_url = info.avatar.unwrap_or_default();
    let store = SessionStore::default();
    match store.save(&session, None) {
        Ok(_) => LoginOutcome::Connected {
            name: session.account_name.clone(),
            email: identity(&session),
        },
        Err(err) if err.no_keyring() => LoginOutcome::Failed {
            error: ERR_KEYRING.to_owned(),
        },
        Err(_) => LoginOutcome::Failed {
            error: ERR_UNKNOWN.to_owned(),
        },
    }
}

pub fn login_from_candidate_with(
    candidate: &BrowserCandidate,
    validator: &dyn Fn(&str) -> Result<AccountInfo, String>,
) -> LoginOutcome {
    match import_login(candidate) {
        Ok(session) => finish_login(session, validator),
        Err(key) => LoginOutcome::Failed {
            error: key.to_owned(),
        },
    }
}

pub fn login_with_cookie(
    cookie: &str,
    source: &str,
    validator: &dyn Fn(&str) -> Result<AccountInfo, String>,
) -> LoginOutcome {
    let Ok(pairs) = parse_cookie_header(cookie) else {
        return LoginOutcome::Failed {
            error: ERR_MANUAL.to_owned(),
        };
    };
    if !has_login_cookie(&pairs) && !has_primary_sapisid(&pairs) {
        return LoginOutcome::Failed {
            error: ERR_MANUAL.to_owned(),
        };
    }
    let records: Vec<CookieRecord> = pairs
        .iter()
        .map(|(name, value)| CookieRecord::new(name, value, ".youtube.com"))
        .collect();
    let session = Session {
        cookie: merge_login_cookies(&records),
        source_browser: source.to_owned(),
        ..Session::default()
    };
    finish_login(session, validator)
}

pub fn start_login_with(validator: &dyn Fn(&str) -> Result<AccountInfo, String>) -> LoginStart {
    match choose_browser(detect_browsers()) {
        BrowserChoice::None => LoginStart::Done(LoginOutcome::Failed {
            error: ERR_NO_BROWSER.to_owned(),
        }),
        BrowserChoice::Single(candidate) => {
            LoginStart::Done(login_from_candidate_with(&candidate, validator))
        }
        BrowserChoice::Multiple(candidates) => LoginStart::Choose(candidates),
    }
}

pub fn run_login_with(validator: &dyn Fn(&str) -> Result<AccountInfo, String>) -> LoginOutcome {
    match start_login_with(validator) {
        LoginStart::Done(outcome) => outcome,
        LoginStart::Choose(_) => LoginOutcome::Failed {
            error: ERR_CHOICE.to_owned(),
        },
    }
}

pub fn run_login() -> LoginOutcome {
    run_login_with(&validate_cookie)
}

pub fn sign_out(store: &SessionStore) {
    let profile = store.active_id();
    let mut session = store.load(&profile);
    if let (Ok(account_path), Ok(cookies_path)) =
        (store.account_path(&profile), store.cookies_path(None))
    {
        session.clear(&account_path, &cookies_path);
    }
}

pub fn restore_session(store: &SessionStore) -> Option<Session> {
    store.migrate();
    let session = store.active();
    if session.catalog_ready() && !session.account_name.is_empty() {
        Some(session)
    } else {
        None
    }
}

pub fn refresh_account(shell: &MainShell, presenter: &Rc<RefCell<SettingsPresenter>>) {
    let state = presenter.borrow();
    refresh_settings(shell, &state);
}

fn apply_restore(
    shell: &MainShell,
    presenter: &Rc<RefCell<SettingsPresenter>>,
    catalog: &Wiring,
    session: Option<Session>,
) {
    {
        let mut state = presenter.borrow_mut();
        if state.account_busy || state.account_connected {
            return;
        }
        match session {
            Some(session) if session.catalog_ready() => {
                state.set_account_connected(&session.account_name, &identity(&session));
                catalog.set_session(Some(session.cookie));
            }
            _ => state.clear_account(),
        }
    }
    refresh_account(shell, presenter);
}

fn apply_login(
    shell: &MainShell,
    presenter: &Rc<RefCell<SettingsPresenter>>,
    catalog: &Wiring,
    outcome: LoginOutcome,
    session: Option<Session>,
) {
    {
        let mut state = presenter.borrow_mut();
        state.finish_account_work();
        match outcome {
            LoginOutcome::Connected { name, email } => {
                if let Some(session) = session.filter(Session::catalog_ready) {
                    catalog.set_session(Some(session.cookie));
                }
                state.set_account_connected(&name, &email);
            }
            LoginOutcome::Failed { error } => state.set_account_error(&error),
        }
    }
    refresh_account(shell, presenter);
}

fn apply_choices(
    shell: &MainShell,
    presenter: &Rc<RefCell<SettingsPresenter>>,
    choices: &Rc<RefCell<Vec<BrowserCandidate>>>,
    candidates: Vec<BrowserCandidate>,
) {
    {
        let names: Vec<String> = candidates
            .iter()
            .map(|candidate| candidate.name.clone())
            .collect();
        *choices.borrow_mut() = candidates;
        let mut state = presenter.borrow_mut();
        state.finish_account_work();
        state.set_account_choices(names);
    }
    refresh_account(shell, presenter);
}

fn poll_account(
    shell: &MainShell,
    presenter: &Rc<RefCell<SettingsPresenter>>,
    catalog: &Wiring,
    pending: &Rc<RefCell<Option<Receiver<AccountEvent>>>>,
    choices: &Rc<RefCell<Vec<BrowserCandidate>>>,
) {
    let ready = pending
        .borrow_mut()
        .as_ref()
        .and_then(|rx| rx.try_recv().ok());
    match ready {
        Some(AccountEvent::Restore(session)) => {
            apply_restore(shell, presenter, catalog, session);
        }
        Some(AccountEvent::Login { outcome, session }) => {
            apply_login(shell, presenter, catalog, outcome, session);
        }
        Some(AccountEvent::Choices(candidates)) => {
            apply_choices(shell, presenter, choices, candidates);
        }
        None => {}
    }
}

fn spawn_account(
    pending: &Rc<RefCell<Option<Receiver<AccountEvent>>>>,
    job: impl FnOnce() -> AccountEvent + Send + 'static,
) {
    let (tx, rx) = mpsc::channel();
    *pending.borrow_mut() = Some(rx);
    let _ = std::thread::Builder::new()
        .name("madrigal-account".to_owned())
        .spawn(move || {
            let _ = tx.send(job());
        });
}

pub fn wire_account(
    shell: &MainShell,
    presenter: Rc<RefCell<SettingsPresenter>>,
    catalog: Arc<Wiring>,
) -> slint::Timer {
    wire_account_with(shell, presenter, catalog, validate_cookie)
}

pub fn wire_account_with(
    shell: &MainShell,
    presenter: Rc<RefCell<SettingsPresenter>>,
    catalog: Arc<Wiring>,
    validator: fn(&str) -> Result<AccountInfo, String>,
) -> slint::Timer {
    let pending: Rc<RefCell<Option<Receiver<AccountEvent>>>> = Rc::new(RefCell::new(None));
    let choices: Rc<RefCell<Vec<BrowserCandidate>>> = Rc::new(RefCell::new(Vec::new()));
    spawn_account(&pending, || {
        AccountEvent::Restore(restore_session(&SessionStore::default()))
    });
    refresh_account(shell, &presenter);

    let timer = slint::Timer::default();
    {
        let weak = shell.as_weak();
        let timer_presenter = presenter.clone();
        let timer_catalog = Arc::clone(&catalog);
        let timer_pending = Rc::clone(&pending);
        let timer_choices = Rc::clone(&choices);
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(100),
            move || {
                let Some(shell) = weak.upgrade() else {
                    return;
                };
                poll_account(
                    &shell,
                    &timer_presenter,
                    &timer_catalog,
                    &timer_pending,
                    &timer_choices,
                );
            },
        );
    }

    let weak = shell.as_weak();
    let handler_presenter = presenter.clone();
    let handler_catalog = Arc::clone(&catalog);
    let handler_pending = Rc::clone(&pending);
    let handler_choices = Rc::clone(&choices);
    shell.on_settings_login(move |id| {
        let Some(shell) = weak.upgrade() else {
            return;
        };
        if id.as_str() == "signout" {
            *handler_pending.borrow_mut() = None;
            handler_choices.borrow_mut().clear();
            sign_out(&SessionStore::default());
            handler_catalog.set_session(None);
            handler_presenter.borrow_mut().clear_account();
            refresh_account(&shell, &handler_presenter);
            return;
        }
        let manual = id.strip_prefix("manual:").map(str::to_owned);
        let candidate = if manual.is_none() {
            handler_choices
                .borrow()
                .iter()
                .find(|candidate| candidate.name == id.as_str())
                .cloned()
        } else {
            None
        };
        if manual.is_none()
            && candidate.is_none()
            && !matches!(id.as_str(), "browser" | "firefox" | "google")
        {
            return;
        }
        handler_presenter.borrow_mut().begin_account_work();
        handler_choices.borrow_mut().clear();
        refresh_account(&shell, &handler_presenter);
        spawn_account(&handler_pending, move || {
            if let Some(cookie) = manual {
                let outcome = login_with_cookie(&cookie, "manual", &validator);
                let session = match &outcome {
                    LoginOutcome::Connected { .. } => Some(SessionStore::default().active()),
                    LoginOutcome::Failed { .. } => None,
                };
                return AccountEvent::Login { outcome, session };
            }
            if let Some(candidate) = candidate {
                let outcome = login_from_candidate_with(&candidate, &validator);
                let session = match &outcome {
                    LoginOutcome::Connected { .. } => Some(SessionStore::default().active()),
                    LoginOutcome::Failed { .. } => None,
                };
                return AccountEvent::Login { outcome, session };
            }
            match start_login_with(&validator) {
                LoginStart::Done(outcome) => {
                    let session = match &outcome {
                        LoginOutcome::Connected { .. } => Some(SessionStore::default().active()),
                        LoginOutcome::Failed { .. } => None,
                    };
                    AccountEvent::Login { outcome, session }
                }
                LoginStart::Choose(candidates) => AccountEvent::Choices(candidates),
            }
        });
    });
    timer
}
