use std::io;
use std::path::{Path, PathBuf};

use madrigal_core::{Event, Track, TrackIdentity, TrackSource};
use madrigal_settings::Integrations;
use serde::{Deserialize, Serialize};

use crate::eligibility::Eligibility;
use crate::error::Error;

pub const OUTBOX_FILE: &str = "scrobble-outbox.json";
pub const DEFAULT_BASE_RETRY_SECS: u64 = 60;
pub const DEFAULT_MAX_RETRY_SECS: u64 = 3600;
pub const SUBMITTED_RETENTION_SECS: i64 = 86400;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub source: String,
    pub track_id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub duration_secs: Option<u32>,
    pub started_at: i64,
    pub attempts: u32,
    pub next_retry_at: i64,
    pub submitted: bool,
    pub submitted_at: Option<i64>,
}

impl OutboxEntry {
    fn key(&self) -> (String, i64) {
        (
            format!("{}:{}", self.source, self.track_id),
            self.started_at,
        )
    }

    pub fn identity(&self) -> TrackIdentity {
        let source = match self.source.as_str() {
            "tidal" => TrackSource::Tidal,
            "local" => TrackSource::Local,
            _ => TrackSource::Youtube,
        };
        TrackIdentity::new(source, self.track_id.clone())
    }

    pub fn track(&self) -> Track {
        Track {
            identity: self.identity(),
            title: self.title.clone(),
            artists: self.artists.clone(),
            album: self.album.clone(),
            duration_secs: self.duration_secs,
            thumbnail: None,
            track_number: None,
            local_path: None,
            set_video_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitOutcome {
    Submitted,
    Retryable,
    Permanent,
}

pub trait ScrobbleSink {
    fn submit(&self, entry: &OutboxEntry) -> SubmitOutcome;
}

impl SubmitOutcome {
    pub fn from_result(result: &crate::Result<()>) -> Self {
        match result {
            Ok(()) => SubmitOutcome::Submitted,
            Err(Error::Transport(_)) => SubmitOutcome::Retryable,
            Err(Error::HttpStatus(code)) if *code == 429 || *code >= 500 => {
                SubmitOutcome::Retryable
            }
            Err(_) => SubmitOutcome::Permanent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlushStats {
    pub submitted: usize,
    pub retried: usize,
    pub dropped: usize,
}

#[derive(Debug)]
pub struct Outbox {
    path: PathBuf,
    entries: Vec<OutboxEntry>,
    base_retry_secs: u64,
    max_retry_secs: u64,
}

impl Outbox {
    pub fn default_path() -> PathBuf {
        madrigal_paths::data_dir().join(OUTBOX_FILE)
    }

    pub fn open(path: &Path) -> io::Result<Self> {
        let mut outbox = Self {
            path: path.to_path_buf(),
            entries: Vec::new(),
            base_retry_secs: DEFAULT_BASE_RETRY_SECS,
            max_retry_secs: DEFAULT_MAX_RETRY_SECS,
        };
        outbox.load()?;
        Ok(outbox)
    }

    pub fn open_default() -> io::Result<Self> {
        Self::open(&Self::default_path())
    }

    pub fn with_backoff(mut self, base_retry_secs: u64, max_retry_secs: u64) -> Self {
        self.base_retry_secs = base_retry_secs.max(1);
        self.max_retry_secs = max_retry_secs.max(self.base_retry_secs);
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len(&self) -> usize {
        self.entries.iter().filter(|entry| !entry.submitted).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn entries(&self) -> Vec<OutboxEntry> {
        self.entries.clone()
    }

    fn load(&mut self) -> io::Result<()> {
        let raw = match std::fs::read(&self.path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err),
        };
        if raw.is_empty() {
            return Ok(());
        }
        let parsed: Vec<OutboxEntry> = serde_json::from_slice(&raw)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        self.entries.clear();
        for entry in parsed {
            if !self.entries.iter().any(|known| known.key() == entry.key()) {
                self.entries.push(entry);
            }
        }
        Ok(())
    }

    fn save(&self) -> io::Result<()> {
        let text = serde_json::to_string(&self.entries)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        madrigal_files::atomic_write_text(&self.path, &text)?;
        Ok(())
    }

    fn backoff_secs(&self, attempts: u32) -> u64 {
        let shift = attempts.saturating_sub(1).min(20);
        self.base_retry_secs
            .saturating_mul(1u64 << shift)
            .min(self.max_retry_secs)
    }

    pub fn push(&mut self, track: &Track, started_at: i64) -> io::Result<bool> {
        let entry = OutboxEntry {
            source: track.identity.source.as_str().to_owned(),
            track_id: track.identity.id.clone(),
            title: track.title.clone(),
            artists: track.artists.clone(),
            album: track.album.clone(),
            duration_secs: track.duration_secs,
            started_at,
            attempts: 0,
            next_retry_at: started_at,
            submitted: false,
            submitted_at: None,
        };
        if self.entries.iter().any(|known| known.key() == entry.key()) {
            return Ok(false);
        }
        self.entries.push(entry);
        self.save()?;
        Ok(true)
    }

    pub fn due(&self, now: i64) -> Vec<OutboxEntry> {
        self.entries
            .iter()
            .filter(|entry| !entry.submitted && entry.next_retry_at <= now)
            .cloned()
            .collect()
    }

    pub fn flush(&mut self, sink: &dyn ScrobbleSink, now: i64) -> io::Result<FlushStats> {
        let mut stats = FlushStats::default();
        let due: Vec<(String, i64)> = self.due(now).iter().map(OutboxEntry::key).collect();
        for key in due {
            let position = self
                .entries
                .iter()
                .position(|entry| entry.key() == key && !entry.submitted);
            let Some(position) = position else {
                continue;
            };
            let entry = self.entries[position].clone();
            match sink.submit(&entry) {
                SubmitOutcome::Submitted => {
                    self.entries[position].submitted = true;
                    self.entries[position].submitted_at = Some(now);
                    stats.submitted += 1;
                }
                SubmitOutcome::Retryable => {
                    let attempts = self.entries[position].attempts.saturating_add(1);
                    self.entries[position].attempts = attempts;
                    self.entries[position].next_retry_at =
                        now.saturating_add(self.backoff_secs(attempts) as i64);
                    stats.retried += 1;
                }
                SubmitOutcome::Permanent => {
                    self.entries.remove(position);
                    stats.dropped += 1;
                }
            }
        }
        self.prune(now);
        self.save()?;
        Ok(stats)
    }

    fn prune(&mut self, now: i64) {
        self.entries.retain(|entry| {
            if !entry.submitted {
                return true;
            }
            match entry.submitted_at {
                Some(at) => now.saturating_sub(at) < SUBMITTED_RETENTION_SECS,
                None => false,
            }
        });
    }
}

pub fn due_events(
    conn: &rusqlite::Connection,
    eligibility: &Eligibility,
    limit: usize,
) -> Vec<Event> {
    let events = madrigal_db::recent_events(conn, limit).unwrap_or_default();
    events
        .into_iter()
        .filter(|event| {
            event.is_valid() && event.completed && eligibility.event_threshold(event).is_some()
        })
        .collect()
}

pub fn integrations_enabled(integrations: &Integrations) -> bool {
    integrations.maloja_enabled
        || integrations.listenbrainz_enabled
        || integrations.lastfm_scrobbling_enabled
}
