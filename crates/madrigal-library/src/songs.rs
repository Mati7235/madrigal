use madrigal_core::{Track, TrackIdentity};
use madrigal_settings::{SongFilter as PrefFilter, SongSortType};
use rusqlite::params;

use crate::{Error, Library, Result, LIKED_PLAYLIST_ID};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongSort {
    Name,
    Artist,
    DateAdded,
    PlayCount,
    PlayTime,
    Year,
    Duration,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongScope {
    Library,
    Liked,
    Downloaded,
}

pub(crate) fn read_track(row: &rusqlite::Row<'_>) -> rusqlite::Result<Track> {
    let source_raw: String = row.get("source")?;
    let track_id: String = row.get("track_id")?;
    let source = match source_raw.as_str() {
        "tidal" => madrigal_core::TrackSource::Tidal,
        "local" => madrigal_core::TrackSource::Local,
        _ => madrigal_core::TrackSource::Youtube,
    };
    let artists_raw: String = row.get("artists")?;
    Ok(Track {
        identity: TrackIdentity::new(source, track_id),
        title: row.get("title")?,
        artists: serde_json::from_str(&artists_raw).unwrap_or_default(),
        album: row.get("album")?,
        duration_secs: row.get("duration_secs")?,
        thumbnail: row.get("thumbnail")?,
        track_number: row.get("track_number")?,
        local_path: row.get("local_path")?,
        set_video_id: row.get("set_video_id")?,
    })
}

fn song_listing_sql(scope: SongScope, sort: SongSort) -> &'static str {
    match (scope, sort) {
        (SongScope::Liked, SongSort::Custom) => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM songs s WHERE (?1 = 'library' OR (?1 = 'liked' AND EXISTS (SELECT 1 FROM playlist_tracks pt WHERE pt.song_key = s.key AND pt.playlist_id = ?2)) OR (?1 = 'downloaded' AND EXISTS (SELECT 1 FROM downloads d WHERE d.song_key = s.key))) ORDER BY (SELECT pt.position FROM playlist_tracks pt WHERE pt.song_key = s.key AND pt.playlist_id = ?2) ASC, s.rowid ASC LIMIT ?3",
        (SongScope::Downloaded, SongSort::Custom) | (SongScope::Downloaded, SongSort::DateAdded) => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM songs s WHERE (?1 = 'library' OR (?1 = 'liked' AND EXISTS (SELECT 1 FROM playlist_tracks pt WHERE pt.song_key = s.key AND pt.playlist_id = ?2)) OR (?1 = 'downloaded' AND EXISTS (SELECT 1 FROM downloads d WHERE d.song_key = s.key))) ORDER BY (SELECT d.created_at FROM downloads d WHERE d.song_key = s.key) ASC, s.rowid ASC LIMIT ?3",
        (_, SongSort::Name) => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM songs s WHERE (?1 = 'library' OR (?1 = 'liked' AND EXISTS (SELECT 1 FROM playlist_tracks pt WHERE pt.song_key = s.key AND pt.playlist_id = ?2)) OR (?1 = 'downloaded' AND EXISTS (SELECT 1 FROM downloads d WHERE d.song_key = s.key))) ORDER BY s.title COLLATE NOCASE ASC, s.rowid ASC LIMIT ?3",
        (_, SongSort::Artist) => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM songs s WHERE (?1 = 'library' OR (?1 = 'liked' AND EXISTS (SELECT 1 FROM playlist_tracks pt WHERE pt.song_key = s.key AND pt.playlist_id = ?2)) OR (?1 = 'downloaded' AND EXISTS (SELECT 1 FROM downloads d WHERE d.song_key = s.key))) ORDER BY s.artists COLLATE NOCASE ASC, s.title COLLATE NOCASE ASC, s.rowid ASC LIMIT ?3",
        (_, SongSort::DateAdded) => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM songs s WHERE (?1 = 'library' OR (?1 = 'liked' AND EXISTS (SELECT 1 FROM playlist_tracks pt WHERE pt.song_key = s.key AND pt.playlist_id = ?2)) OR (?1 = 'downloaded' AND EXISTS (SELECT 1 FROM downloads d WHERE d.song_key = s.key))) ORDER BY s.rowid ASC LIMIT ?3",
        (_, SongSort::Custom) => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM songs s WHERE (?1 = 'library' OR (?1 = 'liked' AND EXISTS (SELECT 1 FROM playlist_tracks pt WHERE pt.song_key = s.key AND pt.playlist_id = ?2)) OR (?1 = 'downloaded' AND EXISTS (SELECT 1 FROM downloads d WHERE d.song_key = s.key))) ORDER BY s.rowid ASC LIMIT ?3",
        (_, SongSort::PlayCount) => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM songs s WHERE (?1 = 'library' OR (?1 = 'liked' AND EXISTS (SELECT 1 FROM playlist_tracks pt WHERE pt.song_key = s.key AND pt.playlist_id = ?2)) OR (?1 = 'downloaded' AND EXISTS (SELECT 1 FROM downloads d WHERE d.song_key = s.key))) ORDER BY (SELECT COUNT(*) FROM events e WHERE e.track_key = s.key) ASC, s.title COLLATE NOCASE ASC, s.rowid ASC LIMIT ?3",
        (_, SongSort::PlayTime) => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM songs s WHERE (?1 = 'library' OR (?1 = 'liked' AND EXISTS (SELECT 1 FROM playlist_tracks pt WHERE pt.song_key = s.key AND pt.playlist_id = ?2)) OR (?1 = 'downloaded' AND EXISTS (SELECT 1 FROM downloads d WHERE d.song_key = s.key))) ORDER BY (SELECT COALESCE(SUM(e.duration_secs), 0) FROM events e WHERE e.track_key = s.key) ASC, s.title COLLATE NOCASE ASC, s.rowid ASC LIMIT ?3",
        (_, SongSort::Year) => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM songs s LEFT JOIN albums a ON a.title = s.album WHERE (?1 = 'library' OR (?1 = 'liked' AND EXISTS (SELECT 1 FROM playlist_tracks pt WHERE pt.song_key = s.key AND pt.playlist_id = ?2)) OR (?1 = 'downloaded' AND EXISTS (SELECT 1 FROM downloads d WHERE d.song_key = s.key))) ORDER BY COALESCE(a.year, '') ASC, s.title COLLATE NOCASE ASC, s.rowid ASC LIMIT ?3",
        (_, SongSort::Duration) => "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM songs s WHERE (?1 = 'library' OR (?1 = 'liked' AND EXISTS (SELECT 1 FROM playlist_tracks pt WHERE pt.song_key = s.key AND pt.playlist_id = ?2)) OR (?1 = 'downloaded' AND EXISTS (SELECT 1 FROM downloads d WHERE d.song_key = s.key))) ORDER BY s.duration_secs IS NULL ASC, s.duration_secs ASC, s.rowid ASC LIMIT ?3",
    }
}

fn scope_param(scope: SongScope) -> &'static str {
    match scope {
        SongScope::Library => "library",
        SongScope::Liked => "liked",
        SongScope::Downloaded => "downloaded",
    }
}

fn sort_from_prefs(sort: SongSortType) -> SongSort {
    match sort {
        SongSortType::CreateDate => SongSort::DateAdded,
        SongSortType::Name => SongSort::Name,
        SongSortType::Artist => SongSort::Artist,
        SongSortType::PlayTime => SongSort::PlayTime,
    }
}

fn scope_from_prefs(filter: PrefFilter) -> SongScope {
    match filter {
        PrefFilter::Library => SongScope::Library,
        PrefFilter::Liked => SongScope::Liked,
        PrefFilter::Downloaded => SongScope::Downloaded,
    }
}

impl<'a> Library<'a> {
    pub fn song(&self, identity: &TrackIdentity) -> Result<Option<Track>> {
        self.conn
            .query_row(
                "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id FROM songs s WHERE s.key = ?1",
                params![identity.to_storage_string()],
                read_track,
            )
            .map(Some)
            .or_else(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(Error::from(other)),
            })
    }

    pub fn songs(&self, scope: SongScope, sort: SongSort, dir: SortDir) -> Result<Vec<Track>> {
        self.songs_limit(scope, sort, dir, -1)
    }

    pub(crate) fn songs_limit(
        &self,
        scope: SongScope,
        sort: SongSort,
        dir: SortDir,
        limit: i64,
    ) -> Result<Vec<Track>> {
        let mut stmt = self
            .conn
            .prepare(song_listing_sql(scope, sort))
            .map_err(Error::from)?;
        let mut tracks = stmt
            .query_map(
                params![scope_param(scope), LIKED_PLAYLIST_ID, limit],
                read_track,
            )
            .map_err(Error::from)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Error::from)?;
        if dir == SortDir::Desc {
            tracks.reverse();
        }
        Ok(tracks)
    }

    pub fn default_songs(&self) -> Result<Vec<Track>> {
        let content = self.settings().content;
        let dir = if content.song_sort_descending {
            SortDir::Desc
        } else {
            SortDir::Asc
        };
        self.songs(
            scope_from_prefs(content.song_filter),
            sort_from_prefs(content.song_sort_type),
            dir,
        )
    }

    pub fn song_play_count(&self, identity: &TrackIdentity) -> Result<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE track_key = ?1",
            params![identity.to_storage_string()],
            |row| row.get(0),
        )?;
        Ok(count.max(0) as u64)
    }

    pub fn mark_downloaded(
        &self,
        track: &Track,
        path: &std::path::Path,
        size_bytes: Option<u64>,
    ) -> Result<()> {
        madrigal_db::upsert_download(
            self.conn,
            &track.identity,
            path,
            track,
            size_bytes,
            crate::now_unix(),
        )?;
        Ok(())
    }

    pub fn remove_download(&self, identity: &TrackIdentity) -> Result<()> {
        self.conn.execute(
            "DELETE FROM downloads WHERE song_key = ?1",
            params![identity.to_storage_string()],
        )?;
        Ok(())
    }

    pub fn is_downloaded(&self, identity: &TrackIdentity) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM downloads WHERE song_key = ?1",
            params![identity.to_storage_string()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}
