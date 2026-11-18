use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use madrigal_core::{
    Album, Artist, ArtistId, Event, Playlist, PlaylistId, PlaylistKind, Recognition,
    SearchHistoryItem, Track, TrackIdentity, TrackSource,
};use rusqlite::{params, Connection, ErrorCode, OptionalExtension};

pub const SQLITE_MAGIC: &[u8] = b"SQLite format 3\x00";
pub const PAGE_SIZE: u64 = 4096;

const MIGRATION_V1: &str = "
CREATE TABLE songs (
    key TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    track_id TEXT NOT NULL,
    title TEXT NOT NULL,
    artists TEXT NOT NULL DEFAULT '[]',
    album TEXT,
    duration_secs INTEGER,
    thumbnail TEXT,
    track_number INTEGER,
    local_path TEXT,
    set_video_id TEXT
);
CREATE INDEX songs_title ON songs(title);
CREATE TABLE artists (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    subscribers TEXT,
    thumbnail TEXT,
    description TEXT
);
CREATE TABLE albums (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    artists TEXT NOT NULL DEFAULT '[]',
    artist_id TEXT,
    year TEXT,
    thumbnail TEXT,
    track_count INTEGER,
    audio_playlist_id TEXT
);
CREATE TABLE playlists (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL DEFAULT 'local',
    title TEXT NOT NULL,
    author TEXT,
    description TEXT,
    thumbnail TEXT,
    track_count INTEGER,
    owned INTEGER NOT NULL DEFAULT 0,
    privacy TEXT,
    author_id TEXT
);
CREATE TABLE playlist_tracks (
    playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    song_key TEXT NOT NULL,
    position INTEGER NOT NULL,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (playlist_id, song_key)
);
CREATE INDEX playlist_tracks_position ON playlist_tracks(playlist_id, position);
CREATE TABLE tags (
    name TEXT PRIMARY KEY
);
CREATE TABLE playlist_tags (
    playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    tag TEXT NOT NULL REFERENCES tags(name) ON DELETE CASCADE,
    PRIMARY KEY (playlist_id, tag)
);
CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    track_key TEXT NOT NULL,
    played_at INTEGER NOT NULL,
    duration_secs INTEGER,
    completed INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX events_played_at ON events(played_at DESC);
CREATE TABLE downloads (
    song_key TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    title TEXT NOT NULL,
    artists TEXT NOT NULL DEFAULT '[]',
    album TEXT,
    duration_secs INTEGER,
    thumbnail TEXT,
    size_bytes INTEGER,
    created_at INTEGER NOT NULL
);
CREATE TABLE lyrics_cache (
    song_key TEXT PRIMARY KEY,
    lyrics TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE search_history (
    query TEXT PRIMARY KEY,
    searched_at INTEGER NOT NULL
);
CREATE INDEX search_history_searched_at ON search_history(searched_at DESC);
CREATE TABLE recognitions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    artists TEXT NOT NULL DEFAULT '[]',
    album TEXT,
    score REAL NOT NULL,
    track_key TEXT
);
";

static MIGRATION_STEPS: &[rusqlite_migration::M<'_>] =
    &[rusqlite_migration::M::up(MIGRATION_V1)];

fn migrations() -> rusqlite_migration::Migrations<'static> {
    rusqlite_migration::Migrations::from_slice(MIGRATION_STEPS)
}

pub fn latest_version() -> usize {
    MIGRATION_STEPS.len()
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Sqlite(rusqlite::Error),
    Migration(rusqlite_migration::Error),
    Json(serde_json::Error),
    Corrupt(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(err) => write!(f, "io error: {err}"),
            Error::Sqlite(err) => write!(f, "sqlite error: {err}"),
            Error::Migration(err) => write!(f, "migration error: {err}"),
            Error::Json(err) => write!(f, "json error: {err}"),
            Error::Corrupt(msg) => write!(f, "corrupt database: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Error::Sqlite(err)
    }
}

impl From<rusqlite_migration::Error> for Error {
    fn from(err: rusqlite_migration::Error) -> Self {
        Error::Migration(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Json(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

fn valid_page_size(raw: [u8; 2]) -> bool {
    let mut value = u16::from_be_bytes(raw) as u64;
    if value == 1 {
        value = 65536;
    }
    (512..=65536).contains(&value) && value & (value - 1) == 0
}

fn sane_header(header: &[u8]) -> bool {
    if header.len() < 100 {
        return false;
    }
    if !valid_page_size([header[16], header[17]]) {
        return false;
    }
    if !matches!((header[18], header[19]), (1, 1) | (2, 2)) {
        return false;
    }
    header[21] == 64 && header[22] == 32 && header[23] == 32
}

fn synthesized_header(size: u64) -> [u8; 100] {
    let mut header = [0u8; 100];
    header[0..16].copy_from_slice(SQLITE_MAGIC);
    header[16..18].copy_from_slice(&(PAGE_SIZE as u16).to_be_bytes());
    header[18] = 2;
    header[19] = 2;
    header[21] = 64;
    header[22] = 32;
    header[23] = 32;
    header[24..28].copy_from_slice(&1u32.to_be_bytes());
    header[28..32].copy_from_slice(&((size / PAGE_SIZE) as u32).to_be_bytes());
    header[44..48].copy_from_slice(&4u32.to_be_bytes());
    header[56..60].copy_from_slice(&1u32.to_be_bytes());
    header[92..96].copy_from_slice(&1u32.to_be_bytes());
    header
}

pub fn repair_damaged_header(target: &Path) -> bool {
    if target.is_symlink() || !target.exists() {
        return false;
    }
    let head = match fs::read(target).map(|data| data[..data.len().min(16)].to_vec()) {
        Ok(head) => head,
        Err(_) => return false,
    };
    if head.is_empty() || head == SQLITE_MAGIC {
        return false;
    }
    let size = match target.metadata() {
        Ok(meta) => meta.len(),
        Err(_) => return false,
    };
    if size < 100 || size % PAGE_SIZE != 0 {
        return false;
    }
    let sample = match fs::read(target) {
        Ok(data) => data[..data.len().min(128 * 1024)].to_vec(),
        Err(_) => return false,
    };
    if !sample
        .windows(b"CREATE TABLE".len())
        .any(|w| w == b"CREATE TABLE")
        && !sample
            .windows(b"sqlite_master".len())
            .any(|w| w == b"sqlite_master")
    {
        return false;
    }
    let raw = match fs::read(target) {
        Ok(data) => data[..100.min(data.len())].to_vec(),
        Err(_) => return false,
    };
    let mut header = [0u8; 100];
    header[..raw.len()].copy_from_slice(&raw);
    let fixed = if !sane_header(&header) {
        synthesized_header(size)
    } else {
        header[0..16].copy_from_slice(SQLITE_MAGIC);
        header[28..32].copy_from_slice(&((size / PAGE_SIZE) as u32).to_be_bytes());
        header[92..96].copy_from_slice(&1u32.to_be_bytes());
        header
    };
    let mut file = match fs::OpenOptions::new().read(true).write(true).open(target) {
        Ok(file) => file,
        Err(_) => return false,
    };
    use std::io::{Seek, SeekFrom, Write};
    if file.seek(SeekFrom::Start(0)).is_err() || file.write_all(&fixed).is_err() {
        return false;
    }
    for suffix in ["-wal", "-shm"] {
        let stale = target.with_file_name(format!(
            "{}{suffix}",
            target.file_name().unwrap_or_default().to_string_lossy()
        ));
        let _ = fs::remove_file(stale);
    }
    true
}

fn quarantine_stamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_owned())
}

pub fn quarantine(target: &Path) -> Option<PathBuf> {
    let stamp = quarantine_stamp();
    let name = target.file_name()?.to_string_lossy().into_owned();
    let mut moved = None;
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let source = target.with_file_name(format!("{name}{suffix}"));
        if !source.exists() {
            continue;
        }
        let destination = target.with_file_name(format!("{name}{suffix}.corrupt-{stamp}"));
        if fs::rename(&source, &destination).is_ok() && suffix.is_empty() {
            moved = Some(destination);
        }
    }
    moved
}

fn is_corruption(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == ErrorCode::DatabaseCorrupt
                || code.code == ErrorCode::NotADatabase
    )
}

fn quick_check(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA quick_check")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let value: String = row.get(0)?;
        if value != "ok" {
            return Err(Error::Corrupt(value));
        }
    }
    Ok(())
}

fn connect_inner(path: &Path) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    quick_check(&conn)?;
    migrations().to_latest(&mut conn)?;
    secure_file(path);
    Ok(conn)
}

#[cfg(unix)]
fn secure_file(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    for candidate in [
        path.to_path_buf(),
        path.with_extension("sqlite3-wal"),
        path.with_extension("sqlite3-shm"),
    ] {
        if candidate.exists() {
            let _ = fs::set_permissions(&candidate, fs::Permissions::from_mode(0o600));
        }
    }
}

#[cfg(not(unix))]
fn secure_file(_path: &Path) {}

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let _ = repair_damaged_header(path);
    match connect_inner(path) {
        Ok(conn) => Ok(conn),
        Err(Error::Sqlite(err)) if is_corruption(&err) => {
            quarantine(path);
            connect_inner(path)
        }
        Err(Error::Corrupt(_)) => {
            quarantine(path);
            connect_inner(path)
        }
        Err(err) => Err(err),
    }
}

pub fn open_default() -> Result<Connection> {
    madrigal_paths::ensure_dirs()?;
    open(&madrigal_paths::db_path())
}

fn encode_artists(artists: &[String]) -> Result<String> {
    Ok(serde_json::to_string(artists)?)
}

fn decode_artists(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn row_track(row: &rusqlite::Row<'_>) -> rusqlite::Result<Track> {
    let source_raw: String = row.get("source")?;
    let track_id: String = row.get("track_id")?;
    let source = match source_raw.as_str() {
        "tidal" => TrackSource::Tidal,
        "local" => TrackSource::Local,
        _ => TrackSource::Youtube,
    };
    Ok(Track {
        identity: TrackIdentity::new(source, track_id),
        title: row.get("title")?,
        artists: decode_artists(&row.get::<_, String>("artists")?),
        album: row.get("album")?,
        duration_secs: row.get("duration_secs")?,
        thumbnail: row.get("thumbnail")?,
        track_number: row.get("track_number")?,
        local_path: row.get("local_path")?,
        set_video_id: row.get("set_video_id")?,
    })
}

pub fn upsert_song(conn: &Connection, track: &Track) -> Result<()> {
    conn.execute(
        "INSERT INTO songs (key, source, track_id, title, artists, album, duration_secs, thumbnail, track_number, local_path, set_video_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(key) DO UPDATE SET source = excluded.source, track_id = excluded.track_id,
           title = excluded.title, artists = excluded.artists, album = excluded.album,
           duration_secs = excluded.duration_secs, thumbnail = excluded.thumbnail,
           track_number = excluded.track_number, local_path = excluded.local_path,
           set_video_id = excluded.set_video_id",
        params![
            track.identity.to_storage_string(),
            track.identity.source.as_str(),
            track.identity.id,
            track.title,
            encode_artists(&track.artists)?,
            track.album,
            track.duration_secs,
            track.thumbnail,
            track.track_number,
            track.local_path,
            track.set_video_id,
        ],
    )?;
    Ok(())
}

pub fn fetch_song(conn: &Connection, identity: &TrackIdentity) -> Result<Option<Track>> {
    conn.query_row(
        "SELECT source, track_id, title, artists, album, duration_secs, thumbnail, track_number, local_path, set_video_id FROM songs WHERE key = ?1",
        params![identity.to_storage_string()],
        row_track,
    )
    .optional()
    .map_err(Error::from)
}

pub fn list_songs(conn: &Connection) -> Result<Vec<Track>> {
    let mut stmt = conn.prepare(
        "SELECT source, track_id, title, artists, album, duration_secs, thumbnail, track_number, local_path, set_video_id FROM songs ORDER BY title",
    )?;
    let tracks = stmt
        .query_map([], row_track)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(tracks)
}

pub fn upsert_artist(conn: &Connection, artist: &Artist) -> Result<()> {
    conn.execute(
        "INSERT INTO artists (id, name, subscribers, thumbnail, description) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, subscribers = excluded.subscribers,
           thumbnail = excluded.thumbnail, description = excluded.description",
        params![
            artist.id.as_str(),
            artist.name,
            artist.subscribers,
            artist.thumbnail,
            artist.description,
        ],
    )?;
    Ok(())
}

pub fn upsert_album(conn: &Connection, album: &Album) -> Result<()> {
    conn.execute(
        "INSERT INTO albums (id, title, artists, artist_id, year, thumbnail, track_count, audio_playlist_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET title = excluded.title, artists = excluded.artists,
           artist_id = excluded.artist_id, year = excluded.year, thumbnail = excluded.thumbnail,
           track_count = excluded.track_count, audio_playlist_id = excluded.audio_playlist_id",
        params![
            album.id.as_str(),
            album.title,
            encode_artists(&album.artists)?,
            album.artist_id.as_ref().map(ArtistId::as_str),
            album.year,
            album.thumbnail,
            album.track_count,
            album.audio_playlist_id,
        ],
    )?;
    Ok(())
}

pub fn upsert_playlist(conn: &Connection, playlist: &Playlist) -> Result<()> {
    conn.execute(
        "INSERT INTO playlists (id, kind, title, author, description, thumbnail, track_count, owned, privacy, author_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(id) DO UPDATE SET kind = excluded.kind, title = excluded.title,
           author = excluded.author, description = excluded.description, thumbnail = excluded.thumbnail,
           track_count = excluded.track_count, owned = excluded.owned, privacy = excluded.privacy,
           author_id = excluded.author_id",
        params![
            playlist.id.as_str(),
            playlist.kind.as_str(),
            playlist.title,
            playlist.author,
            playlist.description,
            playlist.thumbnail,
            playlist.track_count,
            playlist.owned as i64,
            playlist.privacy,
            playlist.author_id,
        ],
    )?;
    Ok(())
}

pub fn set_playlist_tracks(
    conn: &Connection,
    playlist: &PlaylistId,
    tracks: &[TrackIdentity],
    added_at: i64,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
        params![playlist.as_str()],
    )?;
    for (position, identity) in tracks.iter().enumerate() {
        tx.execute(
            "INSERT INTO playlist_tracks (playlist_id, song_key, position, added_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                playlist.as_str(),
                identity.to_storage_string(),
                position as i64,
                added_at
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn fetch_playlist_tracks(conn: &Connection, playlist: &PlaylistId) -> Result<Vec<Track>> {
    let mut stmt = conn.prepare(
        "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail,
                s.track_number, s.local_path, s.set_video_id
         FROM playlist_tracks p JOIN songs s ON s.key = p.song_key
         WHERE p.playlist_id = ?1 ORDER BY p.position",
    )?;
    let tracks = stmt
        .query_map(params![playlist.as_str()], row_track)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(tracks)
}

pub fn tag_playlist(conn: &Connection, playlist: &PlaylistId, tag: &str) -> Result<()> {
    conn.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", params![tag])?;
    conn.execute(
        "INSERT OR IGNORE INTO playlist_tags (playlist_id, tag) VALUES (?1, ?2)",
        params![playlist.as_str(), tag],
    )?;
    Ok(())
}

pub fn record_event(conn: &Connection, event: &Event) -> Result<()> {
    conn.execute(
        "INSERT INTO events (track_key, played_at, duration_secs, completed) VALUES (?1, ?2, ?3, ?4)",
        params![
            event.track.to_storage_string(),
            event.played_at,
            event.duration_secs,
            event.completed as i64,
        ],
    )?;
    Ok(())
}

pub fn recent_events(conn: &Connection, limit: usize) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare(
        "SELECT track_key, played_at, duration_secs, completed FROM events ORDER BY played_at DESC LIMIT ?1",
    )?;
    let events = stmt
        .query_map(params![limit as i64], |row| {
            let key: String = row.get("track_key")?;
            let completed: i64 = row.get("completed")?;
            Ok(Event {
                track: TrackIdentity::parse_prefixed(&key),
                played_at: row.get("played_at")?,
                duration_secs: row.get("duration_secs")?,
                completed: completed != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(events)
}

pub fn upsert_download(
    conn: &Connection,
    identity: &TrackIdentity,
    path: &Path,
    track: &Track,
    size_bytes: Option<u64>,
    created_at: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO downloads (song_key, path, title, artists, album, duration_secs, thumbnail, size_bytes, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(song_key) DO UPDATE SET path = excluded.path, title = excluded.title,
           artists = excluded.artists, album = excluded.album, duration_secs = excluded.duration_secs,
           thumbnail = excluded.thumbnail, size_bytes = excluded.size_bytes, created_at = excluded.created_at",
        params![
            identity.to_storage_string(),
            path.to_string_lossy(),
            track.title,
            encode_artists(&track.artists)?,
            track.album,
            track.duration_secs,
            track.thumbnail,
            size_bytes.map(|v| v as i64),
            created_at,
        ],
    )?;
    Ok(())
}

pub fn cache_lyrics(
    conn: &Connection,
    identity: &TrackIdentity,
    lyrics: &str,
    updated_at: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO lyrics_cache (song_key, lyrics, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(song_key) DO UPDATE SET lyrics = excluded.lyrics, updated_at = excluded.updated_at",
        params![identity.to_storage_string(), lyrics, updated_at],
    )?;
    Ok(())
}

pub fn fetch_lyrics(conn: &Connection, identity: &TrackIdentity) -> Result<Option<String>> {
    conn.query_row(
        "SELECT lyrics FROM lyrics_cache WHERE song_key = ?1",
        params![identity.to_storage_string()],
        |row| row.get(0),
    )
    .optional()
    .map_err(Error::from)
}

pub fn record_search(conn: &Connection, item: &SearchHistoryItem) -> Result<()> {
    conn.execute(
        "INSERT INTO search_history (query, searched_at) VALUES (?1, ?2)
         ON CONFLICT(query) DO UPDATE SET searched_at = excluded.searched_at",
        params![item.query, item.searched_at],
    )?;
    Ok(())
}

pub fn record_recognition(conn: &Connection, recognition: &Recognition) -> Result<()> {
    conn.execute(
        "INSERT INTO recognitions (title, artists, album, score, track_key) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            recognition.title,
            encode_artists(&recognition.artists)?,
            recognition.album,
            recognition.score,
            recognition.track.as_ref().map(|t| t.to_storage_string()),
        ],
    )?;
    Ok(())
}

pub fn playlist_kind(raw: &str) -> PlaylistKind {
    match raw {
        "remote" => PlaylistKind::Remote,
        "auto" => PlaylistKind::Auto,
        "spotify" => PlaylistKind::Spotify,
        _ => PlaylistKind::Local,
    }
}

pub fn user_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}
