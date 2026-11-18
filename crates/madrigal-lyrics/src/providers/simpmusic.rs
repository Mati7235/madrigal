use reqwest::Client;

use crate::providers::{Provider, ProviderResult, Query, get_json, parse_text};

pub struct SimpMusicProvider {
    base_url: String,
}

impl SimpMusicProvider {
    pub fn new() -> Self {
        Self {
            base_url: "https://api-lyrics.simpmusic.org/v1/".to_owned(),
        }
    }

    pub fn with_base_url(base: impl Into<String>) -> Self {
        Self {
            base_url: base.into(),
        }
    }

    pub fn closest(items: &[serde_json::Value], duration: Option<u32>) -> Option<&serde_json::Value> {
        let valid: Vec<&serde_json::Value> =
            items.iter().filter(|item| item.is_object()).collect();
        if valid.is_empty() {
            return None;
        }
        let Some(want) = duration else {
            return valid.into_iter().next();
        };
        valid.into_iter().min_by_key(|item| {
            item.get("duration")
                .and_then(|v| v.as_u64())
                .map(|d| d.abs_diff(want as u64))
                .unwrap_or(u64::MAX)
        })
    }

    pub fn to_lyrics(item: &serde_json::Value) -> Option<crate::Lyrics> {
        if let Some(synced) = item
            .get("syncedLyrics")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
        {
            if let Some(lyrics) = parse_text(synced) {
                return Some(lyrics);
            }
        }
        let plain = item
            .get("plainLyrics")
            .and_then(|v| v.as_str())
            .or_else(|| item.get("plainLyric").and_then(|v| v.as_str()))
            .filter(|s| !s.trim().is_empty())?;
        parse_text(plain)
    }
}

impl Default for SimpMusicProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for SimpMusicProvider {
    fn name(&self) -> &'static str {
        "simpmusic"
    }

    async fn fetch(&self, client: &Client, query: &Query) -> ProviderResult {
        let Some(video_id) = query.video_id.as_deref().filter(|id| !id.is_empty()) else {
            return Ok(None);
        };
        if video_id.starts_with("local:") || video_id.starts_with("tidal:") {
            return Ok(None);
        }
        let url = format!("{}{video_id}", self.base_url);
        let payload = get_json(client, &url, &[]).await?;
        let items = payload.get("data").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        match Self::closest(&items, query.duration_secs) {
            Some(best) => Ok(Self::to_lyrics(best)),
            None => Ok(None),
        }
    }
}
