use std::fmt;

pub mod auth;
pub mod catalog;
pub mod lyrics;
pub mod stream;

pub use auth::{Client, Credentials, DeviceLink, Endpoints, Poll, TokenSet};
pub use catalog::search_tracks;
pub use lyrics::{LyricsPayload, fetch_lyrics};
pub use stream::{ManifestKind, StreamResolution, resolve_stream};

#[derive(Debug)]
pub enum Error {
    Transport(String),
    Status(u16),
    Protocol(String),
    Auth(String),
    NotLoggedIn,
    LoginExpired,
    Unavailable(String),
    Drm,
    InvalidId(String),
    Secrets(String),
    Io(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Transport(detail) => write!(f, "tidal transport: {detail}"),
            Error::Status(code) => write!(f, "tidal status: {code}"),
            Error::Protocol(detail) => write!(f, "tidal response: {detail}"),
            Error::Auth(detail) => write!(f, "tidal auth: {detail}"),
            Error::NotLoggedIn => write!(f, "tidal login required"),
            Error::LoginExpired => write!(f, "tidal login expired"),
            Error::Unavailable(detail) => write!(f, "tidal unavailable: {detail}"),
            Error::Drm => write!(f, "tidal stream is DRM protected"),
            Error::InvalidId(value) => write!(f, "invalid tidal id: {value}"),
            Error::Secrets(detail) => write!(f, "tidal secrets: {detail}"),
            Error::Io(detail) => write!(f, "tidal io: {detail}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Error {
        Error::Protocol(err.to_string())
    }
}

pub fn body_json(response: reqwest::blocking::Response) -> Result<serde_json::Value, Error> {
    response
        .json::<serde_json::Value>()
        .map_err(|err| Error::Protocol(err.to_string()))
}
