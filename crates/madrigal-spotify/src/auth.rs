use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::totp;
use crate::Error;

pub const SECRET_ACCOUNT: &str = "spotify";
pub const EXPIRY_GRACE_SECS: u64 = 60;
pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookies {
    pub sp_dc: String,
    pub sp_key: String,
}

impl Cookies {
    pub fn new(sp_dc: impl Into<String>, sp_key: impl Into<String>) -> Cookies {
        Cookies {
            sp_dc: sp_dc.into(),
            sp_key: sp_key.into(),
        }
    }

    pub fn header(&self) -> String {
        if self.sp_key.is_empty() {
            format!("sp_dc={}", self.sp_dc)
        } else {
            format!("sp_dc={}; sp_key={}", self.sp_dc, self.sp_key)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub access_token: String,
    pub expires_at_ms: i64,
}

impl Token {
    pub fn expired(&self, now_ms: i64) -> bool {
        now_ms + (EXPIRY_GRACE_SECS as i64) * 1000 >= self.expires_at_ms
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|span| span.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone)]
pub struct Endpoints {
    pub nuance_url: String,
    pub server_time_url: String,
    pub token_url: String,
}

impl Default for Endpoints {
    fn default() -> Endpoints {
        Endpoints {
            nuance_url: "https://gist.githubusercontent.com/sonic-liberation/22ed9c6ba463899e933427f7de1f0eef/raw/nuances.json".to_owned(),
            server_time_url: "https://open.spotify.com/api/server-time".to_owned(),
            token_url: "https://open.spotify.com/api/token".to_owned(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Nuance {
    #[serde(default)]
    s: String,
    #[serde(default)]
    v: i64,
}

#[derive(Debug, Deserialize)]
struct ServerTime {
    #[serde(default, rename = "serverTime")]
    server_time: i64,
}

#[derive(Debug, Deserialize)]
struct InternalToken {
    #[serde(default, rename = "accessToken")]
    access_token: String,
    #[serde(default, rename = "accessTokenExpirationTimestampMs")]
    expires_at_ms: i64,
    #[serde(default, rename = "isAnonymous")]
    is_anonymous: bool,
}

pub struct Client {
    http: reqwest::blocking::Client,
    endpoints: Endpoints,
}

impl Client {
    pub fn new() -> Result<Client, Error> {
        Client::with_endpoints(Endpoints::default())
    }

    pub fn with_endpoints(endpoints: Endpoints) -> Result<Client, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .no_proxy()
            .build()
            .map_err(|err| Error::Transport(err.to_string()))?;
        Ok(Client { http, endpoints })
    }

    pub fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    fn get(&self, url: &str, cookie: Option<&str>) -> Result<reqwest::blocking::Response, Error> {
        let mut request = self
            .http
            .get(url)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Language", "en");
        if let Some(cookie) = cookie {
            request = request.header("Cookie", cookie);
        }
        request
            .send()
            .map_err(|err| Error::Transport(err.to_string()))
    }

    fn fetch_nuance(&self) -> Result<Nuance, Error> {
        let response = self.get(&self.endpoints.nuance_url.clone(), None)?;
        if !response.status().is_success() {
            return Err(Error::Status(response.status().as_u16()));
        }
        let nuances: Vec<Nuance> = response
            .json()
            .map_err(|err| Error::Protocol(err.to_string()))?;
        nuances
            .into_iter()
            .filter(|entry| entry.v > 0 && totp::valid_secret(&entry.s))
            .max_by_key(|entry| entry.v)
            .ok_or_else(|| Error::Auth("totp secrets unavailable".to_owned()))
    }

    fn fetch_server_time(&self) -> Result<i64, Error> {
        let response = self.get(&self.endpoints.server_time_url.clone(), None)?;
        if !response.status().is_success() {
            return Err(Error::Status(response.status().as_u16()));
        }
        let body: ServerTime = response
            .json()
            .map_err(|err| Error::Protocol(err.to_string()))?;
        if body.server_time <= 0 {
            return Err(Error::Protocol("server-time lacks serverTime".to_owned()));
        }
        Ok(body.server_time)
    }

    pub fn exchange(&self, cookies: &Cookies) -> Result<Token, Error> {
        if cookies.sp_dc.trim().is_empty() {
            return Err(Error::Auth("sp_dc cookie required".to_owned()));
        }
        let nuance = self.fetch_nuance()?;
        let server_time = self.fetch_server_time().unwrap_or_else(|_| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|span| span.as_secs() as i64)
                .unwrap_or(0)
        });
        let code = totp::generate(&nuance.s, server_time);
        let url = format!(
            "{}?reason=transport&productType=web-player&totp={code}&totpServer={code}&totpVer={}",
            self.endpoints.token_url, nuance.v
        );
        let response = self.get(&url, Some(&cookies.header()))?;
        if response.status().as_u16() == 401 {
            return Err(Error::Auth("sp_dc cookie is invalid or expired".to_owned()));
        }
        if !response.status().is_success() {
            return Err(Error::Status(response.status().as_u16()));
        }
        let body: InternalToken = response
            .json()
            .map_err(|err| Error::Protocol(err.to_string()))?;
        if body.is_anonymous || body.access_token.is_empty() {
            return Err(Error::Auth("sp_dc cookie is invalid or expired".to_owned()));
        }
        Ok(Token {
            access_token: body.access_token,
            expires_at_ms: body.expires_at_ms,
        })
    }

    pub fn save_session(&self, cookies: &Cookies, token: &Token) -> Result<(), Error> {
        if cookies.sp_dc.is_empty() || token.access_token.is_empty() {
            return Err(Error::Auth("refusing to store incomplete session".to_owned()));
        }
        let raw = serde_json::json!({
            "sp_dc": cookies.sp_dc,
            "sp_key": cookies.sp_key,
            "access_token": token.access_token,
            "expires_at_ms": token.expires_at_ms,
        })
        .to_string();
        madrigal_secrets::set_secret(SECRET_ACCOUNT, &raw)
            .map_err(|err| Error::Secrets(err.to_string()))
    }

    pub fn stored_session(&self) -> Result<Option<(Cookies, Token)>, Error> {
        let raw = madrigal_secrets::get_secret(SECRET_ACCOUNT)
            .map_err(|err| Error::Secrets(err.to_string()))?;
        let raw = match raw {
            Some(raw) => raw,
            None => return Ok(None),
        };
        let value: serde_json::Value = serde_json::from_str(&raw).map_err(|_| Error::Auth(
            "stored spotify session is corrupt".to_owned(),
        ))?;
        let text = |key: &str| {
            value
                .get(key)
                .and_then(|entry| entry.as_str())
                .unwrap_or_default()
                .to_owned()
        };
        let cookies = Cookies::new(text("sp_dc"), text("sp_key"));
        let token = Token {
            access_token: text("access_token"),
            expires_at_ms: value
                .get("expires_at_ms")
                .and_then(|entry| entry.as_i64())
                .unwrap_or(0),
        };
        if cookies.sp_dc.is_empty() || token.access_token.is_empty() {
            return Ok(None);
        }
        Ok(Some((cookies, token)))
    }

    pub fn is_logged_in(&self) -> bool {
        self.stored_session().ok().flatten().is_some()
    }

    pub fn load_session(&self) -> Result<(Cookies, Token), Error> {
        let (cookies, token) = self.stored_session()?.ok_or(Error::NotLoggedIn)?;
        if !token.expired(now_ms()) {
            return Ok((cookies, token));
        }
        match self.exchange(&cookies) {
            Ok(fresh) => {
                self.save_session(&cookies, &fresh)?;
                Ok((cookies, fresh))
            }
            Err(_) => Err(Error::NotLoggedIn),
        }
    }

    pub fn refresh(&self, rejected: &Token) -> Result<Token, Error> {
        let (cookies, _) = self.stored_session()?.ok_or(Error::NotLoggedIn)?;
        let fresh = self.exchange(&cookies).map_err(|_| Error::NotLoggedIn)?;
        if fresh.access_token == rejected.access_token {
            return Err(Error::NotLoggedIn);
        }
        self.save_session(&cookies, &fresh)?;
        Ok(fresh)
    }

    pub fn connect(&self, cookies: &Cookies) -> Result<Token, Error> {
        let token = self.exchange(cookies)?;
        self.save_session(cookies, &token)?;
        Ok(token)
    }

    pub fn logout(&self) {
        let _ = madrigal_secrets::delete_secret(SECRET_ACCOUNT);
    }
}
