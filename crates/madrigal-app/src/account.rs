use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Duration;

use madrigal_account::browser::{records_from_browser, BrowserError};
use madrigal_account::cookies::merge_login_cookies;
use madrigal_account::session::Session;
use madrigal_account::store::SessionStore;
use madrigal_innertube::CatalogClient;
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

pub fn validate_cookie(cookie: &str) -> Result<(), String> {
    let client = CatalogClient::with_cookies(cookie).map_err(|err| err.to_string())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| err.to_string())?;
    runtime
        .block_on(client.library_playlists())
        .map(|_| ())
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

pub fn run_login_with(validator: &dyn Fn(&str) -> Result<(), String>) -> LoginOutcome {
    let session = match import_login() {
        Ok(session) => session,
        Err(key) => {
            return LoginOutcome::Failed { error: key.to_owned() };
        }
    };
    if validator(&session.cookie).is_err() {
        return LoginOutcome::Failed { error: ERR_AUTH.to_owned() };
    }
    let store = SessionStore::default();
    let _ = store.save(&session, None);
    LoginOutcome::Connected {
        name: session.account_name.clone(),
        email: session.account_email.clone(),
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
    if session.catalog_ready() {
        Some(session)
    } else {
        None
    }
}

pub fn refresh_account(shell: &MainShell, presenter: &Rc<RefCell<SettingsPresenter>>) {
    let state = presenter.borrow();
    refresh_settings(shell, &state);
}

fn apply_outcome(
    shell: &MainShell,
    presenter: &Rc<RefCell<SettingsPresenter>>,
    catalog: &Wiring,
    outcome: LoginOutcome,
) {
    match outcome {
        LoginOutcome::Connected { name, email } => {
            let session = SessionStore::default().active();
            if session.catalog_ready() {
                catalog.set_session(Some(session.cookie.clone()));
            }
            let mut state = presenter.borrow_mut();
            state.finish_account_work();
            state.set_account_connected(&name, &email);
        }
        LoginOutcome::Failed { error } => {
            let mut state = presenter.borrow_mut();
            state.finish_account_work();
            state.set_account_error(&error);
        }
    }
    refresh_account(shell, presenter);
}

fn poll_outcome(
    shell: &MainShell,
    presenter: &Rc<RefCell<SettingsPresenter>>,
    catalog: &Wiring,
    pending: &Rc<RefCell<Option<Receiver<LoginOutcome>>>>,
) {
    let ready = pending
        .borrow_mut()
        .as_ref()
        .and_then(|rx| rx.try_recv().ok());
    if let Some(outcome) = ready {
        *pending.borrow_mut() = None;
        apply_outcome(shell, presenter, catalog, outcome);
    }
}

fn spawn_login(
    pending: &Rc<RefCell<Option<Receiver<LoginOutcome>>>>,
    job: impl FnOnce() -> LoginOutcome + Send + 'static,
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
    validator: fn(&str) -> Result<(), String>,
) -> slint::Timer {
    let store = SessionStore::default();
    match restore_session(&store) {
        Some(session) => {
            presenter
                .borrow_mut()
                .set_account_connected(&session.account_name, &session.account_email);
            catalog.set_session(Some(session.cookie.clone()));
        }
        None => {
            presenter.borrow_mut().clear_account();
        }
    }
    refresh_account(shell, &presenter);

    let pending: Rc<RefCell<Option<Receiver<LoginOutcome>>>> = Rc::new(RefCell::new(None));
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
                poll_outcome(&shell, &timer_presenter, &timer_catalog, &timer_pending);
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
                spawn_login(&handler_pending, move || run_login_with(&validator));
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
