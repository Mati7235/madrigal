use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::Error;

pub const SCOPE: &str = "r_usr w_usr w_sub";
pub const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
pub const SECRET_ACCOUNT: &str = "tidal";
pub const EXPIRY_MARGIN_SECS: u64 = 60;

#[derive(Debug, Clone)]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
}

impl Credentials {
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Credentials {
        Credentials {
            client_id: client_id.into(),
            client_secret: client_secret.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Endpoints {
    pub device_authorization_url: String,
    pub token_url: String,
    pub api_base: String,
    pub scope: String,
}

impl Default for Endpoints {
    fn default() -> Endpoints {
        Endpoints {
            device_authorization_url:
                "https://auth.tidal.com/v1/oauth2/device_authorization".to_owned(),
            token_url: "https://auth.tidal.com/v1/oauth2/token".to_owned(),
            api_base: "https://api.tidal.com/".to_owned(),
            scope: SCOPE.to_owned(),
        }
    }
}

impl Endpoints {
    pub fn api_url(&self, path: &str) -> String {
        format!("{}{}", self.api_base.trim_end_matches('/'), path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceLink {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in_secs: u64,
    pub interval_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSet {
    pub token_type: String,
    pub access_token: String,
    pub refresh_token: String,
    pub session_id: String,
    pub country_code: String,
    pub expires_at: u64,
}

impl TokenSet {
    pub fn expired(&self, now_epoch: u64) -> bool {
        now_epoch.saturating_add(EXPIRY_MARGIN_SECS) >= self.expires_at
    }
}

pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|span| span.as_secs())
        .unwrap_or(0)
}

#[derive(Debug)]
pub enum Poll {
    Authorized {
        token_type: String,
        access_token: String,
        refresh_token: String,
        expires_in: u64,
    },
    Pending,
    Expired,
}

pub struct Client {
    http: reqwest::blocking::Client,
    endpoints: Endpoints,
    credentials: Credentials,
}

impl Client {
    pub fn new(credentials: Credentials) -> Result<Client, Error> {
        Client::with_endpoints(credentials, Endpoints::default())
    }

    pub fn with_endpoints(
        credentials: Credentials,
        endpoints: Endpoints,
    ) -> Result<Client, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(5))
            .no_proxy()
            .build()
            .map_err(|err| Error::Transport(err.to_string()))?;
        Ok(Client {
            http,
            endpoints,
            credentials,
        })
    }

    pub fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    fn auth_header(token: &TokenSet) -> String {
        format!("{} {}", token.token_type, token.access_token)
    }

    pub fn request_link(&self) -> Result<DeviceLink, Error> {
        let response = self
            .http
            .post(&self.endpoints.device_authorization_url)
            .form(&[
                ("client_id", self.credentials.client_id.as_str()),
                ("scope", self.endpoints.scope.as_str()),
            ])
            .send()
            .map_err(|err| Error::Transport(err.to_string()))?;
        if !response.status().is_success() {
            return Err(Error::Status(response.status().as_u16()));
        }
        let body = crate::body_json(response)?;
        let get = |key: &str| {
            body.get(key)
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| Error::Protocol(format!("link lacks {key}")))
        };
        let expires_in = body
            .get("expiresIn")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| Error::Protocol("link lacks expiresIn".to_owned()))?;
        let interval = body
            .get("interval")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| Error::Protocol("link lacks interval".to_owned()))?;
        Ok(DeviceLink {
            device_code: get("deviceCode")?,
            user_code: get("userCode")?,
            verification_uri: get("verificationUri")?,
            verification_uri_complete: get("verificationUriComplete")?,
            expires_in_secs: expires_in,
            interval_secs: interval,
        })
    }

    pub fn poll(&self, link: &DeviceLink) -> Result<Poll, Error> {
        let response = self
            .http
            .post(&self.endpoints.token_url)
            .form(&[
                ("client_id", self.credentials.client_id.as_str()),
                ("client_secret", self.credentials.client_secret.as_str()),
                ("device_code", link.device_code.as_str()),
                ("grant_type", DEVICE_GRANT),
                ("scope", self.endpoints.scope.as_str()),
            ])
            .send()
            .map_err(|err| Error::Transport(err.to_string()))?;
        if response.status().is_success() {
            let body = crate::body_json(response)?;
            let text = |key: &str| {
                body.get(key)
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| Error::Protocol(format!("token lacks {key}")))
            };
            let expires_in = body
                .get("expires_in")
                .and_then(|value| value.as_u64())
                .ok_or_else(|| Error::Protocol("token lacks expires_in".to_owned()))?;
            return Ok(Poll::Authorized {
                token_type: text("token_type")?,
                access_token: text("access_token")?,
                refresh_token: text("refresh_token")?,
                expires_in,
            });
        }
        let code = response.status().as_u16();
        let name = response
            .json::<serde_json::Value>()
            .ok()
            .and_then(|body| body.get("error").and_then(|value| value.as_str()).map(str::to_owned))
            .unwrap_or_default();
        if name == "expired_token" {
            return Ok(Poll::Expired);
        }
        if (400..500).contains(&code) {
            return Ok(Poll::Pending);
        }
        Err(Error::Status(code))
    }

    fn complete(&self, granted: Granted) -> Result<TokenSet, Error> {
        let response = self
            .http
            .get(self.endpoints.api_url("/v1/sessions"))
            .header("authorization", Self::auth_header(&granted.token()))
            .send()
            .map_err(|err| Error::Transport(err.to_string()))?;
        if !response.status().is_success() {
            return Err(Error::Status(response.status().as_u16()));
        }
        let body = crate::body_json(response)?;
        let text = |key: &str| {
            body.get(key)
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| Error::Protocol(format!("session lacks {key}")))
        };
        let token = TokenSet {
            token_type: granted.token_type,
            access_token: granted.access_token,
            refresh_token: granted.refresh_token,
            session_id: text("sessionId")?,
            country_code: text("countryCode").unwrap_or_else(|_| "US".to_owned()),
            expires_at: now_epoch().saturating_add(granted.expires_in),
        };
        self.save_session(&token)?;
        Ok(token)
    }

    pub fn authorize(
        &self,
        emit: impl Fn(&str, u64),
    ) -> Result<TokenSet, Error> {
        let link = self.request_link()?;
        let mut url = link.verification_uri_complete.clone();
        if !url.contains("://") {
            url = format!("https://{url}");
        }
        if !url.starts_with("https://") {
            return Err(Error::Auth("verification url is not https".to_owned()));
        }
        emit(&url, link.expires_in_secs);
        let deadline = Instant::now() + Duration::from_secs(link.expires_in_secs);
        loop {
            match self.poll(&link)? {
                Poll::Authorized {
                    token_type,
                    access_token,
                    refresh_token,
                    expires_in,
                } => {
                    return self.complete(Granted {
                        token_type,
                        access_token,
                        refresh_token,
                        expires_in,
                    });
                }
                Poll::Expired => return Err(Error::LoginExpired),
                Poll::Pending => {}
            }
            if Instant::now() >= deadline {
                return Err(Error::LoginExpired);
            }
            let wait = Duration::from_secs(link.interval_secs.max(1));
            let remaining = deadline.saturating_duration_since(Instant::now());
            std::thread::sleep(wait.min(remaining.max(Duration::from_secs(1))));
            if Instant::now() >= deadline {
                return Err(Error::LoginExpired);
            }
        }
    }

    pub fn refresh(&self, stored: &TokenSet) -> Result<TokenSet, Error> {
        let response = self
            .http
            .post(&self.endpoints.token_url)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", stored.refresh_token.as_str()),
                ("client_id", self.credentials.client_id.as_str()),
                ("client_secret", self.credentials.client_secret.as_str()),
            ])
            .send()
            .map_err(|err| Error::Transport(err.to_string()))?;
        if !response.status().is_success() {
            return Err(Error::Auth("refresh rejected".to_owned()));
        }
        let body = crate::body_json(response)?;
        let access = body
            .get("access_token")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::Protocol("refresh lacks access_token".to_owned()))?;
        let expires_in = body
            .get("expires_in")
            .and_then(|value| value.as_u64())
            .unwrap_or(3600);
        let token_type = body
            .get("token_type")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .unwrap_or(&stored.token_type)
            .to_owned();
        Ok(TokenSet {
            token_type,
            access_token: access.to_owned(),
            refresh_token: stored.refresh_token.clone(),
            session_id: stored.session_id.clone(),
            country_code: stored.country_code.clone(),
            expires_at: now_epoch().saturating_add(expires_in),
        })
    }

    pub fn bearer(&self, token: &TokenSet) -> String {
        Self::auth_header(token)
    }

    pub fn get(
        &self,
        path: &str,
        token: &TokenSet,
        params: &[(&str, String)],
    ) -> Result<reqwest::blocking::Response, Error> {
        let mut query: Vec<(&str, String)> = vec![
            ("sessionId", token.session_id.clone()),
            ("countryCode", token.country_code.clone()),
        ];
        query.extend(params.iter().cloned());
        self.http
            .get(self.endpoints.api_url(path))
            .header("authorization", Self::auth_header(token))
            .query(&query)
            .send()
            .map_err(|err| Error::Transport(err.to_string()))
    }

    pub fn save_session(&self, token: &TokenSet) -> Result<(), Error> {
        if token.token_type.is_empty()
            || token.access_token.is_empty()
            || token.refresh_token.is_empty()
            || token.session_id.is_empty()
        {
            return Err(Error::Auth("refusing to store incomplete token".to_owned()));
        }
        let raw = serde_json::to_string(token)
            .map_err(|err| Error::Protocol(err.to_string()))?;
        madrigal_secrets::set_secret(SECRET_ACCOUNT, &raw)
            .map_err(|err| Error::Secrets(err.to_string()))
    }

    pub fn stored_session(&self) -> Result<Option<TokenSet>, Error> {
        let raw = madrigal_secrets::get_secret(SECRET_ACCOUNT)
            .map_err(|err| Error::Secrets(err.to_string()))?;
        let raw = match raw {
            Some(raw) => raw,
            None => return Ok(None),
        };
        let token: TokenSet = match serde_json::from_str(&raw) {
            Ok(token) => token,
            Err(_) => return Ok(None),
        };
        if token.token_type.is_empty()
            || token.access_token.is_empty()
            || token.refresh_token.is_empty()
            || token.session_id.is_empty()
        {
            return Ok(None);
        }
        Ok(Some(token))
    }

    pub fn is_logged_in(&self) -> bool {
        self.stored_session().ok().flatten().is_some()
    }

    pub fn load_session(&self) -> Result<TokenSet, Error> {
        let stored = self.stored_session()?.ok_or(Error::NotLoggedIn)?;
        if !stored.expired(now_epoch()) && self.validate(&stored)? {
            return Ok(stored);
        }
        match self.refresh(&stored) {
            Ok(fresh) => {
                self.save_session(&fresh)?;
                Ok(fresh)
            }
            Err(_) => Err(Error::NotLoggedIn),
        }
    }

    fn validate(&self, token: &TokenSet) -> Result<bool, Error> {
        let response = self
            .http
            .get(self.endpoints.api_url("/v1/sessions"))
            .header("authorization", Self::auth_header(token))
            .send()
            .map_err(|err| Error::Transport(err.to_string()))?;
        let code = response.status().as_u16();
        if response.status().is_success() {
            return Ok(true);
        }
        if code == 401 || code == 403 {
            return Ok(false);
        }
        Err(Error::Status(code))
    }

    pub fn logout(&self) {
        let _ = madrigal_secrets::delete_secret(SECRET_ACCOUNT);
    }
}

struct Granted {
    token_type: String,
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

impl Granted {
    fn token(&self) -> TokenSet {
        TokenSet {
            token_type: self.token_type.clone(),
            access_token: self.access_token.clone(),
            refresh_token: self.refresh_token.clone(),
            session_id: String::new(),
            country_code: String::new(),
            expires_at: 0,
        }
    }
}
