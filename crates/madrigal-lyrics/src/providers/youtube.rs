use regex::Regex;
use reqwest::Client;
use std::sync::OnceLock;

use crate::providers::{Provider, ProviderError, ProviderResult, Query, parse_text};
use crate::Lyrics;

pub const YTM_BASE: &str = "https://music.youtube.com/youtubei/v1/";
pub const YTM_KEY: &str = "AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30";

fn video_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"^[A-Za-z0-9_-]{6,64}$").expect("youtube video"))
}

pub struct YoutubeProvider {
    base_url: String,
    api_key: String,
}

impl YoutubeProvider {
    pub fn new() -> Self {
        Self {
            base_url: YTM_BASE.to_owned(),
            api_key: YTM_KEY.to_owned(),
        }
    }

    pub fn with_base_url(base: impl Into<String>) -> Self {
        Self {
            base_url: base.into(),
            api_key: YTM_KEY.to_owned(),
        }
    }

    async fn browse(&self, client: &Client, body: &serde_json::Value, mobile: bool) -> Result<serde_json::Value, ProviderError> {
        let mut payload = body.clone();
        let context = if mobile {
            serde_json::json!({"context": {"client": {"clientName": "ANDROID_MUSIC", "clientVersion": "7.21.50"}}})
        } else {
            serde_json::json!({"context": {"client": {"clientName": "WEB_REMIX", "clientVersion": "1.20240101.01.00"}}})
        };
        merge(&mut payload, &context);
        let url = format!("{}browse?alt=json&key={}", self.base_url, self.api_key);
        let response = client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|err| ProviderError::Http(err.to_string()))?;
        if !response.status().is_success() {
            return Err(ProviderError::Status(response.status().as_u16()));
        }
        response.json::<serde_json::Value>().await.map_err(|err| ProviderError::Json(err.to_string()))
    }

    async fn lyrics_browse_id(&self, client: &Client, video_id: &str) -> Result<Option<String>, ProviderError> {
        let url = format!("{}next?alt=json&key={}", self.base_url, self.api_key);
        let body = serde_json::json!({
            "videoId": video_id,
            "context": {"client": {"clientName": "WEB_REMIX", "clientVersion": "1.20240101.01.00"}},
        });
        let response = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|err| ProviderError::Http(err.to_string()))?;
        if !response.status().is_success() {
            return Err(ProviderError::Status(response.status().as_u16()));
        }
        let payload: serde_json::Value =
            response.json().await.map_err(|err| ProviderError::Json(err.to_string()))?;
        Ok(find_lyrics_browse_id(&payload))
    }

    pub fn timed_to_lyrics(payload: &serde_json::Value) -> Option<Lyrics> {
        let data = payload
            .pointer("/contents/elementRenderer/newElement/type/componentType/model/timedLyricsModel/lyricsData")?;
        let tracks = data.get("timedLyricsData")?.as_array()?;
        let mut rendered = Vec::new();
        let mut any_timed = false;
        for track in tracks {
            let text = track.get("lyricLine").and_then(|v| v.as_str()).unwrap_or_default();
            match track.pointer("/cueRange/startTimeMilliseconds").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok()) {
                Some(start_ms) => {
                    any_timed = true;
                    rendered.push(format!("[{}]{text}", format_lrc_ms(start_ms)));
                }
                None => rendered.push(text.to_owned()),
            }
        }
        if rendered.is_empty() {
            return None;
        }
        if any_timed {
            parse_text(&rendered.join("\n"))
        } else {
            parse_text(&tracks.iter().filter_map(|t| t.get("lyricLine").and_then(|v| v.as_str())).collect::<Vec<_>>().join("\n"))
        }
    }

    pub fn plain_to_lyrics(payload: &serde_json::Value) -> Option<Lyrics> {
        let shelf = find_description_shelf(payload)?;
        let runs = shelf.get("description")?.get("runs")?.as_array()?;
        let text = runs
            .iter()
            .filter_map(|run| run.get("text").and_then(|v| v.as_str()))
            .collect::<String>();
        parse_text(&text)
    }
}

fn merge(into: &mut serde_json::Value, from: &serde_json::Value) {
    match (into, from) {
        (serde_json::Value::Object(into_map), serde_json::Value::Object(from_map)) => {
            for (key, value) in from_map {
                merge(into_map.entry(key.clone()).or_insert(serde_json::Value::Null), value);
            }
        }
        (into_slot, from_value) => *into_slot = from_value.clone(),
    }
}

pub fn find_lyrics_browse_id(payload: &serde_json::Value) -> Option<String> {
    let tabs = payload.pointer("/contents/singleColumnMusicWatchNextResultsRenderer/tabbedRenderer/watchNextTabbedResultsRenderer/tabs")?.as_array()?;
    for tab in tabs {
        let Some(renderer) = tab.get("tabRenderer") else {
            continue;
        };
        if renderer.get("unselectable").is_some() {
            continue;
        }
        let Some(endpoint) = renderer.get("endpoint").and_then(|e| e.get("browseEndpoint")) else {
            continue;
        };
        let Some(page_type) = endpoint
            .pointer("/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        if page_type == "MUSIC_PAGE_TYPE_TRACK_LYRICS" {
            return endpoint.get("browseId").and_then(|v| v.as_str()).map(str::to_owned);
        }
    }
    None
}

fn find_description_shelf(payload: &serde_json::Value) -> Option<&serde_json::Value> {
    match payload {
        serde_json::Value::Object(map) => {
            if let Some(shelf) = map.get("musicDescriptionShelfRenderer") {
                return Some(shelf);
            }
            map.values().find_map(find_description_shelf)
        }
        serde_json::Value::Array(items) => items.iter().find_map(find_description_shelf),
        _ => None,
    }
}

fn format_lrc_ms(start_ms: f64) -> String {
    let total = start_ms.max(0.0) / 1000.0;
    let minutes = (total / 60.0).floor() as i64;
    let seconds = total % 60.0;
    format!("{minutes:02}:{seconds:05.2}")
}

impl Default for YoutubeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for YoutubeProvider {
    fn name(&self) -> &'static str {
        "youtube"
    }

    async fn fetch(&self, client: &Client, query: &Query) -> ProviderResult {
        let Some(video_id) = query.video_id.as_deref().filter(|id| video_re().is_match(id)) else {
            return Ok(None);
        };
        let Some(browse_id) = self.lyrics_browse_id(client, video_id).await? else {
            return Ok(None);
        };
        let timed = self.browse(client, &serde_json::json!({"browseId": browse_id}), true).await?;
        if let Some(lyrics) = Self::timed_to_lyrics(&timed) {
            if lyrics.synced {
                return Ok(Some(lyrics));
            }
        }
        let plain = self.browse(client, &serde_json::json!({"browseId": browse_id}), false).await?;
        Ok(Self::plain_to_lyrics(&plain))
    }
}
