use madrigal_core::Track;
use rusqlite::params;

use crate::Error;

pub const TABLE_DDL: &str = "CREATE TABLE IF NOT EXISTS spotify_mappings (
    spotify_id TEXT PRIMARY KEY,
    youtube_id TEXT NOT NULL,
    resolved TEXT NOT NULL,
    updated_at INTEGER NOT NULL
)";

pub fn ensure_table(conn: &rusqlite::Connection) -> Result<(), Error> {
    conn.execute_batch(TABLE_DDL)
        .map_err(|err| Error::Db(err.to_string()))
}

pub fn lookup_mapping(
    conn: &rusqlite::Connection,
    spotify_id: &str,
) -> Result<Option<Track>, Error> {
    ensure_table(conn)?;
    let mut statement = conn
        .prepare("SELECT resolved FROM spotify_mappings WHERE spotify_id = ?1")
        .map_err(|err| Error::Db(err.to_string()))?;
    let mut rows = statement
        .query([spotify_id])
        .map_err(|err| Error::Db(err.to_string()))?;
    match rows.next().map_err(|err| Error::Db(err.to_string()))? {
        Some(row) => {
            let raw: String = row.get(0).map_err(|err| Error::Db(err.to_string()))?;
            serde_json::from_str(&raw).map(Some).map_err(|err| Error::Db(err.to_string()))
        }
        None => Ok(None),
    }
}

pub fn store_mapping(
    conn: &rusqlite::Connection,
    spotify_id: &str,
    track: &Track,
) -> Result<(), Error> {
    ensure_table(conn)?;
    let raw = serde_json::to_string(track).map_err(|err| Error::Db(err.to_string()))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|span| span.as_secs() as i64)
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO spotify_mappings (spotify_id, youtube_id, resolved, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(spotify_id) DO UPDATE SET youtube_id = excluded.youtube_id,
             resolved = excluded.resolved, updated_at = excluded.updated_at",
        params![spotify_id, track.identity.id, raw, now],
    )
    .map_err(|err| Error::Db(err.to_string()))?;
    Ok(())
}
