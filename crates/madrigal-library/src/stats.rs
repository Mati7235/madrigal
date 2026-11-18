use madrigal_core::{Track, TrackIdentity};
use madrigal_settings::TopRange;
use rusqlite::params;

use crate::songs::read_track;
use crate::{Error, Library, Result, now_unix};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub total_plays: u64,
    pub unique_tracks: u64,
    pub unique_artists: u64,
    pub total_listen_secs: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Streaks {
    pub current_days: u32,
    pub longest_days: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TopTrack {
    pub track: Track,
    pub plays: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopArtist {
    pub name: String,
    pub plays: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopAlbum {
    pub name: String,
    pub plays: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct YearInMusic {
    pub year: i32,
    pub top_tracks: Vec<TopTrack>,
    pub top_artists: Vec<TopArtist>,
    pub top_albums: Vec<TopAlbum>,
}

fn range_cutoff(range: TopRange) -> i64 {
    let span: i64 = match range {
        TopRange::AllTime => return 0,
        TopRange::Day => 86_400,
        TopRange::Week => 7 * 86_400,
        TopRange::Month => 30 * 86_400,
        TopRange::Year => 365 * 86_400,
    };
    now_unix().saturating_sub(span)
}

fn streaks_from_days(days: &[i64], today: i64) -> Streaks {
    let mut longest: u32 = 0;
    let mut run: u32 = 0;
    let mut previous: Option<i64> = None;
    for &day in days {
        match previous {
            Some(prev) if day == prev + 1 => run += 1,
            Some(prev) if day == prev => continue,
            _ => run = 1,
        }
        longest = longest.max(run);
        previous = Some(day);
    }
    let current = match previous {
        Some(last) if last == today || last == today - 1 => run,
        _ => 0,
    };
    Streaks {
        current_days: current,
        longest_days: longest,
    }
}

impl<'a> Library<'a> {
    pub fn play_count(&self, identity: &TrackIdentity) -> Result<u64> {
        self.song_play_count(identity)
    }

    pub fn stats(&self) -> Result<Stats> {
        let (total_plays, unique_tracks, total_listen_secs): (i64, i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COUNT(DISTINCT track_key), COALESCE(SUM(duration_secs), 0) FROM events",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let unique_artists: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT j.value) FROM events e JOIN songs s ON s.key = e.track_key, json_each(s.artists) j",
            [],
            |row| row.get(0),
        )?;
        Ok(Stats {
            total_plays: total_plays.max(0) as u64,
            unique_tracks: unique_tracks.max(0) as u64,
            unique_artists: unique_artists.max(0) as u64,
            total_listen_secs: total_listen_secs.max(0) as u64,
        })
    }

    pub fn streaks(&self) -> Result<Streaks> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT played_at / 86400 AS day FROM events ORDER BY day ASC",
        )?;
        let days = stmt
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(streaks_from_days(&days, now_unix() / 86_400))
    }

    pub fn top_tracks(&self, range: TopRange, limit: usize) -> Result<Vec<TopTrack>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id, COUNT(*) AS plays FROM events e JOIN songs s ON s.key = e.track_key WHERE e.played_at >= ?1 GROUP BY e.track_key ORDER BY plays DESC, MAX(e.played_at) DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![range_cutoff(range), limit as i64], |row| {
                let plays: i64 = row.get("plays")?;
                Ok(TopTrack {
                    track: read_track(row)?,
                    plays: plays.max(0) as u64,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn top_artists(&self, range: TopRange, limit: usize) -> Result<Vec<TopArtist>> {
        let mut stmt = self.conn.prepare(
            "SELECT j.value AS name, COUNT(*) AS plays FROM events e JOIN songs s ON s.key = e.track_key, json_each(s.artists) j WHERE e.played_at >= ?1 GROUP BY j.value ORDER BY plays DESC, name ASC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![range_cutoff(range), limit as i64], |row| {
                let plays: i64 = row.get("plays")?;
                Ok(TopArtist {
                    name: row.get("name")?,
                    plays: plays.max(0) as u64,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn top_albums(&self, range: TopRange, limit: usize) -> Result<Vec<TopAlbum>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.album AS name, COUNT(*) AS plays FROM events e JOIN songs s ON s.key = e.track_key WHERE e.played_at >= ?1 AND s.album IS NOT NULL AND s.album <> '' GROUP BY s.album ORDER BY plays DESC, name ASC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![range_cutoff(range), limit as i64], |row| {
                let plays: i64 = row.get("plays")?;
                Ok(TopAlbum {
                    name: row.get("name")?,
                    plays: plays.max(0) as u64,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn year_in_music(&self, year: i32, limit: usize) -> Result<YearInMusic> {
        if !(1970..=9999).contains(&year) {
            return Err(Error::Invalid("year out of range".to_owned()));
        }
        let year_param = year.to_string();
        let mut tracks_stmt = self.conn.prepare(
            "SELECT s.source, s.track_id, s.title, s.artists, s.album, s.duration_secs, s.thumbnail, s.track_number, s.local_path, s.set_video_id, COUNT(*) AS plays FROM events e JOIN songs s ON s.key = e.track_key WHERE e.played_at >= strftime('%s', ?1 || '-01-01') AND e.played_at < strftime('%s', (CAST(?1 AS INTEGER) + 1) || '-01-01') GROUP BY e.track_key ORDER BY plays DESC, MAX(e.played_at) DESC LIMIT ?2",
        )?;
        let top_tracks = tracks_stmt
            .query_map(params![year_param, limit as i64], |row| {
                let plays: i64 = row.get("plays")?;
                Ok(TopTrack {
                    track: read_track(row)?,
                    plays: plays.max(0) as u64,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut artists_stmt = self.conn.prepare(
            "SELECT j.value AS name, COUNT(*) AS plays FROM events e JOIN songs s ON s.key = e.track_key, json_each(s.artists) j WHERE e.played_at >= strftime('%s', ?1 || '-01-01') AND e.played_at < strftime('%s', (CAST(?1 AS INTEGER) + 1) || '-01-01') GROUP BY j.value ORDER BY plays DESC, name ASC LIMIT ?2",
        )?;
        let top_artists = artists_stmt
            .query_map(params![year_param, limit as i64], |row| {
                let plays: i64 = row.get("plays")?;
                Ok(TopArtist {
                    name: row.get("name")?,
                    plays: plays.max(0) as u64,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut albums_stmt = self.conn.prepare(
            "SELECT s.album AS name, COUNT(*) AS plays FROM events e JOIN songs s ON s.key = e.track_key WHERE e.played_at >= strftime('%s', ?1 || '-01-01') AND e.played_at < strftime('%s', (CAST(?1 AS INTEGER) + 1) || '-01-01') AND s.album IS NOT NULL AND s.album <> '' GROUP BY s.album ORDER BY plays DESC, name ASC LIMIT ?2",
        )?;
        let top_albums = albums_stmt
            .query_map(params![year_param, limit as i64], |row| {
                let plays: i64 = row.get("plays")?;
                Ok(TopAlbum {
                    name: row.get("name")?,
                    plays: plays.max(0) as u64,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(YearInMusic {
            year,
            top_tracks,
            top_artists,
            top_albums,
        })
    }
}
