use std::fmt;

#[derive(Debug)]
pub enum Error {
    NotConfigured(&'static str),
    BadUrl(String),
    Transport(reqwest::Error),
    HttpStatus(u16),
    Api(i64, String),
    BadResponse(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotConfigured(what) => write!(f, "scrobble client not configured: {what}"),
            Error::BadUrl(detail) => write!(f, "invalid scrobble url: {detail}"),
            Error::Transport(err) => write!(f, "scrobble transport error: {err}"),
            Error::HttpStatus(code) => write!(f, "scrobble server returned HTTP {code}"),
            Error::Api(code, message) => write!(f, "scrobble api error {code}: {message}"),
            Error::BadResponse(detail) => write!(f, "unexpected scrobble response: {detail}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Transport(err) => Some(err),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
