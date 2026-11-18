use std::fmt;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::{Category, HASH_PREFIX_LENGTH, Segment, parse};

pub const USER_AGENT: &str = "Madrigal/0.1.0";

#[derive(Debug)]
pub enum Error {
    Transport(String),
    Status(u16),
    Body(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Transport(detail) => write!(f, "sponsorblock transport: {detail}"),
            Error::Status(code) => write!(f, "sponsorblock status: {code}"),
            Error::Body(detail) => write!(f, "sponsorblock body: {detail}"),
        }
    }
}

impl std::error::Error for Error {}

pub fn hash_prefix(video_id: &str) -> String {
    let digest = Sha256::digest(video_id.as_bytes());
    hex::encode(digest)[..HASH_PREFIX_LENGTH].to_owned()
}

#[derive(Debug, Clone)]
pub struct Client {
    base_url: String,
    categories: Vec<Category>,
    enabled: bool,
    timeout: Duration,
}

impl Client {
    pub fn new(base_url: impl Into<String>) -> Client {
        Client {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            categories: Category::DEFAULT.to_vec(),
            enabled: true,
            timeout: Duration::from_secs(20),
        }
    }

    pub fn from_integrations(
        integrations: &madrigal_settings::Integrations,
    ) -> Client {
        let mut categories: Vec<Category> = Vec::new();
        for stored in &integrations.sponsorblock_categories {
            let category = Category::from(*stored);
            if !categories.contains(&category) {
                categories.push(category);
            }
        }
        let url = integrations.sponsorblock_api_url.trim();
        let base = if url.is_empty() {
            madrigal_settings::SPONSORBLOCK_DEFAULT_API_URL.to_owned()
        } else {
            url.to_owned()
        };
        Client {
            base_url: base.trim_end_matches('/').to_owned(),
            categories,
            enabled: integrations.sponsorblock_enabled,
            timeout: Duration::from_secs(20),
        }
    }

    pub fn categories(&self) -> &[Category] {
        &self.categories
    }

    pub fn fetch(&self, video_id: &str) -> Result<Vec<Segment>, Error> {
        if !self.enabled || video_id.is_empty() || self.categories.is_empty() {
            return Ok(Vec::new());
        }
        let names: Vec<&str> = self
            .categories
            .iter()
            .map(|category| category.as_str())
            .collect();
        let url = format!("{}/{}", self.base_url, hash_prefix(video_id));
        let http = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .no_proxy()
            .user_agent(USER_AGENT)
            .build()
            .map_err(|err| Error::Transport(err.to_string()))?;
        let response = http
            .get(&url)
            .query(&[
                ("prefix", HASH_PREFIX_LENGTH.to_string()),
                ("categories", serde_json::to_string(&names).unwrap_or_default()),
            ])
            .send()
            .map_err(|err| Error::Transport(err.to_string()))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            return Err(Error::Status(response.status().as_u16()));
        }
        let payload: serde_json::Value = response
            .json()
            .map_err(|err| Error::Body(err.to_string()))?;
        Ok(parse(&payload, video_id))
    }
}
