pub mod backup;
pub mod exchange;
pub mod schedule;

pub use backup::{
    BACKUP_FORMAT, BACKUP_VERSION, BackupSnapshot, CoverPayload, DownloadBackup, LyricsBackup,
    PlaylistBackup, RestoreOptions, RestoreReport, read_archive, restore, snapshot,
    snapshot_to, snapshot_with_covers,
};
pub use exchange::{
    ARTIST_SEPARATOR, CSV_COLUMNS, JSON_FORMAT, JSON_VERSION, LEGACY_JSON_FORMAT,
    MAX_COVER_BYTES, MAX_FILE_BYTES, MAX_TRACKS, CoverSummary, ExchangeError, ExchangeIssue,
    ExchangeReport, ImportDiff, PlaylistMeta, Severity, TrackEntry, build_json_payload,
    diff_against, export_csv, export_json, load_csv, load_json, parse_csv, parse_json,
    preview_csv, preview_file, preview_json, serialize_csv, serialize_json, validate_csv,
    validate_json,
};
pub use schedule::{
    ARCHIVE_FILE_NAME, Frequency, OverwritePolicy, ScheduleConfig, destination_for, is_due,
    mark_ran, next_run, poll,
};

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Db(madrigal_db::Error),
    Json(serde_json::Error),
    UnsupportedVersion(u32),
    Corrupt(String),
    Limit(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(err) => write!(f, "backup io error: {err}"),
            Error::Db(err) => write!(f, "backup db error: {err}"),
            Error::Json(err) => write!(f, "backup json error: {err}"),
            Error::UnsupportedVersion(v) => write!(f, "unsupported backup version: {v}"),
            Error::Corrupt(msg) => write!(f, "corrupt backup: {msg}"),
            Error::Limit(msg) => write!(f, "backup limit exceeded: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<madrigal_db::Error> for Error {
    fn from(err: madrigal_db::Error) -> Self {
        Error::Db(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Json(err)
    }
}

impl From<Error> for ExchangeError {
    fn from(err: Error) -> Self {
        match err {
            Error::Io(source) => ExchangeError::Io(source),
            Error::Limit(detail) => ExchangeError::Limit(detail),
            other => ExchangeError::Format(other.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
