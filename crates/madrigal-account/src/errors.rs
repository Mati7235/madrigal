use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountError {
    InvalidResponse,
    Network,
    Unauthorized,
    BrowserUnavailable,
    BrowserNoSession,
    CookieIncomplete,
}

impl AccountError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidResponse => "account_invalid_response",
            Self::Network => "account_network",
            Self::Unauthorized => "account_unauthorized",
            Self::BrowserUnavailable => "browser_unavailable",
            Self::BrowserNoSession => "browser_no_session",
            Self::CookieIncomplete => "cookie_incomplete",
        }
    }

    pub fn expired(self) -> bool {
        self == Self::Unauthorized
    }
}

impl fmt::Display for AccountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::InvalidResponse => "YouTube Music returned an invalid account response",
            Self::Network => "could not reach YouTube Music",
            Self::Unauthorized => "YouTube Music rejected the session",
            Self::BrowserUnavailable => "could not access the selected browser",
            Self::BrowserNoSession => "no YouTube session found in the browser",
            Self::CookieIncomplete => "cookies lack the data needed to sign in",
        };
        f.write_str(text)
    }
}

impl std::error::Error for AccountError {}

pub const EXPIRED_STATUS: [u16; 2] = [401, 403];

const EXPIRED_MARKERS: [&str; 9] = [
    "401",
    "unauthorized",
    "login_required",
    "login required",
    "sign in to confirm",
    "not a bot",
    "invalid_grant",
    "session expired",
    "accounts.google.com",
];

pub fn session_expired_status(status: u16) -> bool {
    EXPIRED_STATUS.contains(&status)
}

pub fn session_expired_message(text: &str) -> bool {
    let lowered = text.to_lowercase();
    EXPIRED_MARKERS.iter().any(|marker| lowered.contains(marker))
}
