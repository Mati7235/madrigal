use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Duration;

use madrigal_account::browser::{records_from_browser, BrowserError};
use madrigal_account::cookies::merge_login_cookies;
use madrigal_account::session::Session;
use madrigal_account::store::SessionStore;
use madrigal_innertube::{AccountInfo, CatalogClient};
use madrigal_ui::{refresh_settings, MainShell, SettingsPresenter};
use slint::ComponentHandle;

use crate::catalog::Wiring;

pub const ERR_NO_FIREFOX: &str = "settings.errNoFirefox";
pub const ERR_PROFILE: &str = "settings.errProfileNotFound";
pub const ERR_NO_LOGIN: &str = "settings.errNoLogin";
pub const ERR_READ: &str = "settings.errRead";
pub const ERR_AUTH: &str = "settings.errAuth";
pub const ERR_UNKNOWN: &str = "settings.errUnknown";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginOutcome {
    Connected { name: String, email: String },
    Failed { error: String },
}

enum AccountEvent {
    Restore(Option<Session>),
    Login {
        outcome: LoginOutcome,
        session: Option<Session>,
    },
}

fn error_key(err: &BrowserError) -> &'static str {
    match err {
        BrowserError::Unknown(_) => ERR_UNKNOWN,
        BrowserError::Unsupported(_) => ERR_UNKNOWN,
        BrowserError::NotFound(_) => ERR_NO_FIREFOX,
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

pub fn import_login() -> Result<Session, &'static str> {
    let records = records_from_browser("firefox", "").map_err(|err| error_key(&err))?;
    Ok(Session {
        cookie: merge_login_cookies(&records),
        source_browser: "firefox".to_owned(),
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

pub fn run_login_with(validator: &dyn Fn(&str) -> Result<AccountInfo, String>) -> LoginOutcome {
    let mut session = match import_login() {
        Ok(session) => session,
        Err(key) => {
            return LoginOutcome::Failed {
                error: key.to_owned(),
            };
        }
    };
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
    let _ = store.save(&session, None);
    LoginOutcome::Connected {
        name: session.account_name.clone(),
        email: identity(&session),
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

fn poll_account(
    shell: &MainShell,
    presenter: &Rc<RefCell<SettingsPresenter>>,
    catalog: &Wiring,
    pending: &Rc<RefCell<Option<Receiver<AccountEvent>>>>,
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
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(100),
            move || {
                let Some(shell) = weak.upgrade() else {
                    return;
                };
                poll_account(&shell, &timer_presenter, &timer_catalog, &timer_pending);
            },
        );
    }

    let weak = shell.as_weak();
    let handler_presenter = presenter.clone();
    let handler_catalog = Arc::clone(&catalog);
    let handler_pending = Rc::clone(&pending);
    shell.on_settings_login(move |id| {
        let Some(shell) = weak.upgrade() else {
            return;
        };
        match id.as_str() {
            "firefox" | "google" => {
                handler_presenter.borrow_mut().begin_account_work();
                refresh_account(&shell, &handler_presenter);
                spawn_account(&handler_pending, move || {
                    let outcome = run_login_with(&validator);
                    let session = match &outcome {
                        LoginOutcome::Connected { .. } => Some(SessionStore::default().active()),
                        LoginOutcome::Failed { .. } => None,
                    };
                    AccountEvent::Login { outcome, session }
                });
            }
            "signout" => {
                sign_out(&SessionStore::default());
                handler_catalog.set_session(None);
                handler_presenter.borrow_mut().clear_account();
                refresh_account(&shell, &handler_presenter);
            }
            _ => {}
        }
    });
    timer
}
