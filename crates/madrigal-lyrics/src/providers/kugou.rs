use base64::Engine;
use regex::Regex;
use reqwest::Client;
use std::sync::OnceLock;

use crate::matching::{matches_artist, matches_title, normalize_query_artist, normalize_query_title};
use crate::providers::{Provider, ProviderError, ProviderResult, Query, get_json, get_text, parse_text};

pub const PAGE_SIZE: u32 = 8;
pub const DURATION_TOLERANCE: u32 = 8;
const HEAD_CUT_LIMIT: usize = 30;

fn accepted_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"^\[\d\d:\d\d\.\d{2,3}\].*").expect("kugou accepted"))
}

fn banned_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r".+].+[:：].+").expect("kugou banned"))
}

pub struct KugouProvider {
    song_base: String,
    lyrics_base: String,
}

impl KugouProvider {
    pub fn new() -> Self {
        Self {
            song_base: "https://mobileservice.kugou.com".to_owned(),
            lyrics_base: "https://lyrics.kugou.com".to_owned(),
        }
    }

    pub fn with_bases(song: impl Into<String>, lyrics: impl Into<String>) -> Self {
        Self {
            song_base: song.into(),
            lyrics_base: lyrics.into(),
        }
    }

    async fn search_songs(
        &self,
        client: &Client,
        title: &str,
        artist: &str,
    ) -> Result<Vec<serde_json::Value>, ProviderError> {
        let keyword = urlencoding(title, artist);
        let url = format!(
            "{}/api/v3/search/song?version=9108&plat=0&pagesize={PAGE_SIZE}&showtype=0&keyword={keyword}",
            self.song_base
        );
        match get_text(client, &url, &[]).await? {
            Some(text) => {
                let payload: serde_json::Value =
                    serde_json::from_str(&text).map_err(|err| ProviderError::Json(err.to_string()))?;
                Ok(payload
                    .pointer("/data/info")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default())
            }
            None => Ok(Vec::new()),
        }
    }

    async fn search_lyrics(
        &self,
        client: &Client,
        params: &[(&str, String)],
    ) -> Result<Vec<serde_json::Value>, ProviderError> {
        let url = format!("{}/search", self.lyrics_base);
        let payload = get_json(client, &url, params).await?;
        Ok(payload
            .get("candidates")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default())
    }

    async fn candidate(
        &self,
        client: &Client,
        title: &str,
        artist: &str,
        duration: Option<u32>,
    ) -> Result<Option<serde_json::Value>, ProviderError> {
        let mut weak: Option<serde_json::Value> = None;
        for song in self.search_songs(client, title, artist).await? {
            let song_duration = song.get("duration").and_then(|v| v.as_u64()).map(|v| v as u32);
            let Some(hash) = song.get("hash").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) else {
                continue;
            };
            if !matches_title(title, song.get("songname").and_then(|v| v.as_str()).unwrap_or_default(), 0.6) {
                continue;
            }
            if let (Some(want), Some(have)) = (duration, song_duration) {
                if want.abs_diff(have) > DURATION_TOLERANCE {
                    continue;
                }
            }
            let candidates = self.search_lyrics(client, &[("ver", "1".to_owned()), ("man", "yes".to_owned()), ("client", "pc".to_owned()), ("hash", hash.to_owned())]).await?;
            if candidates.is_empty() {
                continue;
            }
            if matches_artist(artist, song.get("singername").and_then(|v| v.as_str()).unwrap_or_default()) {
                return Ok(candidates.into_iter().next());
            }
            if weak.is_none() {
                weak = candidates.into_iter().next();
            }
        }
        let mut params = vec![
            ("ver", "1".to_owned()),
            ("man", "yes".to_owned()),
            ("client", "pc".to_owned()),
            ("keyword", format!("{title} - {artist}")),
        ];
        if let Some(seconds) = duration {
            params.push(("duration", (seconds as u64 * 1000).to_string()));
        }
        for item in self.search_lyrics(client, &params).await? {
            if !matches_title(title, item.get("song").and_then(|v| v.as_str()).unwrap_or_default(), 0.6) {
                continue;
            }
            if matches_artist(artist, item.get("singer").and_then(|v| v.as_str()).unwrap_or_default()) {
                return Ok(Some(item));
            }
            if weak.is_none() {
                weak = Some(item);
            }
        }
        Ok(weak)
    }

    async fn download(
        &self,
        client: &Client,
        candidate: &serde_json::Value,
    ) -> Result<Option<String>, ProviderError> {
        let id = candidate.get("id").map(|v| v.to_string()).unwrap_or_default();
        let accesskey = candidate.get("accesskey").and_then(|v| v.as_str()).unwrap_or_default();
        if id.is_empty() || accesskey.is_empty() {
            return Ok(None);
        }
        let url = format!("{}/download", self.lyrics_base);
        let payload = get_json(
            client,
            &url,
            &[
                ("fmt", "lrc".to_owned()),
                ("charset", "utf8".to_owned()),
                ("client", "pc".to_owned()),
                ("ver", "1".to_owned()),
                ("id", id),
                ("accesskey", accesskey.to_owned()),
            ],
        )
        .await?;
        let Some(content) = payload.get("content").and_then(|v| v.as_str()) else {
            return Ok(None);
        };
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(content)
            .map_err(|err| ProviderError::Json(err.to_string()))?;
        Ok(Some(String::from_utf8_lossy(&decoded).into_owned()))
    }

    pub fn normalize(text: &str) -> String {
        let staged = text.replace("&apos;", "'");
        let lines: Vec<&str> = staged.lines().filter(|line| accepted_re().is_match(line)).collect();
        if lines.is_empty() {
            return String::new();
        }
        let mut head_cut = 0;
        for index in (0..=(lines.len() - 1).min(HEAD_CUT_LIMIT)).rev() {
            if banned_re().is_match(lines[index]) {
                head_cut = index + 1;
                break;
            }
        }
        let filtered = &lines[head_cut.min(lines.len())..];
        let mut tail_cut = 0;
        let upper = (lines.len() as i64 - HEAD_CUT_LIMIT as i64).min(lines.len() as i64 - 1);
        if upper >= 0 {
            for index in (0..=upper as usize).rev() {
                if banned_re().is_match(lines[lines.len() - 1 - index]) {
                    tail_cut = index + 1;
                    break;
                }
            }
        }
        let kept = &filtered[..filtered.len().saturating_sub(tail_cut)];
        kept.join("\n")
    }
}

fn urlencoding(title: &str, artist: &str) -> String {
    percent_encode(&format!("{title} - {artist}"))
}

pub(crate) fn percent_encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

impl Default for KugouProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for KugouProvider {
    fn name(&self) -> &'static str {
        "kugou"
    }

    async fn fetch(&self, client: &Client, query: &Query) -> ProviderResult {
        let title = normalize_query_title(&query.title);
        let artist = normalize_query_artist(query.artist());
        let Some(candidate) = self.candidate(client, &title, &artist, query.duration_secs).await? else {
            return Ok(None);
        };
        let Some(raw) = self.download(client, &candidate).await? else {
            return Ok(None);
        };
        let normalized = Self::normalize(&raw);
        if normalized.is_empty() {
            return Ok(None);
        }
        Ok(parse_text(&normalized))
    }
}
