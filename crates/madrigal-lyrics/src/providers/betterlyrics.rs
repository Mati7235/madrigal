use reqwest::Client;
use serde_json::Value;

use crate::providers::{Provider, ProviderResult, Query, get_text};
use crate::{ttml, Lyrics};

pub const BASE: &str = "https://lyrics-api.boidu.dev/";
const MAX_UNWRAP_DEPTH: usize = 3;
const WALK_KEYS: &[&str] = &["ttml", "lyrics", "data", "result", "response"];

pub struct BetterLyricsProvider {
    base_url: String,
    paths: Vec<String>,
    portato: bool,
}

impl BetterLyricsProvider {
    pub fn new() -> Self {
        Self {
            base_url: BASE.to_owned(),
            paths: vec!["getLyrics".to_owned(), "kugou/getLyrics".to_owned()],
            portato: false,
        }
    }

    pub fn portato() -> Self {
        Self {
            base_url: BASE.to_owned(),
            paths: vec!["qq/getLyrics".to_owned()],
            portato: true,
        }
    }

    pub fn with_base_url(base: impl Into<String>, portato: bool) -> Self {
        let mut provider = if portato { Self::portato() } else { Self::new() };
        provider.base_url = base.into();
        provider
    }

    async fn fetch_ttml(&self, client: &Client, query: &Query) -> Option<Lyrics> {
        let mut params = vec![
            ("s", query.title.trim().to_owned()),
            ("a", query.artist().trim().to_owned()),
        ];
        if let Some(album) = query.album.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
            params.push(("al", album.to_owned()));
        }
        if let Some(duration) = query.duration_secs {
            params.push(("d", duration.to_string()));
        }
        for path in &self.paths {
            let url = format!("{}{path}", self.base_url);
            let Ok(Some(text)) = get_text(client, &url, &params).await else {
                continue;
            };
            if let Some(content) = extract_ttml(&text) {
                if let Ok(parsed) = ttml::parse(&content) {
                    if !parsed.lines.is_empty() {
                        return Some(parsed);
                    }
                }
            }
        }
        None
    }
}

pub fn extract_ttml(text: &str) -> Option<String> {
    let stripped = text.trim_start_matches('\u{feff}').trim();
    if stripped.is_empty() {
        return None;
    }
    if stripped.starts_with('{') || stripped.starts_with('[') {
        if let Ok(payload) = serde_json::from_str::<Value>(stripped) {
            if let Some(found) = walk(&payload, 0) {
                return Some(found);
            }
        }
    }
    if ttml::looks_like_ttml(stripped) {
        return Some(stripped.to_owned());
    }
    None
}

fn walk(element: &Value, depth: usize) -> Option<String> {
    if depth > MAX_UNWRAP_DEPTH {
        return None;
    }
    match element {
        Value::String(text) => {
            if ttml::looks_like_ttml(text) {
                return Some(text.clone());
            }
            let trimmed = text.trim();
            if trimmed.starts_with('{') {
                if let Ok(nested) = serde_json::from_str::<Value>(trimmed) {
                    return walk(&nested, depth + 1);
                }
            }
            None
        }
        Value::Object(map) => {
            for key in WALK_KEYS {
                if let Some(found) = map.get(*key).and_then(|child| walk(child, depth + 1)) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => {
            for item in items {
                if let Some(found) = walk(item, depth + 1) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

impl Default for BetterLyricsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for BetterLyricsProvider {
    fn name(&self) -> &'static str {
        if self.portato { "betterlyrics_portato" } else { "betterlyrics" }
    }

    fn word_level(&self) -> bool {
        true
    }

    async fn fetch(&self, client: &Client, query: &Query) -> ProviderResult {
        if query.title.trim().is_empty() || query.artist().trim().is_empty() {
            return Ok(None);
        }
        Ok(self.fetch_ttml(client, query).await)
    }
}
