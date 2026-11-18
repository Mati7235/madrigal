use std::time::Duration;

use madrigal_core::Track;
use madrigal_settings::Integrations;

use crate::error::{Error, Result};
use crate::http::{self, DEFAULT_TIMEOUT};

pub const DEFAULT_ENDPOINT: &str = "https://api.listenbrainz.org/1/submit-listens";
pub const SUBMISSION_CLIENT: &str = "Madrigal";

#[derive(Debug, Clone)]
pub struct ListenBrainzClient {
    endpoint: String,
    token: String,
    timeout: Duration,
}

impl ListenBrainzClient {
    pub fn new(endpoint: &str, token: &str) -> Self {
        Self {
            endpoint: endpoint.trim().to_owned(),
            token: token.trim().to_owned(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn from_integrations(integrations: &Integrations) -> Self {
        Self::new(DEFAULT_ENDPOINT, &integrations.listenbrainz_token)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn configured(&self) -> bool {
        !self.token.is_empty()
    }

    fn track_metadata(track: &Track) -> serde_json::Value {
        let mut info = serde_json::Map::new();
        info.insert(
            "duration_ms".to_owned(),
            serde_json::Value::from(track.duration_secs.unwrap_or(0) as u64 * 1000),
        );
        info.insert(
            "submission_client".to_owned(),
            serde_json::Value::from(SUBMISSION_CLIENT),
        );
        let mut track_metadata = serde_json::Map::new();
        track_metadata.insert(
            "artist_name".to_owned(),
            serde_json::Value::from(track.artists.join(" & ")),
        );
        track_metadata.insert(
            "track_name".to_owned(),
            serde_json::Value::from(track.title.clone()),
        );
        if let Some(album) = track.album.as_deref().filter(|a| !a.trim().is_empty()) {
            track_metadata.insert(
                "release_name".to_owned(),
                serde_json::Value::from(album.to_owned()),
            );
        }
        track_metadata.insert(
            "additional_info".to_owned(),
            serde_json::Value::Object(info),
        );
        serde_json::Value::Object(track_metadata)
    }

    fn submit(&self, listen_type: &str, payload: serde_json::Value) -> Result<()> {
        if !self.configured() {
            return Err(Error::NotConfigured("ListenBrainz token"));
        }
        let body = serde_json::json!({
            "listen_type": listen_type,
            "payload": payload,
        });
        let text = serde_json::to_string(&body)
            .map_err(|_| Error::BadResponse("unserializable listen payload".to_owned()))?;
        let auth = format!("Token {}", self.token);
        let response = http::post(
            &self.endpoint,
            "application/json",
            &[("Authorization", auth.as_str())],
            text.as_bytes(),
            self.timeout,
        )?;
        if (200..300).contains(&response.status) {
            Ok(())
        } else {
            Err(Error::HttpStatus(response.status))
        }
    }

    pub fn now_playing(&self, track: &Track) -> Result<()> {
        let listen = serde_json::json!({ "track_metadata": Self::track_metadata(track) });
        self.submit("playing_now", serde_json::Value::Array(vec![listen]))
    }

    pub fn submit_listen(&self, track: &Track, listened_at: i64) -> Result<()> {
        let listen = serde_json::json!({
            "listened_at": listened_at,
            "track_metadata": Self::track_metadata(track),
        });
        self.submit("single", serde_json::Value::Array(vec![listen]))
    }

    pub fn test_connection(&self, track: &Track) -> Result<()> {
        self.now_playing(track)
    }
}
