use reqwest::Client;

use crate::matching::{clean_title, similarity};
use crate::providers::{Provider, ProviderError, ProviderResult, Query, get_json, parse_text};
use crate::Lyrics;

pub const SEARCH_PATH: &str = "/api/search";
pub const MAX_DURATION_DELTA: u32 = 2;
pub const MIN_SIMILARITY: f64 = 0.6;
pub const MIN_MATCH: f64 = 0.35;

pub struct LrclibProvider {
    base_url: String,
}

impl LrclibProvider {
    pub fn new() -> Self {
        Self {
            base_url: "https://lrclib.net".to_owned(),
        }
    }

    pub fn with_base_url(base: impl Into<String>) -> Self {
        Self {
            base_url: base.into(),
        }
    }

    fn usable(item: &serde_json::Value) -> bool {
        item.get("syncedLyrics").and_then(|v| v.as_str()).is_some_and(|s| !s.trim().is_empty())
            || item.get("plainLyrics").and_then(|v| v.as_str()).is_some_and(|s| !s.trim().is_empty())
            || item.get("instrumental").and_then(|v| v.as_bool()).unwrap_or(false)
    }

    fn score(item: &serde_json::Value, query: &Query) -> f64 {
        let title = similarity(
            &clean_title(&query.title),
            item.get("trackName").and_then(|v| v.as_str()).unwrap_or_default(),
        );
        let artist = similarity(
            query.artist(),
            item.get("artistName").and_then(|v| v.as_str()).unwrap_or_default(),
        );
        let mut score = (title + artist) / 2.0;
        if item.get("syncedLyrics").and_then(|v| v.as_str()).is_some() {
            score += 0.1;
        }
        score
    }

    fn select<'a>(items: &'a [serde_json::Value], query: &Query) -> Option<&'a serde_json::Value> {
        let usable: Vec<&serde_json::Value> =
            items.iter().filter(|item| Self::usable(item)).collect();
        if usable.is_empty() {
            return None;
        }
        if let Some(want) = query.duration_secs {
            let mut within: Vec<(&serde_json::Value, f64)> = usable
                .iter()
                .filter(|item| {
                    item.get("duration")
                        .and_then(|v| v.as_f64())
                        .is_some_and(|d| (d - want as f64).abs() <= MAX_DURATION_DELTA as f64)
                })
                .map(|item| (*item, Self::score(item, query)))
                .collect();
            if within.is_empty() {
                return None;
            }
            within.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            if within[0].1 < MIN_MATCH {
                return None;
            }
            let top = within[0].1;
            let near: Vec<&serde_json::Value> = within
                .iter()
                .filter(|(_, s)| *s >= top - 0.2)
                .map(|(item, _)| *item)
                .collect();
            return Self::prefer_synced(&near).or_else(|| near.first().copied());
        }
        let mut scored: Vec<(&serde_json::Value, f64)> =
            usable.iter().map(|item| (*item, Self::score(item, query))).collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        if scored[0].1 < MIN_SIMILARITY {
            return None;
        }
        let near: Vec<&serde_json::Value> = scored.iter().take(5).map(|(item, _)| *item).collect();
        Self::prefer_synced(&near).or_else(|| near.first().copied())
    }

    fn prefer_synced<'a>(items: &[&'a serde_json::Value]) -> Option<&'a serde_json::Value> {
        items
            .iter()
            .find(|item| {
                item.get("syncedLyrics").and_then(|v| v.as_str()).is_some_and(|s| !s.trim().is_empty())
            })
            .copied()
            .or_else(|| {
                items
                    .iter()
                    .find(|item| {
                        item.get("plainLyrics").and_then(|v| v.as_str()).is_some_and(|s| !s.trim().is_empty())
                            || item.get("instrumental").and_then(|v| v.as_bool()).unwrap_or(false)
                    })
                    .copied()
            })
    }

    pub fn to_lyrics(item: &serde_json::Value) -> Option<Lyrics> {
        if item.get("instrumental").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Some(Lyrics::default());
        }
        if let Some(synced) = item.get("syncedLyrics").and_then(|v| v.as_str()) {
            if let Some(lyrics) = parse_text(synced) {
                return Some(lyrics);
            }
        }
        if let Some(plain) = item.get("plainLyrics").and_then(|v| v.as_str()) {
            return parse_text(plain);
        }
        None
    }
}

impl Default for LrclibProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for LrclibProvider {
    fn name(&self) -> &'static str {
        "lrclib"
    }

    async fn fetch(&self, client: &Client, query: &Query) -> ProviderResult {
        let mut params: Vec<(&str, String)> =
            vec![("track_name", crate::matching::clean_title(&query.title))];
        if !query.artist().is_empty() {
            params.push(("artist_name", query.artist().to_owned()));
        }
        if let Some(album) = query.album.as_deref().filter(|a| !a.trim().is_empty()) {
            params.push(("album_name", album.to_owned()));
        }
        let url = format!("{}{SEARCH_PATH}", self.base_url);
        let payload = get_json(client, &url, &params).await?;
        let items = payload.as_array().ok_or_else(|| ProviderError::Json("expected array".to_owned()))?;
        match Self::select(items, query) {
            Some(item) => Ok(Self::to_lyrics(item)),
            None => Ok(None),
        }
    }
}
