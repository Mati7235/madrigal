use std::fmt;

use crate::cookies::CookieRecord;

pub const BROWSERS: [(&str, &str); 7] = [
    ("firefox", "Firefox"),
    ("brave", "Brave"),
    ("chrome", "Google Chrome"),
    ("chromium", "Chromium"),
    ("edge", "Microsoft Edge"),
    ("opera", "Opera"),
    ("vivaldi", "Vivaldi"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserError {
    Unknown(String),
    Unsupported(String),
}

impl fmt::Display for BrowserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(browser) => write!(f, "unknown browser: {browser:?}"),
            Self::Unsupported(browser) => {
                write!(f, "browser cookie import not implemented: {browser:?}")
            }
        }
    }
}

impl std::error::Error for BrowserError {}

// TODO(seam): read Chromium/Firefox sqlite cookie stores; blocked until a
// store reader lands, so known browsers report Unsupported for now.
pub fn records_from_browser(
    browser: &str,
    _profile: &str,
) -> Result<Vec<CookieRecord>, BrowserError> {
    if !BROWSERS.iter().any(|(key, _)| *key == browser) {
        return Err(BrowserError::Unknown(browser.to_owned()));
    }
    Err(BrowserError::Unsupported(browser.to_owned()))
}
