use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

const MB: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum CacheKind {
    Image,
    Song,
    Canvas,
    Lyrics,
}

impl CacheKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "images",
            Self::Song => "songs",
            Self::Canvas => "canvas",
            Self::Lyrics => "lyrics",
        }
    }

    pub fn dir(self) -> PathBuf {
        madrigal_paths::cache_dir().join(self.as_str())
    }

    pub fn all() -> [Self; 4] {
        [Self::Image, Self::Song, Self::Canvas, Self::Lyrics]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheLimits {
    pub image_mb: i32,
    pub song_mb: i32,
    pub canvas_mb: i32,
    pub lyrics_mb: i32,
}

impl CacheLimits {
    pub fn from_settings(settings: &madrigal_settings::Settings) -> Self {
        Self {
            image_mb: settings.storage.max_image_cache_mb,
            song_mb: settings.storage.max_song_cache_mb,
            canvas_mb: settings.storage.max_canvas_cache_mb,
            lyrics_mb: settings.storage.max_lyrics_cache_mb,
        }
    }

    pub fn cap_for(self, kind: CacheKind) -> u64 {
        let mb = match kind {
            CacheKind::Image => self.image_mb,
            CacheKind::Song => self.song_mb,
            CacheKind::Canvas => self.canvas_mb,
            CacheKind::Lyrics => self.lyrics_mb,
        };
        (mb.max(0) as u64).saturating_mul(MB)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CacheReport {
    pub kind: CacheKind,
    pub cap_bytes: u64,
    pub removed_files: usize,
    pub freed_bytes: u64,
    pub remaining_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SweepReport {
    pub reports: Vec<CacheReport>,
}

impl SweepReport {
    pub fn total_freed(&self) -> u64 {
        self.reports.iter().map(|r| r.freed_bytes).sum()
    }

    pub fn total_removed(&self) -> usize {
        self.reports.iter().map(|r| r.removed_files).sum()
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::with_capacity(self.reports.len());
        for report in &self.reports {
            parts.push(format!(
                "{}: removed {} files, freed {} bytes, remaining {} of {} bytes",
                report.kind.as_str(),
                report.removed_files,
                report.freed_bytes,
                report.remaining_bytes,
                report.cap_bytes,
            ));
        }
        parts.join("; ")
    }
}

fn collect_files(dir: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let meta = match fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            if meta.is_dir() && !meta.file_type().is_symlink() {
                stack.push(path);
            } else if meta.is_file() {
                let mtime = meta.modified().unwrap_or(UNIX_EPOCH);
                files.push((path, meta.len(), mtime));
            }
        }
    }
    files.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));
    files
}

pub fn prune_dir_to_bytes(dir: &Path, cap_bytes: u64) -> CacheReportKindless {
    let files = collect_files(dir);
    let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
    let mut removed = 0usize;
    let mut freed = 0u64;
    let mut index = 0;
    while total > cap_bytes && index < files.len() {
        let (ref path, size, _) = files[index];
        index += 1;
        if fs::remove_file(path).is_ok() {
            removed += 1;
            freed += size;
            total -= size;
        }
    }
    CacheReportKindless {
        removed_files: removed,
        freed_bytes: freed,
        remaining_bytes: total,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheReportKindless {
    pub removed_files: usize,
    pub freed_bytes: u64,
    pub remaining_bytes: u64,
}

pub fn prune_cache(kind: CacheKind, cap_bytes: u64) -> CacheReport {
    let dir = kind.dir();
    let _ = fs::create_dir_all(&dir);
    let outcome = prune_dir_to_bytes(&dir, cap_bytes);
    CacheReport {
        kind,
        cap_bytes,
        removed_files: outcome.removed_files,
        freed_bytes: outcome.freed_bytes,
        remaining_bytes: outcome.remaining_bytes,
    }
}

pub fn enforce_limits(limits: &CacheLimits) -> SweepReport {
    let mut reports = Vec::with_capacity(4);
    for kind in CacheKind::all() {
        reports.push(prune_cache(kind, limits.cap_for(kind)));
    }
    SweepReport { reports }
}
