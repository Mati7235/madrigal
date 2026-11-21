use madrigal_core::{Event, SearchHistoryItem};
use rusqlite::params;

use crate::{Error, Library, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub event: Event,
    pub title: Option<String>,
    pub artist: Option<String>,
}

impl<'a> Library<'a> {
    pub fn record_play(&self, event: &Event) -> Result<bool> {
        self.record_play_meta(event, None, None)
    }

    pub fn record_play_meta(
        &self,
        event: &Event,
        title: Option<&str>,
        artist: Option<&str>,
    ) -> Result<bool> {
        if !event.is_valid() {
            return Err(Error::Invalid("event is not valid".to_owned()));
        }
        if self.settings().privacy.pause_listen_history {
            return Ok(false);
        }
        madrigal_db::record_event_meta(self.conn, event, title, artist)?;
        Ok(true)
    }

    pub fn listen_history(&self, limit: usize) -> Result<Vec<Event>> {
        Ok(self
            .listen_history_entries(limit)?
            .into_iter()
            .map(|entry| entry.event)
            .collect())
    }

    pub fn listen_history_entries(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT track_key, played_at, duration_secs, completed, title, artist FROM events WHERE played_at >= ?1 ORDER BY played_at DESC, rowid DESC LIMIT ?2",
        )?;
        let events = stmt
            .query_map(params![self.history_cutoff(), limit as i64], |row| {
                let key: String = row.get("track_key")?;
                let completed: i64 = row.get("completed")?;
                Ok(HistoryEntry {
                    event: Event {
                        track: madrigal_core::TrackIdentity::parse_prefixed(&key),
                        played_at: row.get("played_at")?,
                        duration_secs: row.get("duration_secs")?,
                        completed: completed != 0,
                    },
                    title: row.get("title")?,
                    artist: row.get("artist")?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(events)
    }

    pub fn clear_listen_history(&self) -> Result<()> {
        self.conn.execute("DELETE FROM events", [])?;
        Ok(())
    }

    pub fn record_search(&self, query: &str) -> Result<bool> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(false);
        }
        if self.settings().privacy.pause_search_history {
            return Ok(false);
        }
        madrigal_db::record_search(
            self.conn,
            &SearchHistoryItem {
                query: query.to_owned(),
                searched_at: crate::now_unix(),
            },
        )?;
        Ok(true)
    }

    pub fn search_history(&self, limit: usize) -> Result<Vec<SearchHistoryItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT query, searched_at FROM search_history WHERE searched_at >= ?1 ORDER BY searched_at DESC, rowid DESC LIMIT ?2",
        )?;
        let items = stmt
            .query_map(params![self.history_cutoff(), limit as i64], |row| {
                Ok(SearchHistoryItem {
                    query: row.get("query")?,
                    searched_at: row.get("searched_at")?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(items)
    }

    pub fn remove_search(&self, query: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM search_history WHERE query = ?1",
            params![query],
        )?;
        Ok(())
    }

    pub fn clear_search_history(&self) -> Result<()> {
        self.conn.execute("DELETE FROM search_history", [])?;
        Ok(())
    }
}
