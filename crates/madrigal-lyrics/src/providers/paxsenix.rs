use reqwest::Client;
use serde_json::Value;

use crate::providers::{Provider, ProviderResult, Query, parse_text};
use crate::Lyrics;

pub const BASE: &str = "https://api.paxsenix.org/";
const MINIMUM_MATCH_SCORE: i64 = 10;
const CONTENT_KEYS: &[&str] = &[
    "lyrics",
    "lrc",
    "content",
    "lines",
    "text",
    "plainLyrics",
    "syncedLyrics",
    "line",
    "lyric",
];

#[derive(Debug, Clone)]
struct Candidate {
    id: String,
    title: String,
    artist: String,
    duration: u32,
}

pub struct PaxsenixProvider {
    base_url: String,
    api_key: String,
    flavor: Flavor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    AppleMusic,
    Spotify,
    Musixmatch,
}

impl Flavor {
    fn name(&self) -> &'static str {
        match self {
            Flavor::AppleMusic => "paxsenix_apple_music",
            Flavor::Spotify => "paxsenix_spotify",
            Flavor::Musixmatch => "paxsenix_musixmatch",
        }
    }
}

impl PaxsenixProvider {
    pub fn apple_music(api_key: impl Into<String>) -> Self {
        Self::styled(Flavor::AppleMusic, api_key)
    }

    pub fn spotify(api_key: impl Into<String>) -> Self {
        Self::styled(Flavor::Spotify, api_key)
    }

    pub fn musixmatch(api_key: impl Into<String>) -> Self {
        Self::styled(Flavor::Musixmatch, api_key)
    }

    fn styled(flavor: Flavor, api_key: impl Into<String>) -> Self {
        Self {
            base_url: BASE.to_owned(),
            api_key: api_key.into(),
            flavor,
        }
    }

    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base_url = base.into();
        self
    }

    pub fn has_key(&self) -> bool {
        !self.api_key.trim().is_empty()
    }

    async fn get(&self, client: &Client, path: &str, params: &[(&str, String)]) -> Result<Option<String>, crate::providers::ProviderError> {
        use crate::providers::ProviderError;
        let params: Vec<(String, String)> =
            params.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect();
        let response = client
            .get(format!("{}{path}", self.base_url))
            .query(&params)
            .header("Authorization", format!("Bearer {}", self.api_key.trim()))
            .header("Accept", "application/json, text/plain, */*")
            .send()
            .await
            .map_err(|err| ProviderError::Http(err.to_string()))?;
        if !response.status().is_success() {
            return Err(ProviderError::Status(response.status().as_u16()));
        }
        response.text().await.map(Some).map_err(|err| ProviderError::Http(err.to_string()))
    }

    async fn search_id(&self, client: &Client, path: &str, query: &Query) -> Result<Option<String>, crate::providers::ProviderError> {
        let text = match self
            .get(client, path, &[("q", format!("{} {}", query.title.trim(), query.artist().trim()).trim().to_owned())])
            .await?
        {
            Some(text) => text,
            None => return Ok(None),
        };
        let payload: Value = match serde_json::from_str(&text) {
            Ok(payload) => payload,
            Err(_) => return Ok(None),
        };
        let mut candidates = Vec::new();
        collect(&payload, &mut candidates);
        let mut best: Option<(i64, String)> = None;
        for candidate in candidates {
            let score = score_candidate(&candidate, query);
            if score >= MINIMUM_MATCH_SCORE && best.as_ref().is_none_or(|(top, _)| score > *top) {
                best = Some((score, candidate.id));
            }
        }
        Ok(best.map(|(_, id)| id))
    }

    async fn lyrics_for_id(&self, client: &Client, path: &str, id: &str) -> Result<Option<Lyrics>, crate::providers::ProviderError> {
        let Some(body) = self.get(client, path, &[("id", id.to_owned())]).await? else {
            return Ok(None);
        };
        if self.flavor == Flavor::AppleMusic {
            if let Some(lrc) = apple_to_lrc(&body) {
                if let Some(lyrics) = parse_text(&lrc) {
                    return Ok(Some(lyrics));
                }
            }
        }
        Ok(extract(&body).and_then(|text| parse_text(&text)))
    }
}

fn collect(element: &Value, out: &mut Vec<Candidate>) {
    match element {
        Value::Array(items) => {
            for item in items {
                collect(item, out);
            }
        }
        Value::Object(map) => {
            if let Some(candidate) = candidate_of(map) {
                out.push(candidate);
            }
            for value in map.values() {
                collect(value, out);
            }
        }
        _ => {}
    }
}

fn candidate_of(map: &serde_json::Map<String, Value>) -> Option<Candidate> {
    let details = map.get("attributes").filter(|v| v.is_object()).unwrap_or(&Value::Null);
    let scope = if details.is_object() { details } else { &Value::Object(map.clone()) };
    let id = first_string(scope, &["id", "trackId", "track_id", "realId"])
        .or_else(|| first_string(&Value::Object(map.clone()), &["id", "trackId", "track_id", "realId"]))?;
    let title = first_string(scope, &["name", "title", "trackName", "track_name"])?;
    Some(Candidate {
        id,
        artist: artist_name(scope).unwrap_or_default(),
        duration: duration_seconds(scope),
        title,
    })
}

fn artist_name(element: &Value) -> Option<String> {
    if let Some(name) = element.get("artistName").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
        return Some(name.trim().to_owned());
    }
    if let Some(name) = element.get("artist_name").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
        return Some(name.trim().to_owned());
    }
    match element.get("artists").or_else(|| element.get("artist")) {
        Some(Value::String(name)) => Some(name.clone()),
        Some(Value::Object(map)) => first_string(&Value::Object(map.clone()), &["name", "artistName", "title"]),
        Some(Value::Array(items)) => {
            let names: Vec<String> = items
                .iter()
                .filter_map(|item| match item {
                    Value::String(name) => Some(name.clone()),
                    Value::Object(map) => {
                        first_string(&Value::Object(map.clone()), &["name", "artistName", "title"])
                    }
                    _ => None,
                })
                .collect();
            if names.is_empty() { None } else { Some(names.join(", ")) }
        }
        _ => None,
    }
}

fn duration_seconds(element: &Value) -> u32 {
    for key in ["durationInMillis", "durationMs", "duration_ms", "duration"] {
        if let Some(value) = element.get(key).and_then(|v| v.as_f64()).filter(|v| *v > 0.0) {
            return if value >= 10_000.0 { (value / 1000.0) as u32 } else { value as u32 };
        }
    }
    0
}

fn first_string(element: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = element.get(*key).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) {
            return Some(text.to_owned());
        }
    }
    None
}

fn score_candidate(candidate: &Candidate, query: &Query) -> i64 {
    let mut score = 0;
    score += text_score(&candidate.title, &query.title, 20, 10);
    score += text_score(&candidate.artist, query.artist(), 15, 5);
    if let Some(want) = query.duration_secs {
        if candidate.duration > 0 {
            let diff = want.abs_diff(candidate.duration);
            if diff < 3 {
                score += 10;
            } else if diff < 10 {
                score += 5;
            }
        }
    }
    score
}

fn text_score(candidate: &str, wanted: &str, exact: i64, partial: i64) -> i64 {
    let left = candidate.trim().to_lowercase();
    let right = wanted.trim().to_lowercase();
    if left.is_empty() || right.is_empty() {
        return 0;
    }
    if left == right {
        return exact;
    }
    if left.contains(&right) || right.contains(&left) {
        return partial;
    }
    0
}

pub fn apple_to_lrc(body: &str) -> Option<String> {
    let payload: Value = serde_json::from_str(body).ok()?;
    let content = payload.get("content")?.as_array()?;
    let mut lines = Vec::new();
    for line in content {
        let timestamp = line.get("timestamp")?.as_f64()?;
        let parts = line.get("text")?.as_array()?;
        let text = parts
            .iter()
            .filter_map(|part| part.get("text").and_then(|v| v.as_str()))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if text.is_empty() {
            continue;
        }
        let total = timestamp.max(0.0) / 1000.0;
        let minutes = (total / 60.0).floor() as i64;
        let seconds = total % 60.0;
        lines.push(format!("[{minutes:02}:{seconds:05.2}]{text}"));
    }
    if lines.is_empty() { None } else { Some(lines.join("\n")) }
}

pub fn extract(body: &str) -> Option<String> {
    let stripped = body.trim();
    if stripped.is_empty() {
        return None;
    }
    match serde_json::from_str::<Value>(stripped) {
        Ok(payload) => extract_element(&payload),
        Err(_) => Some(stripped.to_owned()),
    }
}

fn extract_element(element: &Value) -> Option<String> {
    match element {
        Value::Null => None,
        Value::Bool(_) => None,
        Value::Number(_) => None,
        Value::String(text) => {
            let value = text.trim();
            if value.is_empty() {
                return None;
            }
            match serde_json::from_str::<Value>(value) {
                Ok(Value::String(inner)) => Some(inner),
                Ok(nested @ (Value::Object(_) | Value::Array(_))) => extract_element(&nested),
                _ => Some(value.to_owned()),
            }
        }
        Value::Array(items) => {
            let parts: Vec<String> =
                items.iter().filter_map(extract_element).collect();
            let joined = parts.join("\n").trim().to_owned();
            if joined.is_empty() { None } else { Some(joined) }
        }
        Value::Object(map) => {
            if map.get("isError").and_then(|v| v.as_bool()).unwrap_or(false) {
                return None;
            }
            if map.get("ok").and_then(|v| v.as_bool()).is_some_and(|ok| !ok) {
                return None;
            }
            match map.get("error") {
                Some(Value::Bool(true)) => return None,
                Some(Value::String(text)) if !text.trim().is_empty() => return None,
                _ => {}
            }
            for key in CONTENT_KEYS {
                if let Some(found) = map.get(*key).and_then(extract_element) {
                    return Some(found);
                }
            }
            if let Some(metadata) = map.get("metadata") {
                for key in CONTENT_KEYS {
                    if let Some(found) = metadata.get(*key).and_then(extract_element) {
                        return Some(found);
                    }
                }
            }
            None
        }
    }
}

impl Provider for PaxsenixProvider {
    fn name(&self) -> &'static str {
        self.flavor.name()
    }

    async fn fetch(&self, client: &Client, query: &Query) -> ProviderResult {
        if !self.has_key() || query.title.trim().is_empty() {
            return Ok(None);
        }
        match self.flavor {
            Flavor::AppleMusic => {
                let Some(id) = self.search_id(client, "apple-music/search", query).await? else {
                    return Ok(None);
                };
                self.lyrics_for_id(client, "lyrics/applemusic", &id).await
            }
            Flavor::Spotify => {
                let Some(id) = self.search_id(client, "spotify/search", query).await? else {
                    return Ok(None);
                };
                self.lyrics_for_id(client, "lyrics/spotify", &id).await
            }
            Flavor::Musixmatch => {
                let mut params = vec![
                    ("t", query.title.trim().to_owned()),
                    ("a", query.artist().trim().to_owned()),
                ];
                if let Some(duration) = query.duration_secs {
                    params.push(("d", duration.to_string()));
                }
                let Some(body) = self.get(client, "lyrics/musixmatch", &params).await? else {
                    return Ok(None);
                };
                Ok(extract(&body).and_then(|text| parse_text(&text)))
            }
        }
    }
}
