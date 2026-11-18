mod history;
mod playlists;
mod songs;
mod stats;

pub use playlists::{AddReport, AutoPlaylist, DuplicatePolicy, PlaylistListFilter};
pub use songs::{SongScope, SongSort, SortDir};
pub use stats::{Stats, Streaks, TopAlbum, TopArtist, TopTrack, YearInMusic};

use std::cell::RefCell;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

pub const LIKED_PLAYLIST_ID: &str = "auto:liked";
pub const DOWNLOADED_PLAYLIST_ID: &str = "auto:downloaded";
pub const HIDDEN_PRIVACY: &str = "hidden";

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug)]
pub enum Error {
    Db(madrigal_db::Error),
    Invalid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Db(err) => write!(f, "database error: {err}"),
            Error::Invalid(msg) => write!(f, "invalid library operation: {msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Db(err) => Some(err),
            Error::Invalid(_) => None,
        }
    }
}

impl From<madrigal_db::Error> for Error {
    fn from(err: madrigal_db::Error) -> Self {
        Error::Db(err)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Error::Db(madrigal_db::Error::from(err))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Library<'a> {
    conn: &'a rusqlite::Connection,
    settings_source: Box<dyn Fn() -> madrigal_settings::Settings + 'a>,
    undo: RefCell<Vec<playlists::UndoOp>>,
    id_counter: RefCell<u64>,
}

impl<'a> Library<'a> {
    pub fn new(conn: &'a rusqlite::Connection, settings: madrigal_settings::Settings) -> Self {
        Self::with_settings_source(conn, move || settings.clone())
    }

    pub fn with_settings_source(
        conn: &'a rusqlite::Connection,
        source: impl Fn() -> madrigal_settings::Settings + 'a,
    ) -> Self {
        Self {
            conn,
            settings_source: Box::new(source),
            undo: RefCell::new(Vec::new()),
            id_counter: RefCell::new(0),
        }
    }

    pub fn connection(&self) -> &'a rusqlite::Connection {
        self.conn
    }

    fn settings(&self) -> madrigal_settings::Settings {
        (self.settings_source)()
    }

    fn history_cutoff(&self) -> i64 {
        let days = self.settings().privacy.history_duration_days.clamp(5, 60) as i64;
        now_unix().saturating_sub(days * 86_400)
    }

    fn next_local_id(&self, prefix: &str) -> Result<madrigal_core::PlaylistId> {
        for _ in 0..1024 {
            let mut counter = self.id_counter.borrow_mut();
            *counter = counter.wrapping_add(1);
            let candidate =
                madrigal_core::PlaylistId::new(format!("{prefix}-{}-{counter}", now_unix()));
            drop(counter);
            if self.get_playlist(&candidate)?.is_none() {
                return Ok(candidate);
            }
        }
        Err(Error::Invalid("could not allocate playlist id".to_owned()))
    }
}
