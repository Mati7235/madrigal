use madrigal_core::{Playlist, PlaylistId, PlaylistKind, Track, TrackIdentity};
use madrigal_settings::PlaylistSongSortType;
use rusqlite::params;

use crate::songs::{read_track, SongSort, SortDir};
use crate::{Error, Library, Result, DOWNLOADED_PLAYLIST_ID, HIDDEN_PRIVACY, LIKED_PLAYLIST_ID};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoPlaylist {
    Liked,
    Downloaded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicatePolicy {
    Skip,
    AddAnyway,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddReport {
    pub added: Vec<TrackIdentity>,
    pub skipped: Vec<TrackIdentity>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaylistListFilter {
    pub tag: Option<String>,
    pub include_hidden: bool,
    pub kind: Option<PlaylistKind>,
}

#[derive(Debug, Clone)]
pub(crate) struct UndoRow {
    pub song_key: String,
    pub position: i64,
    pub added_at: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct UndoOp {
    pub playlist: String,
    pub before: Vec<UndoRow>,
}

pub(crate) fn read_playlist(row: &rusqlite::Row<'_>) -> rusqlite::Result<Playlist> {
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
}

fn playlist_track_sql(sort: SongSort) -> &'static str {
    match sort {
        SongSort::Custom => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM playlist_tracks p JOIN songs s ON s.key = p.song_key WHERE p.playlist_id = ?1 ORDER BY p.position ASC LIMIT ?2",
        SongSort::DateAdded => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM playlist_tracks p JOIN songs s ON s.key = p.song_key WHERE p.playlist_id = ?1 ORDER BY p.added_at ASC, p.position ASC LIMIT ?2",
        SongSort::Name => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM playlist_tracks p JOIN songs s ON s.key = p.song_key WHERE p.playlist_id = ?1 ORDER BY s.title COLLATE NOCASE ASC, p.position ASC LIMIT ?2",
        SongSort::Artist => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM playlist_tracks p JOIN songs s ON s.key = p.song_key WHERE p.playlist_id = ?1 ORDER BY s.artists COLLATE NOCASE ASC, s.title COLLATE NOCASE ASC, p.position ASC LIMIT ?2",
        SongSort::PlayCount => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM playlist_tracks p JOIN songs s ON s.key = p.song_key WHERE p.playlist_id = ?1 ORDER BY (SELECT COUNT(*) FROM events e WHERE e.track_key = s.key) ASC, s.title COLLATE NOCASE ASC, p.position ASC LIMIT ?2",
        SongSort::PlayTime => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM playlist_tracks p JOIN songs s ON s.key = p.song_key WHERE p.playlist_id = ?1 ORDER BY (SELECT COALESCE(SUM(e.duration_secs), 0) FROM events e WHERE e.track_key = s.key) ASC, s.title COLLATE NOCASE ASC, p.position ASC LIMIT ?2",
        SongSort::Year => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM playlist_tracks p JOIN songs s ON s.key = p.song_key LEFT JOIN albums a ON a.title = s.album WHERE p.playlist_id = ?1 ORDER BY COALESCE(a.year, '') ASC, s.title COLLATE NOCASE ASC, p.position ASC LIMIT ?2",
        SongSort::Duration => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM playlist_tracks p JOIN songs s ON s.key = p.song_key WHERE p.playlist_id = ?1 ORDER BY s.duration_secs IS NULL ASC, s.duration_secs ASC, p.position ASC LIMIT ?2",
    }
}

fn sort_from_prefs(sort: PlaylistSongSortType) -> SongSort {
    match sort {
        PlaylistSongSortType::Custom => SongSort::Custom,
        PlaylistSongSortType::CreateDate => SongSort::DateAdded,
        PlaylistSongSortType::Name => SongSort::Name,
        PlaylistSongSortType::Artist => SongSort::Artist,
        PlaylistSongSortType::PlayTime => SongSort::PlayTime,
    }
}

impl<'a> Library<'a> {
    pub fn create_playlist(&self, title: &str) -> Result<Playlist> {
        let title = title.trim();
        if title.is_empty() {
            return Err(Error::Invalid("playlist title is empty".to_owned()));
        }
        let playlist = Playlist {
            id: self.next_local_id("local")?,
            kind: PlaylistKind::Local,
            title: title.to_owned(),
            author: None,
            description: None,
            thumbnail: None,
            track_count: None,
            owned: true,
            privacy: None,
            author_id: None,
        };
        madrigal_db::upsert_playlist(self.conn, &playlist)?;
        Ok(playlist)
    }

    pub fn get_playlist(&self, id: &PlaylistId) -> Result<Option<Playlist>> {
        self.conn
            .query_row(
                "SELECT id, kind, title, author, description, thumbnail, track_count, owned, privacy, author_id FROM playlists WHERE id = ?1",
                params![id.as_str()],
                read_playlist,
            )
            .map(Some)
            .or_else(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(Error::from(other)),
            })
    }

    pub fn list_playlists(&self, filter: &PlaylistListFilter) -> Result<Vec<Playlist>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, title, author, description, thumbnail, track_count, owned, privacy, author_id FROM playlists p WHERE (?1 = '' OR EXISTS (SELECT 1 FROM playlist_tags t WHERE t.playlist_id = p.id AND t.tag = ?1)) AND (?2 = 1 OR COALESCE(p.privacy, '') <> 'hidden') AND (?3 = '' OR p.kind = ?3) AND (?4 = 1 OR p.kind <> 'auto') ORDER BY p.rowid ASC",
        )?;
        let tag = filter.tag.as_deref().unwrap_or("");
        let include_hidden: i64 = i64::from(filter.include_hidden);
        let (kind, include_auto): (&str, i64) = match &filter.kind {
            Some(kind) => (kind.as_str(), 1),
            None => ("", 0),
        };
        let playlists = stmt
            .query_map(params![tag, include_hidden, kind, include_auto], read_playlist)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(playlists)
    }

    pub fn rename_playlist(&self, id: &PlaylistId, title: &str) -> Result<bool> {
        let title = title.trim();
        if title.is_empty() {
            return Err(Error::Invalid("playlist title is empty".to_owned()));
        }
        let changed = self.conn.execute(
            "UPDATE playlists SET title = ?1 WHERE id = ?2",
            params![title, id.as_str()],
        )?;
        Ok(changed > 0)
    }

    pub fn set_playlist_description(
        &self,
        id: &PlaylistId,
        description: Option<&str>,
    ) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE playlists SET description = ?1 WHERE id = ?2",
            params![description, id.as_str()],
        )?;
        Ok(changed > 0)
    }

    pub fn set_cover(&self, id: &PlaylistId, cover_path: Option<&str>) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE playlists SET thumbnail = ?1 WHERE id = ?2",
            params![cover_path, id.as_str()],
        )?;
        Ok(changed > 0)
    }

    pub fn set_hidden(&self, id: &PlaylistId, hidden: bool) -> Result<bool> {
        let privacy: Option<&str> = if hidden { Some(HIDDEN_PRIVACY) } else { None };
        let changed = self.conn.execute(
            "UPDATE playlists SET privacy = ?1 WHERE id = ?2",
            params![privacy, id.as_str()],
        )?;
        Ok(changed > 0)
    }

    pub fn is_hidden(&self, id: &PlaylistId) -> Result<bool> {
        Ok(self
            .get_playlist(id)?
            .is_some_and(|playlist| playlist.privacy.as_deref() == Some(HIDDEN_PRIVACY)))
    }

    pub fn delete_playlist(&self, id: &PlaylistId) -> Result<bool> {
        let changed = self.conn.execute(
            "DELETE FROM playlists WHERE id = ?1",
            params![id.as_str()],
        )?;
        Ok(changed > 0)
    }

    pub fn assign_tag(&self, id: &PlaylistId, tag: &str) -> Result<()> {
        let tag = tag.trim();
        if tag.is_empty() {
            return Err(Error::Invalid("playlist tag is empty".to_owned()));
        }
        if self.get_playlist(id)?.is_none() {
            return Err(Error::Invalid("playlist does not exist".to_owned()));
        }
        madrigal_db::tag_playlist(self.conn, id, tag)?;
        Ok(())
    }

    pub fn remove_tag(&self, id: &PlaylistId, tag: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM playlist_tags WHERE playlist_id = ?1 AND tag = ?2",
            params![id.as_str(), tag],
        )?;
        Ok(())
    }

    pub fn playlist_tags(&self, id: &PlaylistId) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT tag FROM playlist_tags WHERE playlist_id = ?1 ORDER BY tag ASC",
        )?;
        let tags = stmt
            .query_map(params![id.as_str()], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(tags)
    }

    pub fn all_tags(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT name FROM tags ORDER BY name ASC")?;
        let tags = stmt
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(tags)
    }

    pub fn playlist_tracks(
        &self,
        id: &PlaylistId,
        sort: SongSort,
        dir: SortDir,
    ) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(playlist_track_sql(sort))?;
        let mut tracks = stmt
            .query_map(params![id.as_str(), -1i64], read_track)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if dir == SortDir::Desc {
            tracks.reverse();
        }
        Ok(tracks)
    }

    pub fn default_playlist_tracks(&self, id: &PlaylistId) -> Result<Vec<Track>> {
        let content = self.settings().content;
        let dir = if content.playlist_song_sort_descending {
            SortDir::Desc
        } else {
            SortDir::Asc
        };
        self.playlist_tracks(id, sort_from_prefs(content.playlist_song_sort_type), dir)
    }

    fn playlist_snapshot(&self, id: &PlaylistId) -> Result<Vec<UndoRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT song_key, position, added_at FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position ASC",
        )?;
        let rows = stmt
            .query_map(params![id.as_str()], |row| {
                Ok(UndoRow {
                    song_key: row.get("song_key")?,
                    position: row.get("position")?,
                    added_at: row.get("added_at")?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn playlist_keys(&self, id: &PlaylistId) -> Result<std::collections::HashSet<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT song_key FROM playlist_tracks WHERE playlist_id = ?1")?;
        let keys = stmt
            .query_map(params![id.as_str()], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<std::collections::HashSet<_>>>()?;
        Ok(keys)
    }

    pub fn add_tracks(
        &self,
        id: &PlaylistId,
        tracks: &[Track],
        policy: DuplicatePolicy,
    ) -> Result<AddReport> {
        if self.get_playlist(id)?.is_none() {
            return Err(Error::Invalid("playlist does not exist".to_owned()));
        }
        let before = self.playlist_snapshot(id)?;
        let mut present = self.playlist_keys(id)?;
        let mut position: i64 = before.iter().map(|row| row.position).max().unwrap_or(-1) + 1;
        let added_at = crate::now_unix();
        let tx = self.conn.unchecked_transaction()?;
        let mut report = AddReport::default();
        for track in tracks {
            madrigal_db::upsert_song(&tx, track)?;
            let key = track.identity.to_storage_string();
            if present.contains(&key) {
                match policy {
                    DuplicatePolicy::Skip => {
                        report.skipped.push(track.identity.clone());
                        continue;
                    }
                    DuplicatePolicy::AddAnyway => {
                        tx.execute(
                            "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND song_key = ?2",
                            params![id.as_str(), key],
                        )?;
                    }
                }
            }
            tx.execute(
                "INSERT INTO playlist_tracks (playlist_id, song_key, position, added_at) VALUES (?1, ?2, ?3, ?4)",
                params![id.as_str(), key, position, added_at],
            )?;
            position += 1;
            present.insert(key);
            report.added.push(track.identity.clone());
        }
        tx.commit()?;
        self.undo.borrow_mut().push(UndoOp {
            playlist: id.as_str().to_owned(),
            before,
        });
        Ok(report)
    }

    pub fn undo_last_add(&self) -> Result<bool> {
        let Some(op) = self.undo.borrow_mut().pop() else {
            return Ok(false);
        };
        let id = PlaylistId::new(op.playlist);
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
            params![id.as_str()],
        )?;
        for row in &op.before {
            tx.execute(
                "INSERT INTO playlist_tracks (playlist_id, song_key, position, added_at) VALUES (?1, ?2, ?3, ?4)",
                params![id.as_str(), row.song_key, row.position, row.added_at],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }

    pub fn move_track(&self, id: &PlaylistId, from: usize, to: usize) -> Result<bool> {
        let order = self.playlist_snapshot(id)?;
        if from >= order.len() || to >= order.len() || from == to {
            return Ok(false);
        }
        let mut order = order;
        let row = order.remove(from);
        order.insert(to, row);
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
            params![id.as_str()],
        )?;
        for (position, row) in order.iter().enumerate() {
            tx.execute(
                "INSERT INTO playlist_tracks (playlist_id, song_key, position, added_at) VALUES (?1, ?2, ?3, ?4)",
                params![id.as_str(), row.song_key, position as i64, row.added_at],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }

    fn ensure_auto(&self, id: &str, title: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO playlists (id, kind, title, owned) VALUES (?1, 'auto', ?2, 0)",
            params![id, title],
        )?;
        Ok(())
    }

    pub fn set_liked(&self, identity: &TrackIdentity, liked: bool) -> Result<()> {
        self.ensure_auto(LIKED_PLAYLIST_ID, "Liked")?;
        if liked {
            let position: i64 = self.conn.query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_tracks WHERE playlist_id = ?1",
                params![LIKED_PLAYLIST_ID],
                |row| row.get(0),
            )?;
            self.conn.execute(
                "INSERT OR IGNORE INTO playlist_tracks (playlist_id, song_key, position, added_at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    LIKED_PLAYLIST_ID,
                    identity.to_storage_string(),
                    position,
                    crate::now_unix()
                ],
            )?;
        } else {
            self.conn.execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND song_key = ?2",
                params![LIKED_PLAYLIST_ID, identity.to_storage_string()],
            )?;
        }
        Ok(())
    }

    pub fn is_liked(&self, identity: &TrackIdentity) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = ?1 AND song_key = ?2",
            params![LIKED_PLAYLIST_ID, identity.to_storage_string()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn auto_tracks(&self, playlist: AutoPlaylist) -> Result<Vec<Track>> {
        match playlist {
            AutoPlaylist::Liked => {
                self.ensure_auto(LIKED_PLAYLIST_ID, "Liked")?;
                self.playlist_tracks(
                    &PlaylistId::new(LIKED_PLAYLIST_ID),
                    SongSort::Custom,
                    SortDir::Asc,
                )
            }
            AutoPlaylist::Downloaded => {
                self.ensure_auto(DOWNLOADED_PLAYLIST_ID, "Downloaded")?;
                Ok(Vec::new())
            }
        }
    }

    pub fn top_playlists(&self, limit: usize) -> Result<Vec<(Playlist, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.kind, p.title, p.author, p.description, p.thumbnail, p.track_count, p.owned, p.privacy, p.author_id, (SELECT COUNT(*) FROM events e JOIN playlist_tracks pt ON pt.song_key = e.track_key WHERE pt.playlist_id = p.id) AS plays FROM playlists p WHERE p.kind = 'local' AND COALESCE(p.privacy, '') <> 'hidden' ORDER BY plays DESC, p.rowid ASC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], |row| {
                let plays: i64 = row.get("plays")?;
                Ok((read_playlist(row)?, plays.max(0) as u64))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
