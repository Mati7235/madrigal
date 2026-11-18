pub mod betterlyrics;
pub mod kugou;
pub mod lrclib;
pub mod megalobiz;
pub mod paxsenix;
pub mod simpmusic;
pub mod tidal;
pub mod unison;
pub mod youlyplus;
pub mod youtube;
pub mod youtube_subtitle;

use std::fmt;
use std::time::Duration;

use reqwest::Client;

use crate::{lrc, lrc_enhanced, Lyrics};

pub const PROVIDER_TIMEOUT: Duration = Duration::from_secs(8);
pub const USER_AGENT: &str = "Madrigal/0.1.0";

#[derive(Debug, Clone, Default)]
pub struct Query {
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub duration_secs: Option<u32>,
    pub video_id: Option<String>,
}

impl Query {
    pub fn artist(&self) -> &str {
        self.artists.first().map(String::as_str).unwrap_or_default()
    }
}

#[derive(Debug)]
pub enum ProviderError {
    Http(String),
    Status(u16),
    Json(String),
    Unsupported(&'static str),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::Http(err) => write!(f, "http error: {err}"),
            ProviderError::Status(code) => write!(f, "unexpected status: {code}"),
            ProviderError::Json(err) => write!(f, "bad payload: {err}"),
            ProviderError::Unsupported(name) => write!(f, "{name} provider not wired yet"),
        }
    }
}

impl std::error::Error for ProviderError {}

pub type ProviderResult = Result<Option<Lyrics>, ProviderError>;

pub trait Provider {
    fn name(&self) -> &'static str;
    fn word_level(&self) -> bool {
        false
    }
    fn fetch(&self, client: &Client, query: &Query) -> impl std::future::Future<Output = ProviderResult> + Send;
}

pub fn parse_text(text: &str) -> Option<Lyrics> {
    if text.trim().is_empty() {
        return None;
    }
    if let Ok(synced) = lrc_enhanced::parse(text) {
        return Some(synced);
    }
    let parsed = lrc::parse(text);
    if parsed.lines.is_empty() && !parsed.synced {
        return None;
    }
    Some(parsed)
}

pub fn http_client() -> Client {
    Client::builder()
        .timeout(PROVIDER_TIMEOUT + Duration::from_secs(1))
        .user_agent(USER_AGENT)
        .build()
        .expect("http client")
}

pub(crate) fn get_text(
    client: &Client,
    url: &str,
    params: &[(&str, String)],
) -> impl std::future::Future<Output = Result<Option<String>, ProviderError>> + Send {
    let client = client.clone();
    let url = url.to_owned();
    let params: Vec<(String, String)> = params
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect();
    async move {
        let response = client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|err| ProviderError::Http(err.to_string()))?;
        if !response.status().is_success() {
            return Err(ProviderError::Status(response.status().as_u16()));
        }
        response
            .text()
            .await
            .map(Some)
            .map_err(|err| ProviderError::Http(err.to_string()))
    }
}

pub(crate) fn get_json(
    client: &Client,
    url: &str,
    params: &[(&str, String)],
) -> impl std::future::Future<Output = Result<serde_json::Value, ProviderError>> + Send {
    let client = client.clone();
    let url = url.to_owned();
    let params: Vec<(String, String)> = params
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect();
    async move {
        let response = client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|err| ProviderError::Http(err.to_string()))?;
        if !response.status().is_success() {
            return Err(ProviderError::Status(response.status().as_u16()));
        }
        response
            .json::<serde_json::Value>()
            .await
            .map_err(|err| ProviderError::Json(err.to_string()))
    }
}
