use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use madrigal_core::Track;
use serde::Serialize;

pub const MODERATE_MARKER: &str = "moderate:";
pub const REFRESH_INTERVAL_SECS: i64 = 4 * 3600;
pub const MAX_RESPONSE_BYTES: usize = 5 * 1024 * 1024;
pub const FETCH_TIMEOUT_SECS: u64 = 30;
pub const DEFAULT_SOURCE_URL: &str = "";

const FALLBACK_LIST: &str = include_str!("../fixtures/ai_filter_sample.txt");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    Off,
    HighConfidence,
    IncludeModerate,
}

impl FilterMode {
    pub fn from_settings(settings: &madrigal_settings::Settings) -> Self {
        if !settings.privacy.ai_content_filter_enabled {
            return Self::Off;
        }
        if settings.privacy.ai_content_filter_include_moderate {
            Self::IncludeModerate
        } else {
            Self::HighConfidence
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Blocklists {
    pub high: HashSet<String>,
    pub moderate: HashSet<String>,
}

impl Blocklists {
    pub fn is_empty(&self) -> bool {
        self.high.is_empty() && self.moderate.is_empty()
    }

    pub fn len(&self) -> (usize, usize) {
        (self.high.len(), self.moderate.len())
    }
}

pub fn normalize_key(value: &str) -> Option<String> {
    let cleaned: String = value
        .trim()
        .strip_prefix('@')
        .unwrap_or(value.trim())
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    let lowered = cleaned.to_lowercase();
    if lowered.is_empty() {
        None
    } else {
        Some(lowered)
    }
}

fn valid_id(raw: &str) -> bool {
    let id = raw.trim();
    !id.is_empty() && id.len() <= 256 && normalize_key(id).is_some()
}

pub fn parse_list(text: &str) -> Blocklists {
    let mut lists = Blocklists::default();
    let mut moderate_section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        if line.eq_ignore_ascii_case(MODERATE_MARKER) {
            moderate_section = true;
            continue;
        }
        let (moderate, id) = match line.split_once(':') {
            Some((prefix, rest))
                if prefix.trim().eq_ignore_ascii_case("moderate") && !rest.contains(':') =>
            {
                (true, rest.trim())
            }
            _ => (moderate_section, line),
        };
        if !valid_id(id) {
            continue;
        }
        if moderate {
            lists.moderate.insert(id.to_owned());
        } else {
            lists.high.insert(id.to_owned());
        }
    }
    lists
}

fn normalized_set(ids: &HashSet<String>) -> HashSet<String> {
    ids.iter().filter_map(|id| normalize_key(id)).collect()
}

pub struct SlopFilter {
    mode: FilterMode,
    high: HashSet<String>,
    high_norm: HashSet<String>,
    moderate: HashSet<String>,
    moderate_norm: HashSet<String>,
}

impl SlopFilter {
    pub fn new(lists: Blocklists, mode: FilterMode) -> Self {
        Self {
            mode,
            high_norm: normalized_set(&lists.high),
            moderate_norm: normalized_set(&lists.moderate),
            high: lists.high,
            moderate: lists.moderate,
        }
    }

    pub fn from_settings(
        settings: &madrigal_settings::Settings,
        lists: Blocklists,
    ) -> Self {
        Self::new(lists, FilterMode::from_settings(settings))
    }

    pub fn mode(&self) -> FilterMode {
        self.mode
    }

    fn matches(set: &HashSet<String>, norm: &HashSet<String>, value: &str) -> bool {
        if set.contains(value) {
            return true;
        }
        match normalize_key(value) {
            Some(key) => norm.contains(&key),
            None => false,
        }
    }

    pub fn is_blocked(&self, track: &Track) -> bool {
        if self.mode == FilterMode::Off {
            return false;
        }
        let mut blocked =
            Self::matches(&self.high, &self.high_norm, &track.identity.id);
        if !blocked {
            blocked = track
                .artists
                .iter()
                .any(|artist| Self::matches(&self.high, &self.high_norm, artist));
        }
        if !blocked && self.mode == FilterMode::IncludeModerate {
            blocked = Self::matches(&self.moderate, &self.moderate_norm, &track.identity.id)
                || track
                    .artists
                    .iter()
                    .any(|artist| {
                        Self::matches(&self.moderate, &self.moderate_norm, artist)
                    });
        }
        blocked
    }

    pub fn filter_tracks(&self, tracks: Vec<Track>) -> (Vec<Track>, usize) {
        if self.mode == FilterMode::Off {
            return (tracks, 0);
        }
        let mut kept = Vec::with_capacity(tracks.len());
        let mut hidden = 0;
        for track in tracks {
            if self.is_blocked(&track) {
                hidden += 1;
            } else {
                kept.push(track);
            }
        }
        (kept, hidden)
    }

    pub fn status(&self, last_updated: i64) -> FilterStatus {
        FilterStatus {
            blocklist_count: self.high.len(),
            warnlist_count: self.moderate.len(),
            last_updated,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FilterStatus {
    pub blocklist_count: usize,
    pub warnlist_count: usize,
    pub last_updated: i64,
}

pub fn cache_path() -> PathBuf {
    madrigal_paths::cache_dir().join("curation").join("ai_filter.txt")
}

pub fn load_cached() -> Option<String> {
    std::fs::read_to_string(cache_path()).ok()
}

pub fn store_cached(text: &str) -> io::Result<()> {
    madrigal_files::atomic_write_text(&cache_path(), text)?;
    Ok(())
}

pub fn fallback_lists() -> Blocklists {
    parse_list(FALLBACK_LIST)
}

#[derive(Debug)]
pub enum FetchError {
    Transport(String),
    Status(u16),
    TooLarge,
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(detail) => write!(f, "ai list fetch failed: {detail}"),
            Self::Status(code) => write!(f, "ai list fetch failed: http {code}"),
            Self::TooLarge => write!(f, "ai list fetch failed: response too large"),
        }
    }
}

impl std::error::Error for FetchError {}

pub fn refresh_from_url(url: &str) -> Result<String, FetchError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
        .build()
        .map_err(|err| FetchError::Transport(err.to_string()))?;
    let mut response = client
        .get(url)
        .header("User-Agent", "madrigal-curation")
        .send()
        .map_err(|err| FetchError::Transport(err.to_string()))?;
    if !response.status().is_success() {
        return Err(FetchError::Status(response.status().as_u16()));
    }
    let mut total = 0usize;
    let mut body = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        use std::io::Read;
        match response.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                if total > MAX_RESPONSE_BYTES {
                    return Err(FetchError::TooLarge);
                }
                body.extend_from_slice(&buf[..n]);
            }
            Err(err) => return Err(FetchError::Transport(err.to_string())),
        }
    }
    String::from_utf8(body).map_err(|err| FetchError::Transport(err.to_string()))
}

pub fn should_refresh(last_updated: i64, now_secs: i64, force: bool) -> bool {
    if force {
        return true;
    }
    now_secs.saturating_sub(last_updated) >= REFRESH_INTERVAL_SECS
}

pub fn now_epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
