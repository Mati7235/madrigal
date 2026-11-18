use std::fmt;

pub mod discovery;
pub mod media;
pub mod protocol;
pub mod session;

#[derive(Debug)]
pub enum Error {
    Malformed(String),
    NeedMore,
    UnexpectedType(String),
    WrongState(String),
    Transport(String),
    Json(String),
    Mdns(String),
    Tls(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Malformed (detail) => write!(f, "malformed cast frame: {detail}"),
            Error::NeedMore => write!(f, "incomplete cast frame"),
            Error::UnexpectedType (detail) => write!(f, "unexpected cast payload: {detail}"),
            Error::WrongState (detail) => write!(f, "wrong cast session state: {detail}"),
            Error::Transport (detail) => write!(f, "cast transport failed: {detail}"),
            Error::Json (detail) => write!(f, "cast json failed: {detail}"),
            Error::Mdns (detail) => write!(f, "cast discovery failed: {detail}"),
            Error::Tls (detail) => write!(f, "cast tls failed: {detail}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Error::Json(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
