use reqwest::Client;

use crate::providers::{Provider, ProviderResult, Query, get_json, get_text, parse_text};
use crate::{ttml, Lyrics};

pub const TTML_PATH: &str = "v1/ttml/get";
pub const LRC_PATH: &str = "v2/lyrics/get";

pub const DEFAULT_MIRRORS: &[&str] = &[
    "https://lyricsplus.binimum.org/",
    "https://lyricsplus.prjktla.my.id/",
    "https://lyricsplus.prjktla.workers.dev/",
    "https://lyricsplus.atomix.one/",
    "https://lyricsplus-seven.vercel.app/",
];

pub struct YouLyPlusProvider {
    mirrors: Vec<String>,
}

impl YouLyPlusProvider {
    pub fn new() -> Self {
        Self {
            mirrors: DEFAULT_MIRRORS.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    pub fn with_mirrors(mirrors: Vec<String>) -> Self {
        Self { mirrors }
    }

    fn params(query: &Query) -> Vec<(&str, String)> {
        let mut params = vec![
            ("title", query.title.trim().to_owned()),
            ("artist", query.artist().trim().to_owned()),
        ];
        if let Some(album) = query.album.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
            params.push(("album", album.to_owned()));
        }
        if let Some(duration) = query.duration_secs {
            params.push(("duration", duration.to_string()));
        }
        params
    }

    async fn fetch_ttml(&self, client: &Client, params: &[(&str, String)]) -> Option<Lyrics> {
        for base in &self.mirrors {
            let url = format!("{base}{TTML_PATH}");
            let Ok(Some(text)) = get_text(client, &url, params).await else {
                continue;
            };
            let text = text.trim().to_owned();
            if text.is_empty() {
                continue;
            }
            if text.starts_with('<') {
                if ttml::looks_like_ttml(&text) {
                    if let Ok(parsed) = ttml::parse(&text) {
                        if !parsed.lines.is_empty() {
                            return Some(parsed);
                        }
                    }
                }
                continue;
            }
            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(candidate) = payload.get("ttml").and_then(|v| v.as_str()) {
                    if ttml::looks_like_ttml(candidate) {
                        if let Ok(parsed) = ttml::parse(candidate) {
                            if !parsed.lines.is_empty() {
                                return Some(parsed);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    async fn fetch_lrc(&self, client: &Client, params: &[(&str, String)]) -> Option<Lyrics> {
        for base in &self.mirrors {
            let url = format!("{base}{LRC_PATH}");
            let Ok(payload) = get_json(client, &url, params).await else {
                continue;
            };
            if let Some(lyrics) = Self::json_to_lyrics(&payload) {
                return Some(lyrics);
            }
        }
        None
    }

    pub fn json_to_lyrics(payload: &serde_json::Value) -> Option<Lyrics> {
        let lines = payload.get("lyrics")?.as_array()?;
        if lines.is_empty() {
            return None;
        }
        let mut synced = false;
        let mut rendered: Vec<String> = Vec::new();
        for line in lines {
            let text = line.get("text").and_then(|v| v.as_str()).unwrap_or_default().trim();
            if text.is_empty() {
                continue;
            }
            match line.get("time").and_then(|v| v.as_f64()) {
                Some(stamp) => {
                    synced = true;
                    rendered.push(format!("[{}]{text}", format_timestamp_ms(stamp)));
                }
                None => rendered.push(text.to_owned()),
            }
        }
        if rendered.is_empty() {
            return None;
        }
        let joined = rendered.join("\n");
        match parse_text(&joined) {
            Some(lyrics) if !synced || lyrics.synced => Some(lyrics),
            _ => None,
        }
    }
}

fn format_timestamp_ms(milliseconds: f64) -> String {
    let total = milliseconds.max(0.0) / 1000.0;
    let minutes = (total / 60.0).floor() as i64;
    let seconds = total % 60.0;
    format!("{minutes:02}:{seconds:05.2}")
}

impl Default for YouLyPlusProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for YouLyPlusProvider {
    fn name(&self) -> &'static str {
        "youlyplus"
    }

    fn word_level(&self) -> bool {
        true
    }

    async fn fetch(&self, client: &Client, query: &Query) -> ProviderResult {
        if query.title.trim().is_empty() || query.artist().trim().is_empty() {
            return Ok(None);
        }
        let params = Self::params(query);
        if let Some(lyrics) = self.fetch_ttml(client, &params).await {
            return Ok(Some(lyrics));
        }
        Ok(self.fetch_lrc(client, &params).await)
    }
}
