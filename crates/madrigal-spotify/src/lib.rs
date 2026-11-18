use std::fmt;

pub mod auth;
pub mod cache;
pub mod import;
pub mod mapper;
pub mod models;
pub mod playlists;
pub mod totp;

pub use auth::{Client as AuthClient, Cookies, Endpoints as AuthEndpoints, Token};
pub use cache::{lookup_mapping, store_mapping};
pub use import::{ImportOutcome, Search, SpotifySource, UnresolvedTrack, import_playlist};
pub use mapper::{apply_resolution, build_query, match_score, pick_best, top_score};
pub use models::{SpotifyPlaylist, SpotifyTrack};
pub use playlists::{GqlClient, GqlEndpoints};

#[derive(Debug)]
pub enum Error {
    Transport(String),
    Status(u16),
    Protocol(String),
    Auth(String),
    NotLoggedIn,
    Disabled,
    HashRotated(String),
    RateLimited(u64),
    Secrets(String),
    Db(String),
    Io(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Transport(detail) => write!(f, "spotify transport: {detail}"),
            Error::Status(code) => write!(f, "spotify status: {code}"),
            Error::Protocol(detail) => write!(f, "spotify response: {detail}"),
            Error::Auth(detail) => write!(f, "spotify auth: {detail}"),
            Error::NotLoggedIn => write!(f, "spotify login required"),
            Error::Disabled => write!(f, "spotify playlists disabled in settings"),
            Error::HashRotated(operation) => {
                write!(f, "spotify query hash rotated for {operation}")
            }
            Error::RateLimited(wait) => write!(f, "spotify rate limited, retry after {wait}s"),
            Error::Secrets(detail) => write!(f, "spotify secrets: {detail}"),
            Error::Db(detail) => write!(f, "spotify cache: {detail}"),
            Error::Io(detail) => write!(f, "spotify io: {detail}"),
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
