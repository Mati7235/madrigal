use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::resolver::AudioStream;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    stream: AudioStream,
    expires_at: f64,
}

pub fn now_epoch() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn sanitize_quality(quality: Option<&str>) -> String {
    let raw = quality.unwrap_or("default");
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "default".to_string()
    } else {
        cleaned
    }
}

pub struct StreamCache {
    dir: PathBuf,
    safety_margin_secs: f64,
    fallback_ttl_secs: f64,
}

impl StreamCache {
    pub fn new(safety_margin_secs: f64, fallback_ttl_secs: f64) -> io::Result<Self> {
        let dir = madrigal_paths::cache_dir().join("streams");
        fs::create_dir_all(&dir)?;
        Ok(Self::with_dir(dir, safety_margin_secs, fallback_ttl_secs))
    }

    pub fn with_dir(
        dir: impl Into<PathBuf>,
        safety_margin_secs: f64,
        fallback_ttl_secs: f64,
    ) -> Self {
        Self {
            dir: dir.into(),
            safety_margin_secs: safety_margin_secs.max(0.0),
            fallback_ttl_secs: fallback_ttl_secs.max(1.0),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn entry_path(&self, video_id: &str, quality: Option<&str>) -> PathBuf {
        self.dir
            .join(format!("{}__{}.json", video_id, sanitize_quality(quality)))
    }

    pub fn ttl_for(&self, stream: &AudioStream, now: f64) -> f64 {
        let ttl = match stream.expires_at {
            Some(expiry) => expiry - now - self.safety_margin_secs,
            None => self.fallback_ttl_secs,
        };
        ttl.clamp(0.0, 6.0 * 3600.0)
    }

    pub fn load(&self, video_id: &str, quality: Option<&str>, now: f64) -> Option<AudioStream> {
        let raw = fs::read(self.entry_path(video_id, quality)).ok()?;
        let entry: Entry = serde_json::from_slice(&raw).ok()?;
        if entry.expires_at <= now {
            return None;
        }
        if entry.stream.video_id != video_id {
            return None;
        }
        Some(entry.stream)
    }

    pub fn store(
        &self,
        video_id: &str,
        quality: Option<&str>,
        stream: &AudioStream,
        now: f64,
    ) -> io::Result<bool> {
        let ttl = self.ttl_for(stream, now);
        if ttl <= 0.0 {
            return Ok(false);
        }
        fs::create_dir_all(&self.dir)?;
        let entry = Entry {
            stream: stream.clone(),
            expires_at: now + ttl,
        };
        let raw = serde_json::to_vec(&entry)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        madrigal_files::atomic_write_bytes(&self.entry_path(video_id, quality), &raw)?;
        Ok(true)
    }

    pub fn prune(&self, now: f64) -> usize {
        let mut removed = 0;
        let entries = fs::read_dir(&self.dir).map(|it| it.collect::<Vec<_>>());
        for entry in entries.unwrap_or_default().into_iter().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let expired = fs::read(&path)
                .ok()
                .and_then(|raw| serde_json::from_slice::<Entry>(&raw).ok())
                .is_none_or(|entry| entry.expires_at <= now);
            if expired && fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
        removed
    }
}
