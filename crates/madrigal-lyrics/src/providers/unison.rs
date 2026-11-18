use regex::Regex;
use reqwest::Client;
use std::sync::OnceLock;

use crate::providers::{Provider, ProviderResult, Query, get_json, get_text, parse_text};
use crate::{ttml, Lyrics};

pub const SEARCH_LIMIT: u32 = 5;

fn video_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"^[A-Za-z0-9_-]{6,64}$").expect("unison video"))
}

pub struct UnisonProvider {
    base_url: String,
}

impl UnisonProvider {
    pub fn new() -> Self {
        Self {
            base_url: "https://unison.boidu.dev/".to_owned(),
        }
    }

    pub fn with_base_url(base: impl Into<String>) -> Self {
        Self {
            base_url: base.into(),
        }
    }

    async fn search(
        &self,
        client: &Client,
        query: &Query,
    ) -> Result<Vec<serde_json::Value>, crate::providers::ProviderError> {
        let mut params = vec![
            ("song", query.title.trim().to_owned()),
            ("artist", query.artist().trim().to_owned()),
            ("limit", SEARCH_LIMIT.to_string()),
        ];
        if let Some(album) = query.album.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
            params.push(("album", album.to_owned()));
        }
        if let Some(duration) = query.duration_secs {
            params.push(("duration", duration.to_string()));
        }
        let url = format!("{}lyrics/search", self.base_url);
        let payload = get_json(client, &url, &params).await?;
        if payload.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
            Ok(payload.get("data").and_then(|v| v.as_array()).cloned().unwrap_or_default())
        } else {
            Ok(Vec::new())
        }
    }

    async fn by_video(&self, client: &Client, video_id: &str) -> Result<Option<serde_json::Value>, crate::providers::ProviderError> {
        if !video_re().is_match(video_id) {
            return Ok(None);
        }
        let url = format!("{}lyrics", self.base_url);
        let payload = get_json(client, &url, &[("v", video_id.to_owned())]).await?;
        Ok(entry(&payload))
    }

    async fn by_id(&self, client: &Client, entry_id: &serde_json::Value) -> Result<Option<serde_json::Value>, crate::providers::ProviderError> {
        let id = match entry_id {
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            _ => return Ok(None),
        };
        let url = format!("{}lyrics/{id}", self.base_url);
        let text = match get_text(client, &url, &[]).await? {
            Some(text) => text,
            None => return Ok(None),
        };
        let payload: serde_json::Value =
            serde_json::from_str(&text).map_err(|err| crate::providers::ProviderError::Json(err.to_string()))?;
        Ok(entry(&payload))
    }

    pub fn to_lyrics(text: &str) -> Option<Lyrics> {
        if text.trim().is_empty() {
            return None;
        }
        if ttml::looks_like_ttml(text) {
            if let Ok(parsed) = ttml::parse(text) {
                if !parsed.lines.is_empty() {
                    return Some(parsed);
                }
            }
            return None;
        }
        parse_text(text)
    }
}

fn entry(payload: &serde_json::Value) -> Option<serde_json::Value> {
    if !payload.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
        return None;
    }
    match payload.get("data") {
        Some(data) if data.get("lyrics").and_then(|v| v.as_str()).is_some() => Some(data.clone()),
        _ => None,
    }
}

impl Default for UnisonProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for UnisonProvider {
    fn name(&self) -> &'static str {
        "unison"
    }

    fn word_level(&self) -> bool {
        true
    }

    async fn fetch(&self, client: &Client, query: &Query) -> ProviderResult {
        let title = query.title.trim();
        let artist = query.artist().trim();
        if title.is_empty() || artist.is_empty() {
            return Ok(None);
        }
        let mut candidates: Vec<serde_json::Value> = Vec::new();
        for summary in self.search(client, query).await? {
            if summary.get("lyrics").and_then(|v| v.as_str()).is_some() {
                candidates.push(summary);
                continue;
            }
            let mut resolved = None;
            if let Some(video_id) = summary.get("videoId").and_then(|v| v.as_str()) {
                resolved = self.by_video(client, video_id).await?;
            }
            if resolved.is_none() {
                if let Some(id) = summary.get("id") {
                    resolved = self.by_id(client, id).await?;
                }
            }
            if let Some(found) = resolved {
                candidates.push(found);
            }
        }
        if candidates.is_empty() {
            if let Some(video_id) = query.video_id.as_deref().filter(|id| !id.is_empty()) {
                if let Some(found) = self.by_video(client, video_id).await? {
                    candidates.push(found);
                }
            }
        }
        for candidate in candidates {
            if let Some(text) = candidate.get("lyrics").and_then(|v| v.as_str()) {
                if let Some(lyrics) = Self::to_lyrics(text) {
                    return Ok(Some(lyrics));
                }
            }
        }
        Ok(None)
    }
}
