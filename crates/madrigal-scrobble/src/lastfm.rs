use std::time::Duration;

use madrigal_core::Track;
use madrigal_settings::{Integrations, LastFmProvider};

use crate::error::{Error, Result};
use crate::http::{self, DEFAULT_TIMEOUT};
use crate::md5;

pub const LASTFM_ENDPOINT: &str = "https://ws.audioscrobbler.com/2.0/";
pub const LIBREFM_ENDPOINT: &str = "https://libre.fm/2.0/";
pub const USER_AGENT: &str = "Madrigal/0.1.0";

pub fn api_sig(params: &[(&str, &str)], secret: &str) -> String {
    let mut sorted: Vec<(&str, &str)> = params.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(b.0).then(a.1.cmp(b.1)));
    let mut raw = String::new();
    for (key, value) in sorted {
        raw.push_str(key);
        raw.push_str(value);
    }
    raw.push_str(secret);
    md5::hexdigest(raw.as_bytes())
}

pub fn normalize_endpoint(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    let (scheme, rest) = trimmed
        .split_once("://")
        .ok_or_else(|| Error::BadUrl(format!("endpoint {trimmed:?} needs http or https")))?;
    if scheme != "http" && scheme != "https" {
        return Err(Error::BadUrl(format!(
            "endpoint {trimmed:?} needs http or https"
        )));
    }
    if rest.contains('?') || rest.contains('#') {
        return Err(Error::BadUrl(format!(
            "endpoint {trimmed:?} must not carry query or fragment"
        )));
    }
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], rest[index..].to_owned()),
        None => (rest, String::new()),
    };
    if authority.is_empty() {
        return Err(Error::BadUrl(format!("endpoint {trimmed:?} needs a host")));
    }
    let path = path.trim_end_matches('/');
    let path = if path.is_empty() { "/2.0" } else { path };
    Ok(format!("{scheme}://{authority}{path}/"))
}

pub fn endpoint_for(provider: &LastFmProvider, custom: &str) -> Result<String> {
    match provider {
        LastFmProvider::LastFm => Ok(LASTFM_ENDPOINT.to_owned()),
        LastFmProvider::LibreFm => Ok(LIBREFM_ENDPOINT.to_owned()),
        LastFmProvider::Custom => normalize_endpoint(custom),
    }
}

#[derive(Debug, Clone)]
pub struct LastFmClient {
    endpoint: String,
    api_key: String,
    secret: String,
    session_key: Option<String>,
    timeout: Duration,
}

impl LastFmClient {
    pub fn new(endpoint: &str, api_key: &str, secret: &str) -> Result<Self> {
        Ok(Self {
            endpoint: normalize_endpoint(endpoint)?,
            api_key: api_key.trim().to_owned(),
            secret: secret.to_owned(),
            session_key: None,
            timeout: DEFAULT_TIMEOUT,
        })
    }

    pub fn from_integrations(
        integrations: &Integrations,
        api_key: &str,
        secret: &str,
        session_key: Option<String>,
    ) -> Result<Self> {
        let endpoint = endpoint_for(
            &integrations.lastfm_provider,
            &integrations.lastfm_custom_endpoint,
        )?;
        let mut client = Self::new(&endpoint, api_key, secret)?;
        client.session_key = session_key
            .map(|key| key.trim().to_owned())
            .filter(|key| !key.is_empty());
        Ok(client)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn session_key(&self) -> Option<&str> {
        self.session_key.as_deref()
    }

    pub fn configured(&self) -> bool {
        !self.api_key.is_empty() && !self.secret.is_empty()
    }

    fn check_configured(&self) -> Result<()> {
        if self.configured() {
            Ok(())
        } else {
            Err(Error::NotConfigured("Last.fm key or secret"))
        }
    }

    fn check_session(&self) -> Result<&str> {
        self.session_key
            .as_deref()
            .ok_or(Error::NotConfigured("Last.fm session key"))
    }

    fn post(
        &self,
        method: &str,
        session: Option<&str>,
        extra: Vec<(String, String)>,
    ) -> Result<String> {
        self.check_configured()?;
        let mut params: Vec<(&str, &str)> = vec![("method", method), ("api_key", &self.api_key)];
        if let Some(key) = session {
            params.push(("sk", key));
        }
        for (key, value) in &extra {
            params.push((key.as_str(), value.as_str()));
        }
        let sig = api_sig(&params, &self.secret);
        let mut body_params: Vec<(&str, &str)> = params;
        body_params.push(("api_sig", &sig));
        body_params.push(("format", "json"));
        let body = http::encode_form(&body_params);
        let response = http::post(
            &self.endpoint,
            "application/x-www-form-urlencoded",
            &[("User-Agent", USER_AGENT)],
            body.as_bytes(),
            self.timeout,
        )?;
        if !(200..300).contains(&response.status) {
            return Err(Error::HttpStatus(response.status));
        }
        if response.body.contains("\"error\"") {
            let value: serde_json::Value = serde_json::from_str(&response.body)
                .map_err(|_| Error::BadResponse("unparseable error payload".to_owned()))?;
            let code = value.get("error").and_then(|v| v.as_i64()).unwrap_or(-1);
            let message = value
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_owned();
            return Err(Error::Api(code, message));
        }
        Ok(response.body)
    }

    pub fn get_mobile_session(&mut self, username: &str, password: &str) -> Result<String> {
        let extra = vec![
            ("username".to_owned(), username.trim().to_owned()),
            ("password".to_owned(), password.to_owned()),
        ];
        let body = self.post("auth.getMobileSession", None, extra)?;
        let value: serde_json::Value = serde_json::from_str(&body)
            .map_err(|_| Error::BadResponse("unparseable session payload".to_owned()))?;
        let key = value
            .get("session")
            .and_then(|s| s.get("key"))
            .and_then(|k| k.as_str())
            .ok_or_else(|| Error::BadResponse("missing session key".to_owned()))?;
        self.session_key = Some(key.to_owned());
        Ok(key.to_owned())
    }

    fn track_params(track: &Track, indexed: bool) -> Vec<(String, String)> {
        let mut params = Vec::new();
        if indexed {
            params.push(("artist[0]".to_owned(), track.artist_text()));
            params.push(("track[0]".to_owned(), track.title.clone()));
        } else {
            params.push(("artist".to_owned(), track.artist_text()));
            params.push(("track".to_owned(), track.title.clone()));
        }
        if let Some(album) = track.album.as_deref().filter(|a| !a.trim().is_empty()) {
            params.push((
                (if indexed { "album[0]" } else { "album" }).to_owned(),
                album.to_owned(),
            ));
        }
        if let Some(number) = track.track_number {
            params.push((
                (if indexed {
                    "trackNumber[0]"
                } else {
                    "trackNumber"
                })
                .to_owned(),
                number.to_string(),
            ));
        }
        if let Some(duration) = track.duration_secs {
            params.push((
                (if indexed { "duration[0]" } else { "duration" }).to_owned(),
                duration.to_string(),
            ));
        }
        params
    }

    pub fn update_now_playing(&self, track: &Track) -> Result<()> {
        let session = self.check_session()?.to_owned();
        self.post(
            "track.updateNowPlaying",
            Some(&session),
            Self::track_params(track, false),
        )?;
        Ok(())
    }

    pub fn scrobble(&self, track: &Track, timestamp: i64) -> Result<()> {
        let session = self.check_session()?.to_owned();
        let mut extra = Self::track_params(track, true);
        extra.push(("timestamp[0]".to_owned(), timestamp.to_string()));
        self.post("track.scrobble", Some(&session), extra)?;
        Ok(())
    }

    pub fn test_connection(&self, track: &Track) -> Result<()> {
        self.update_now_playing(track)
    }
}
