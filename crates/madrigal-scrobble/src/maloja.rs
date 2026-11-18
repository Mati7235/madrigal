use std::time::Duration;

use madrigal_core::Track;
use madrigal_settings::Integrations;

use crate::error::{Error, Result};
use crate::http::{self, DEFAULT_TIMEOUT};

pub const NOWPLAYING_PATH: &str = "/apis/track/nowplaying";
pub const SCROBBLE_PATH: &str = "/apis/track/scrobble";
pub const TEST_ARTIST: &str = "upstream";
pub const TEST_TITLE: &str = "Connection test";

fn loopback(host: &str) -> bool {
    let normalized = host.trim().to_lowercase();
    let normalized = normalized.trim_end_matches('.');
    normalized == "localhost"
        || normalized == "::1"
        || normalized == "127.0.0.1"
        || normalized.starts_with("127.")
}

fn host_of(server: &str) -> Option<String> {
    let rest = server.split_once("://")?.1;
    let authority = rest.split('/').next()?;
    if let Some(stripped) = authority.strip_prefix('[') {
        return stripped.split(']').next().map(str::to_owned);
    }
    Some(authority.split(':').next().unwrap_or(authority).to_owned())
}

fn require_scheme(server: &str) -> Result<()> {
    if server.starts_with("https://") {
        return Ok(());
    }
    if server.starts_with("http://") {
        let host = host_of(server).unwrap_or_default();
        if loopback(&host) {
            return Ok(());
        }
    }
    Err(Error::BadUrl("Maloja server requires https".to_owned()))
}

#[derive(Debug, Clone)]
pub struct MalojaClient {
    server: String,
    api_key: String,
    timeout: Duration,
}

impl MalojaClient {
    pub fn new(server: &str, api_key: &str) -> Self {
        Self {
            server: server.trim().trim_end_matches('/').to_owned(),
            api_key: api_key.trim().to_owned(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn from_integrations(integrations: &Integrations) -> Self {
        Self::new(&integrations.maloja_server, &integrations.maloja_api_key)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn server(&self) -> &str {
        &self.server
    }

    pub fn configured(&self) -> bool {
        !self.server.is_empty() && !self.api_key.is_empty()
    }

    fn check_configured(&self) -> Result<()> {
        if self.configured() {
            Ok(())
        } else {
            Err(Error::NotConfigured("Maloja server or key"))
        }
    }

    fn post(&self, path: &str, fields: Vec<(&str, String)>) -> Result<()> {
        self.check_configured()?;
        let url = format!("{}{path}", self.server);
        require_scheme(&url)?;
        let owned: Vec<(String, String)> = fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        let mut refs: Vec<(&str, &str)> = vec![("key", self.api_key.as_str())];
        for (key, value) in &owned {
            refs.push((key.as_str(), value.as_str()));
        }
        let body = http::encode_form(&refs);
        let response = http::post(
            &url,
            "application/x-www-form-urlencoded",
            &[],
            body.as_bytes(),
            self.timeout,
        )?;
        if (200..300).contains(&response.status) {
            Ok(())
        } else {
            Err(Error::HttpStatus(response.status))
        }
    }

    fn payload(track: &Track) -> Vec<(&'static str, String)> {
        vec![
            ("artist", artist_of(track)),
            ("title", track.title.clone()),
            ("album", track.album.clone().unwrap_or_default()),
            ("length", track.duration_secs.unwrap_or(0).to_string()),
        ]
    }

    pub fn now_playing(&self, track: &Track) -> Result<()> {
        self.post(NOWPLAYING_PATH, Self::payload(track))
    }

    pub fn scrobble(&self, track: &Track, started_at: i64) -> Result<()> {
        let mut fields = Self::payload(track);
        fields.push(("time", started_at.to_string()));
        self.post(SCROBBLE_PATH, fields)
    }

    pub fn test_connection(&self) -> Result<()> {
        self.post(
            NOWPLAYING_PATH,
            vec![
                ("artist", TEST_ARTIST.to_owned()),
                ("title", TEST_TITLE.to_owned()),
                ("album", String::new()),
                ("length", "0".to_owned()),
            ],
        )
    }
}

fn artist_of(track: &Track) -> String {
    let text = track.artist_text();
    if text.trim().is_empty() {
        "Unknown".to_owned()
    } else {
        text
    }
}
