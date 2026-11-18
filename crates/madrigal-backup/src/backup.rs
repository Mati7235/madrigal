use std::collections::HashMap;
use std::path::{Path, PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use madrigal_core::{
    Album, Artist, Event, Playlist, PlaylistId, Recognition, SearchHistoryItem, Track,
    TrackIdentity,
};
use madrigal_settings::Settings;
use rusqlite::{OptionalExtension, params};

use crate::{Error, Result};

pub const BACKUP_FORMAT: &str = "madrigal-backup";
pub const BACKUP_VERSION: u32 = 1;
pub const BACKUP_MAX_BYTES: u64 = 256 * 1024 * 1024;
pub const COVER_MAX_BYTES: usize = 5 * 1024 * 1024;

const COVER_MIME: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
    ("bmp", "image/bmp"),
];

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CoverPayload {
    pub name: String,
    pub mime: String,
    pub bytes: u64,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlaylistBackup {
    pub playlist: Playlist,
    pub tracks: Vec<String>,
    pub tags: Vec<String>,
    pub cover: Option<CoverPayload>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DownloadBackup {
    pub key: String,
    pub path: String,
    pub track: Track,
    pub size_bytes: Option<u64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LyricsBackup {
    pub key: String,
    pub lyrics: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BackupSnapshot {
    pub format: String,
    pub version: u32,
    pub created_at: i64,
    pub settings: serde_json::Value,
    pub songs: Vec<Track>,
    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub playlists: Vec<PlaylistBackup>,
    pub events: Vec<Event>,
    pub downloads: Vec<DownloadBackup>,
    pub lyrics: Vec<LyricsBackup>,
    pub search_history: Vec<SearchHistoryItem>,
    pub recognitions: Vec<Recognition>,
}

#[derive(Debug, Clone, Default)]
pub struct RestoreOptions {
    pub overwrite_settings: bool,
    pub redownload_missing: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RestoreReport {
    pub songs_added: usize,
    pub songs_kept: usize,
    pub playlists_added: usize,
    pub playlists_kept: usize,
    pub events_added: usize,
    pub covers_restored: Vec<PathBuf>,
    pub missing_local_paths: Vec<String>,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn cover_mime(name: &str) -> String {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    COVER_MIME
        .iter()
        .find(|(key, _)| *key == ext)
        .map(|(_, mime)| (*mime).to_owned())
        .unwrap_or_else(|| "application/octet-stream".to_owned())
}

fn encode_cover(path: &Path) -> Result<CoverPayload> {
    let bytes = std::fs::read(path)?;
    if bytes.len() > COVER_MAX_BYTES {
        return Err(Error::Limit(format!("cover exceeds {COVER_MAX_BYTES} bytes")));
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "cover".to_owned());
    Ok(CoverPayload {
        mime: cover_mime(&name),
        bytes: bytes.len() as u64,
        data: STANDARD.encode(&bytes),
        name,
    })
}

fn decode_cover(payload: &CoverPayload) -> Result<Vec<u8>> {
    if payload.bytes > COVER_MAX_BYTES as u64 {
        return Err(Error::Limit("cover exceeds size cap".to_owned()));
    }
    let raw = STANDARD
        .decode(payload.data.as_bytes())
        .map_err(|err| Error::Corrupt(format!("cover data invalid: {err}")))?;
    if raw.len() as u64 != payload.bytes {
        return Err(Error::Corrupt("cover size mismatch".to_owned()));
    }
    Ok(raw)
}

fn dump_playlists(conn: &rusqlite::Connection) -> Result<Vec<PlaylistBackup>> {
    let mut stmt = conn
        .prepare("SELECT id, kind, title, author, description, thumbnail, track_count, owned, privacy, author_id FROM playlists ORDER BY id")
        .map_err(madrigal_db::Error::from)?;
    let playlists = stmt
        .query_map([], |row| {
            let kind_raw: String = row.get("kind")?;
            let owned: i64 = row.get("owned")?;
            Ok(Playlist {
                id: PlaylistId::new(row.get::<_, String>("id")?),
                kind: madrigal_db::playlist_kind(&kind_raw),
                title: row.get("title")?,
                author: row.get("author")?,
                description: row.get("description")?,
                thumbnail: row.get("thumbnail")?,
                track_count: row.get("track_count")?,
                owned: owned != 0,
                privacy: row.get("privacy")?,
                author_id: row.get("author_id")?,
            })
        })
        .map_err(madrigal_db::Error::from)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(madrigal_db::Error::from)?;
    let mut out = Vec::with_capacity(playlists.len());
    for playlist in playlists {
        let mut tracks_stmt = conn
            .prepare("SELECT song_key FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
            .map_err(madrigal_db::Error::from)?;
        let tracks = tracks_stmt
            .query_map(params![playlist.id.as_str()], |row| row.get(0))
            .map_err(madrigal_db::Error::from)?
            .collect::<rusqlite::Result<Vec<String>>>()
            .map_err(madrigal_db::Error::from)?;
        let mut tags_stmt = conn
            .prepare("SELECT tag FROM playlist_tags WHERE playlist_id = ?1 ORDER BY tag")
            .map_err(madrigal_db::Error::from)?;
        let tags = tags_stmt
            .query_map(params![playlist.id.as_str()], |row| row.get(0))
            .map_err(madrigal_db::Error::from)?
            .collect::<rusqlite::Result<Vec<String>>>()
            .map_err(madrigal_db::Error::from)?;
        out.push(PlaylistBackup {
            playlist,
            tracks,
            tags,
            cover: None,
        });
    }
    Ok(out)
}

fn dump_downloads(conn: &rusqlite::Connection) -> Result<Vec<DownloadBackup>> {
    let mut stmt = conn
        .prepare("SELECT song_key, path, title, artists, album, duration_secs, thumbnail, size_bytes, created_at FROM downloads ORDER BY song_key")
        .map_err(madrigal_db::Error::from)?;
    let rows = stmt
        .query_map([], |row| {
            let key: String = row.get("song_key")?;
            let identity = TrackIdentity::parse_prefixed(&key);
            let artists_raw: String = row.get("artists")?;
            let artists: Vec<String> = serde_json::from_str(&artists_raw).unwrap_or_default();
            Ok(DownloadBackup {
                key,
                path: row.get("path")?,
                track: Track {
                    identity,
                    title: row.get("title")?,
                    artists,
                    album: row.get("album")?,
                    duration_secs: row.get("duration_secs")?,
                    thumbnail: row.get("thumbnail")?,
                    track_number: None,
                    local_path: None,
                    set_video_id: None,
                },
                size_bytes: row.get::<_, Option<i64>>("size_bytes")?.map(|v| v as u64),
                created_at: row.get("created_at")?,
            })
        })
        .map_err(madrigal_db::Error::from)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(madrigal_db::Error::from)?;
    Ok(rows)
}

fn dump_lyrics(conn: &rusqlite::Connection) -> Result<Vec<LyricsBackup>> {
    let mut stmt = conn
        .prepare("SELECT song_key, lyrics, updated_at FROM lyrics_cache ORDER BY song_key")
        .map_err(madrigal_db::Error::from)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(LyricsBackup {
                key: row.get("song_key")?,
                lyrics: row.get("lyrics")?,
                updated_at: row.get("updated_at")?,
            })
        })
        .map_err(madrigal_db::Error::from)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(madrigal_db::Error::from)?;
    Ok(rows)
}

fn dump_events(conn: &rusqlite::Connection) -> Result<Vec<Event>> {
    let mut stmt = conn
        .prepare("SELECT track_key, played_at, duration_secs, completed FROM events ORDER BY id")
        .map_err(madrigal_db::Error::from)?;
    let rows = stmt
        .query_map([], |row| {
            let key: String = row.get("track_key")?;
            let completed: i64 = row.get("completed")?;
            Ok(Event {
                track: TrackIdentity::parse_prefixed(&key),
                played_at: row.get("played_at")?,
                duration_secs: row.get("duration_secs")?,
                completed: completed != 0,
            })
        })
        .map_err(madrigal_db::Error::from)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(madrigal_db::Error::from)?;
    Ok(rows)
}

fn dump_search(conn: &rusqlite::Connection) -> Result<Vec<SearchHistoryItem>> {
    let mut stmt = conn
        .prepare("SELECT query, searched_at FROM search_history ORDER BY query")
        .map_err(madrigal_db::Error::from)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SearchHistoryItem {
                query: row.get("query")?,
                searched_at: row.get("searched_at")?,
            })
        })
        .map_err(madrigal_db::Error::from)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(madrigal_db::Error::from)?;
    Ok(rows)
}

fn dump_recognitions(conn: &rusqlite::Connection) -> Result<Vec<Recognition>> {
    let mut stmt = conn
        .prepare("SELECT title, artists, album, score, track_key FROM recognitions ORDER BY id")
        .map_err(madrigal_db::Error::from)?;
    let rows = stmt
        .query_map([], |row| {
            let artists_raw: String = row.get("artists")?;
            let artists: Vec<String> = serde_json::from_str(&artists_raw).unwrap_or_default();
            let key: Option<String> = row.get("track_key")?;
            Ok(Recognition {
                title: row.get("title")?,
                artists,
                album: row.get("album")?,
                score: row.get("score")?,
                track: key.map(|k| TrackIdentity::parse_prefixed(&k)),
            })
        })
        .map_err(madrigal_db::Error::from)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(madrigal_db::Error::from)?;
    Ok(rows)
}

fn dump_artists(conn: &rusqlite::Connection) -> Result<Vec<Artist>> {
    let mut stmt = conn
        .prepare("SELECT id, name, subscribers, thumbnail, description FROM artists ORDER BY id")
        .map_err(madrigal_db::Error::from)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Artist {
                id: madrigal_core::ArtistId::new(row.get::<_, String>("id")?),
                name: row.get("name")?,
                subscribers: row.get("subscribers")?,
                thumbnail: row.get("thumbnail")?,
                description: row.get("description")?,
            })
        })
        .map_err(madrigal_db::Error::from)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(madrigal_db::Error::from)?;
    Ok(rows)
}

fn dump_albums(conn: &rusqlite::Connection) -> Result<Vec<Album>> {
    let mut stmt = conn
        .prepare("SELECT id, title, artists, artist_id, year, thumbnail, track_count, audio_playlist_id FROM albums ORDER BY id")
        .map_err(madrigal_db::Error::from)?;
    let rows = stmt
        .query_map([], |row| {
            let artists_raw: String = row.get("artists")?;
            let artists: Vec<String> = serde_json::from_str(&artists_raw).unwrap_or_default();
            let artist_id: Option<String> = row.get("artist_id")?;
            Ok(Album {
                id: madrigal_core::AlbumId::new(row.get::<_, String>("id")?),
                title: row.get("title")?,
                artists,
                artist_id: artist_id.map(madrigal_core::ArtistId::new),
                year: row.get("year")?,
                thumbnail: row.get("thumbnail")?,
                track_count: row.get("track_count")?,
                audio_playlist_id: row.get("audio_playlist_id")?,
            })
        })
        .map_err(madrigal_db::Error::from)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(madrigal_db::Error::from)?;
    Ok(rows)
}

pub fn snapshot(db_path: &Path, settings: &Settings) -> Result<BackupSnapshot> {
    snapshot_with_covers(db_path, settings, &HashMap::new())
}

pub fn snapshot_with_covers(
    db_path: &Path,
    settings: &Settings,
    covers: &HashMap<String, PathBuf>,
) -> Result<BackupSnapshot> {
    let conn = madrigal_db::open(db_path)?;
    let settings_value = serde_json::to_value(settings)?;
    let mut playlists = dump_playlists(&conn)?;
    for entry in &mut playlists {
        if let Some(path) = covers.get(entry.playlist.id.as_str()) {
            entry.cover = Some(encode_cover(path)?);
        }
    }
    Ok(BackupSnapshot {
        format: BACKUP_FORMAT.to_owned(),
        version: BACKUP_VERSION,
        created_at: now_secs(),
        settings: settings_value,
        songs: madrigal_db::list_songs(&conn)?,
        artists: dump_artists(&conn)?,
        albums: dump_albums(&conn)?,
        playlists,
        events: dump_events(&conn)?,
        downloads: dump_downloads(&conn)?,
        lyrics: dump_lyrics(&conn)?,
        search_history: dump_search(&conn)?,
        recognitions: dump_recognitions(&conn)?,
    })
}

pub fn snapshot_to(
    db_path: &Path,
    settings: &Settings,
    covers: &HashMap<String, PathBuf>,
    dest: &Path,
) -> Result<PathBuf> {
    let snap = snapshot_with_covers(db_path, settings, covers)?;
    write_archive(&snap, dest)
}

pub fn write_archive(snap: &BackupSnapshot, dest: &Path) -> Result<PathBuf> {
    let mut text = serde_json::to_string_pretty(snap)?;
    text.push('\n');
    if text.len() as u64 > BACKUP_MAX_BYTES {
        return Err(Error::Limit("backup exceeds size cap".to_owned()));
    }
    Ok(madrigal_files::atomic_write_text(dest, &text)?)
}

pub fn read_archive(path: &Path) -> Result<BackupSnapshot> {
    if path.is_symlink() {
        return Err(Error::Corrupt("symlink archive rejected".to_owned()));
    }
    let meta = std::fs::metadata(path)?;
    if meta.len() > BACKUP_MAX_BYTES {
        return Err(Error::Limit("backup exceeds size cap".to_owned()));
    }
    let text = std::fs::read_to_string(path)?;
    let snap: BackupSnapshot = serde_json::from_str(&text)?;
    if snap.format != BACKUP_FORMAT {
        return Err(Error::Corrupt(format!("unknown backup format: {}", snap.format)));
    }
    if snap.version > BACKUP_VERSION {
        return Err(Error::UnsupportedVersion(snap.version));
    }
    Ok(snap)
}

fn exists(conn: &rusqlite::Connection, sql: &str, key: &str) -> Result<bool> {
    let hit: Option<i64> = conn
        .query_row(sql, params![key], |row| row.get(0))
        .optional()
        .map_err(madrigal_db::Error::from)?;
    Ok(hit.is_some())
}

fn restore_cover(
    playlist_id: &str,
    cover: &CoverPayload,
) -> Result<PathBuf> {
    let raw = decode_cover(cover)?;
    let dir = madrigal_paths::data_dir().join("covers");
    std::fs::create_dir_all(&dir)?;
    let safe_name: String = cover
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let dest = dir.join(format!("{playlist_id}-{safe_name}"));
    madrigal_files::atomic_write_bytes(&dest, &raw)?;
    Ok(dest)
}

pub fn restore(archive: &Path, options: &RestoreOptions) -> Result<RestoreReport> {
    let snap = read_archive(archive)?;
    madrigal_paths::ensure_dirs()?;
    let db_path = madrigal_paths::db_path();
    let conn = madrigal_db::open(&db_path)?;
    let mut report = RestoreReport::default();

    for song in &snap.songs {
        if madrigal_db::fetch_song(&conn, &song.identity)?.is_some() {
            report.songs_kept += 1;
        } else {
            madrigal_db::upsert_song(&conn, song)?;
            report.songs_added += 1;
        }
    }
    for artist in &snap.artists {
        if !exists(&conn, "SELECT 1 FROM artists WHERE id = ?1", artist.id.as_str())? {
            madrigal_db::upsert_artist(&conn, artist)?;
        }
    }
    for album in &snap.albums {
        if !exists(&conn, "SELECT 1 FROM albums WHERE id = ?1", album.id.as_str())? {
            madrigal_db::upsert_album(&conn, album)?;
        }
    }
    for entry in &snap.playlists {
        if exists(
            &conn,
            "SELECT 1 FROM playlists WHERE id = ?1",
            entry.playlist.id.as_str(),
        )? {
            report.playlists_kept += 1;
            continue;
        }
        let mut playlist = entry.playlist.clone();
        if let Some(cover) = &entry.cover {
            let dest = restore_cover(playlist.id.as_str(), cover)?;
            playlist.thumbnail = Some(dest.to_string_lossy().into_owned());
            report.covers_restored.push(dest);
        }
        madrigal_db::upsert_playlist(&conn, &playlist)?;
        let tracks: Vec<TrackIdentity> = entry
            .tracks
            .iter()
            .map(|key| TrackIdentity::parse_prefixed(key))
            .collect();
        madrigal_db::set_playlist_tracks(&conn, &playlist.id, &tracks, snap.created_at)?;
        for tag in &entry.tags {
            madrigal_db::tag_playlist(&conn, &playlist.id, tag)?;
        }
        report.playlists_added += 1;
    }
    for event in &snap.events {
        let added = conn
            .execute(
                "INSERT INTO events (track_key, played_at, duration_secs, completed)
                 SELECT ?1, ?2, ?3, ?4 WHERE NOT EXISTS
                 (SELECT 1 FROM events WHERE track_key = ?1 AND played_at = ?2)",
                params![
                    event.track.to_storage_string(),
                    event.played_at,
                    event.duration_secs,
                    event.completed as i64,
                ],
            )
            .map_err(madrigal_db::Error::from)?;
        report.events_added += added as usize;
    }
    for item in &snap.search_history {
        conn.execute(
            "INSERT OR IGNORE INTO search_history (query, searched_at) VALUES (?1, ?2)",
            params![item.query, item.searched_at],
        )
        .map_err(madrigal_db::Error::from)?;
    }
    for recognition in &snap.recognitions {
        let artists = serde_json::to_string(&recognition.artists).unwrap_or_default();
        conn.execute(
            "INSERT INTO recognitions (title, artists, album, score, track_key)
             SELECT ?1, ?2, ?3, ?4, ?5 WHERE NOT EXISTS
             (SELECT 1 FROM recognitions WHERE title = ?1 AND artists = ?2 AND score = ?4)",
            params![
                recognition.title,
                artists,
                recognition.album,
                recognition.score,
                recognition.track.as_ref().map(|t| t.to_storage_string()),
            ],
        )
        .map_err(madrigal_db::Error::from)?;
    }
    for download in &snap.downloads {
        conn.execute(
            "INSERT OR IGNORE INTO downloads
             (song_key, path, title, artists, album, duration_secs, thumbnail, size_bytes, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                download.key,
                download.path,
                download.track.title,
                serde_json::to_string(&download.track.artists).unwrap_or_default(),
                download.track.album,
                download.track.duration_secs,
                download.track.thumbnail,
                download.size_bytes.map(|v| v as i64),
                download.created_at,
            ],
        )
        .map_err(madrigal_db::Error::from)?;
    }
    for row in &snap.lyrics {
        conn.execute(
            "INSERT OR IGNORE INTO lyrics_cache (song_key, lyrics, updated_at) VALUES (?1, ?2, ?3)",
            params![row.key, row.lyrics, row.updated_at],
        )
        .map_err(madrigal_db::Error::from)?;
    }

    let settings_path = madrigal_paths::settings_path();
    if options.overwrite_settings || !settings_path.exists() {
        let mut text = serde_json::to_string_pretty(&snap.settings)?;
        text.push('\n');
        madrigal_files::atomic_write_text(&settings_path, &text)?;
    }

    if options.redownload_missing {
        let mut missing: Vec<String> = snap
            .downloads
            .iter()
            .map(|d| d.path.clone())
            .filter(|p| !Path::new(p).exists())
            .collect();
        missing.extend(
            snap.songs
                .iter()
                .filter_map(|s| s.local_path.clone())
                .filter(|p| !Path::new(p).exists()),
        );
        missing.sort();
        missing.dedup();
        report.missing_local_paths = missing;
    }
    Ok(report)
}
