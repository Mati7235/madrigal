use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use madrigal_core::Track;
use serde::{Deserialize, Serialize};

use crate::{CanvasEntry, CanvasSource};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stored {
    entry: CanvasEntry,
    expires_at: u64,
}

pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|span| span.as_secs())
        .unwrap_or(0)
}

pub fn key_for(track: &Track, source: CanvasSource) -> String {
    let mut key = format!(
        "{}__{}__{}",
        source.name().to_lowercase(),
        track.identity.to_storage_string(),
        track.title.trim().to_lowercase(),
    );
    key.push_str(&track.artist_text().trim().to_lowercase());
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn max_bytes(storage: &madrigal_settings::Storage) -> u64 {
    let mb = storage.max_canvas_cache_mb.clamp(0, 1024 * 1024) as u64;
    mb.saturating_mul(1024 * 1024)
}

pub struct CanvasCache {
    dir: PathBuf,
}

impl CanvasCache {
    pub fn new() -> io::Result<CanvasCache> {
        let dir = madrigal_paths::cache_dir().join("canvas");
        CanvasCache::with_dir(dir)
    }

    pub fn with_dir(dir: impl Into<PathBuf>) -> io::Result<CanvasCache> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(CanvasCache { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn entry_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }

    pub fn load(&self, key: &str) -> Option<CanvasEntry> {
        let raw = fs::read(self.entry_path(key)).ok()?;
        let stored: Stored = serde_json::from_slice(&raw).ok()?;
        if stored.expires_at <= now_epoch() {
            return None;
        }
        if !stored.entry.has_animation() {
            return None;
        }
        Some(stored.entry)
    }

    pub fn store(&self, key: &str, entry: &CanvasEntry, ttl_secs: u64) -> io::Result<bool> {
        if ttl_secs == 0 || !entry.has_animation() {
            return Ok(false);
        }
        fs::create_dir_all(&self.dir)?;
        let stored = Stored {
            entry: entry.clone(),
            expires_at: now_epoch().saturating_add(ttl_secs),
        };
        let raw = serde_json::to_vec(&stored)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        madrigal_files::atomic_write_bytes(&self.entry_path(key), &raw)?;
        Ok(true)
    }

    pub fn invalidate(&self, key: &str) {
        let _ = fs::remove_file(self.entry_path(key));
    }

    pub fn prune(&self, storage: &madrigal_settings::Storage) -> usize {
        let budget = max_bytes(storage);
        let mut files: Vec<(PathBuf, u64, u64)> = Vec::new();
        let mut total = 0u64;
        let entries = fs::read_dir(&self.dir).map(|it| it.collect::<Vec<_>>());
        for entry in entries.unwrap_or_default().into_iter().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            let expired = fs::read(&path)
                .ok()
                .and_then(|raw| serde_json::from_slice::<Stored>(&raw).ok())
                .is_none_or(|stored| stored.expires_at <= now_epoch());
            if expired && fs::remove_file(&path).is_ok() {
                continue;
            }
            let mtime = entry
                .metadata()
                .ok()
                .and_then(|meta| meta.modified().ok())
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|span| span.as_secs())
                .unwrap_or(0);
            total = total.saturating_add(size);
            files.push((path, size, mtime));
        }
        files.sort_by_key(|(_, _, mtime)| *mtime);
        let mut removed = 0;
        let mut usage = total;
        for (path, size, _) in files {
            if usage <= budget {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                usage = usage.saturating_sub(size);
                removed += 1;
            }
        }
        removed
    }
}
