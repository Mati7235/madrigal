use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

use crate::cookies::{
    has_login_cookie, has_primary_sapisid, parse_cookie_header, to_netscape, CookieRecord,
};

pub const SECRET_BACKEND: &str = "secret";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub cookie: String,
    pub visitor_data: String,
    pub data_sync_id: String,
    pub account_name: String,
    pub account_email: String,
    pub channel_handle: String,
    pub channel_id: String,
    pub avatar_url: String,
    pub authuser: String,
    pub secret_backend: String,
    pub source_browser: String,
    pub source_profile: String,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            cookie: String::new(),
            visitor_data: String::new(),
            data_sync_id: String::new(),
            account_name: String::new(),
            account_email: String::new(),
            channel_handle: String::new(),
            channel_id: String::new(),
            avatar_url: String::new(),
            authuser: "0".to_owned(),
            secret_backend: String::new(),
            source_browser: String::new(),
            source_profile: String::new(),
        }
    }
}

impl Session {
    pub fn logged_in(&self) -> bool {
        parse_cookie_header(&self.cookie)
            .map(|pairs| has_login_cookie(&pairs))
            .unwrap_or(false)
    }

    pub fn catalog_ready(&self) -> bool {
        parse_cookie_header(&self.cookie)
            .map(|pairs| has_primary_sapisid(&pairs))
            .unwrap_or(false)
    }

    pub fn cookie_count(&self) -> usize {
        parse_cookie_header(&self.cookie).map(|pairs| pairs.len()).unwrap_or(0)
    }

    pub fn brand_channel(&self) -> Option<madrigal_core::Artist> {
        if self.data_sync_id.is_empty() {
            return None;
        }
        Some(madrigal_core::Artist {
            id: madrigal_core::ArtistId::new(self.data_sync_id.clone()),
            name: self.account_name.clone(),
            subscribers: None,
            thumbnail: None,
            description: None,
        })
    }

    pub fn secret_key_for(path: &Path) -> String {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        stem
            .strip_prefix("session_")
            .unwrap_or(stem)
            .to_owned()
    }

    pub fn load(path: &Path) -> Self {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(_) => return Self::default(),
        };
        let mut session: Self = serde_json::from_str(&text).unwrap_or_default();
        session.recover_cookie(path);
        session
    }

    fn recover_cookie(&mut self, path: &Path) {
        let key = Self::secret_key_for(path);
        if self.secret_backend == SECRET_BACKEND {
            match madrigal_secrets::get_secret(&key) {
                Ok(Some(secret)) if !secret.is_empty() => self.cookie = secret,
                _ => self.cookie = String::new(),
            }
            return;
        }
        if self.cookie.is_empty() {
            return;
        }
        if madrigal_secrets::set_secret(&key, &self.cookie).is_err() {
            return;
        }
        self.secret_backend = SECRET_BACKEND.to_owned();
        let _ = self.save(path);
    }

    pub fn save(&self, path: &Path) -> io::Result<PathBuf> {
        let key = Self::secret_key_for(path);
        let mut stored = self.clone();
        stored.secret_backend = String::new();
        if !self.cookie.is_empty()
            && madrigal_secrets::set_secret(&key, &self.cookie).is_ok()
        {
            stored.cookie = String::new();
            stored.secret_backend = SECRET_BACKEND.to_owned();
        }
        let mut text = serde_json::to_string_pretty(&stored)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        text.push('\n');
        madrigal_secrets::write_private(path, &text)
    }

    pub fn clear(&mut self, account_path: &Path, cookies_path: &Path) {
        *self = Self::default();
        let _ = madrigal_secrets::delete_secret(&Self::secret_key_for(account_path));
        madrigal_secrets::remove_private(account_path);
        madrigal_secrets::remove_private(cookies_path);
    }
}

pub fn save_cookies(records: &[CookieRecord], path: &Path) -> io::Result<PathBuf> {
    let text = to_netscape(records)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
    madrigal_secrets::write_private(path, &text)
}
