pub mod auth;
pub mod browser;
mod chromium;
pub mod cookies;
pub mod errors;
mod secrets;
pub mod session;
pub mod store;

pub use auth::{AuthHeaders, browser_auth, on_behalf_of_user};
pub use browser::{
    choose_browser, detect_browsers, import_candidate, records_from_browser, BrowserCandidate,
    BrowserChoice, BrowserError,
};
pub use cookies::{CookieError, CookieRecord};
pub use errors::AccountError;
pub use madrigal_secrets::SecretError;
pub use session::Session;
pub use store::SessionStore;
