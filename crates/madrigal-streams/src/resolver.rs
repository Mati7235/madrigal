use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use madrigal_core::{TrackIdentity, TrackSource};

use crate::cache::{self, StreamCache};
use crate::fallback;
use crate::po_token::{self, PoConfig, PoToken};

pub const WATCH_URL: &str = "https://music.youtube.com/watch?v=";
pub const DEFAULT_FORMAT: &str = "bestaudio/best";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioStream {
    pub video_id: String,
    pub url: String,
    pub user_agent: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub ext: Option<String>,
    #[serde(default)]
    pub acodec: Option<String>,
    #[serde(default)]
    pub abr: Option<f64>,
    #[serde(default = "default_client")]
    pub client: String,
    #[serde(default)]
    pub expires_at: Option<f64>,
    #[serde(default = "default_client")]
    pub resolution_path: String,
}

fn default_client() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolveError {
    InvalidId(String),
    Unsupported(String),
    RateLimited { retry_after_secs: u64 },
    Unavailable(String),
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::InvalidId(id) => write!(f, "invalid video id: {id}"),
            ResolveError::Unsupported(source) => write!(f, "unsupported source: {source}"),
            ResolveError::RateLimited { retry_after_secs } => {
                write!(f, "rate limited, retry in {retry_after_secs}s")
            }
            ResolveError::Unavailable(details) => write!(f, "stream unavailable: {details}"),
        }
    }
}

impl std::error::Error for ResolveError {}

#[derive(Debug, Clone)]
pub struct StreamConfig {
    pub ytdlp_bin: PathBuf,
    pub format: String,
    pub timeout_secs: u64,
    pub po: PoConfig,
    pub safety_margin_secs: f64,
    pub fallback_ttl_secs: f64,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            ytdlp_bin: PathBuf::from("yt-dlp"),
            format: DEFAULT_FORMAT.to_string(),
            timeout_secs: 60,
            po: PoConfig::default(),
            safety_margin_secs: 60.0,
            fallback_ttl_secs: 300.0,
        }
    }
}

pub fn validate_id(video_id: &str) -> bool {
    let bytes = video_id.as_bytes();
    if bytes.len() < 6 || bytes.len() > 64 {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_')
}

pub fn backoff_delay_secs(attempts: u32) -> u64 {
    let shift = attempts.min(5);
    30u64.saturating_mul(1u64 << shift).min(600)
}

pub fn build_args(
    video_id: &str,
    format: &str,
    client: Option<&str>,
    token: Option<&PoToken>,
    po_base_url: &str,
) -> Vec<String> {
    let mut args = vec![
        "--dump-single-json".to_string(),
        "--no-playlist".to_string(),
        "--no-warnings".to_string(),
        "--format".to_string(),
        format.to_string(),
    ];
    if let Some(name) = client {
        args.push("--extractor-args".to_string());
        args.push(format!("youtube:player_client={name}"));
    }
    if let Some(token) = token {
        args.extend(po_token::extractor_args(po_base_url, token));
    }
    args.push(format!("{WATCH_URL}{video_id}"));
    args
}

fn expiry_from_url(url: &str) -> Option<f64> {
    let query = url.split_once('?')?.1;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        if key == "expire" {
            return value.parse::<f64>().ok();
        }
    }
    None
}

fn parse_output(video_id: &str, label: &str, stdout: &str) -> Result<AudioStream, String> {
    let info: serde_json::Value =
        serde_json::from_str(stdout).map_err(|err| format!("{label}: invalid JSON: {err}"))?;
    let url = info.get("url").and_then(|v| v.as_str()).unwrap_or("");
    if !url.starts_with("https://") {
        return Err(format!("{label}: no url field"));
    }
    let expires_at = info
        .get("url_expiry")
        .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse::<f64>().ok()))
        .or_else(|| expiry_from_url(url));
    let user_agent = info
        .get("http_headers")
        .and_then(|h| h.get("User-Agent"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Ok(AudioStream {
        video_id: video_id.to_string(),
        url: url.to_string(),
        user_agent,
        title: info
            .get("title")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        duration: info
            .get("duration")
            .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse::<f64>().ok())),
        ext: info.get("ext").and_then(|v| v.as_str()).map(str::to_string),
        acodec: info
            .get("acodec")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        abr: info
            .get("abr")
            .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse::<f64>().ok())),
        client: info
            .get("player_client")
            .or_else(|| info.get("format_note"))
            .and_then(|v| v.as_str())
            .map_or_else(|| label.to_string(), str::to_string),
        expires_at,
        resolution_path: label.to_string(),
    })
}

fn run_child(bin: &PathBuf, args: &[String], timeout: Duration) -> Result<(i32, String, String), String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut command = Command::new(bin);
    command.args(args);
    let child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| format!("spawn failed: {err}"))?;
    std::thread::spawn(move || {
        let output = child.wait_with_output();
        let _ = sender.send(output);
    });
    match receiver.recv_timeout(timeout) {
        Ok(Ok(output)) => Ok((
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )),
        Ok(Err(err)) => Err(format!("wait failed: {err}")),
        Err(_) => Err("yt-dlp timed out".to_string()),
    }
}

struct Inner {
    config: StreamConfig,
    cache: StreamCache,
    rate_limits: Mutex<HashMap<String, u32>>,
}

#[derive(Clone)]
pub struct Orchestrator {
    inner: Arc<Inner>,
}

impl Orchestrator {
    pub fn new(config: StreamConfig) -> io::Result<Self> {
        let cache = StreamCache::new(config.safety_margin_secs, config.fallback_ttl_secs)?;
        Ok(Self::with_cache(config, cache))
    }

    pub fn with_cache_dir(config: StreamConfig, dir: impl Into<PathBuf>) -> Self {
        let cache = StreamCache::with_dir(
            dir,
            config.safety_margin_secs,
            config.fallback_ttl_secs,
        );
        Self::with_cache(config, cache)
    }

    fn with_cache(config: StreamConfig, cache: StreamCache) -> Self {
        Self {
            inner: Arc::new(Inner {
                config,
                cache,
                rate_limits: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn cache(&self) -> &StreamCache {
        &self.inner.cache
    }

    pub fn retry_delay_secs(&self, video_id: &str) -> Option<u64> {
        self.inner
            .rate_limits
            .lock()
            .ok()?
            .get(video_id)
            .copied()
            .map(backoff_delay_secs)
    }

    fn note_rate_limited(&self, video_id: &str) -> u64 {
        let mut limits = self.inner.rate_limits.lock().expect("backoff lock");
        let attempts = limits.get(video_id).copied().unwrap_or(0).saturating_add(1);
        limits.insert(video_id.to_string(), attempts);
        backoff_delay_secs(attempts)
    }

    pub fn resolve(
        &self,
        identity: &TrackIdentity,
        quality: Option<&str>,
    ) -> Result<(AudioStream, String), ResolveError> {
        if identity.source != TrackSource::Youtube {
            return Err(ResolveError::Unsupported(identity.source.to_string()));
        }
        if !validate_id(&identity.id) {
            return Err(ResolveError::InvalidId(identity.id.clone()));
        }
        let video_id = &identity.id;
        let now = cache::now_epoch();
        if let Some(cached) = self.inner.cache.load(video_id, quality, now) {
            return Ok((cached, "cache".to_string()));
        }
        let timeout = Duration::from_secs(self.inner.config.timeout_secs.max(1));
        let mut errors: Vec<String> = Vec::new();
        for client in fallback::player_clients() {
            let label = fallback::client_label(client.as_deref()).to_string();
            let token = po_token::fetch(&self.inner.config.po, video_id, client.as_deref());
            let args = build_args(
                video_id,
                &self.inner.config.format,
                client.as_deref(),
                token.as_ref(),
                &self.inner.config.po.base_url,
            );
            match run_child(&self.inner.config.ytdlp_bin, &args, timeout) {
                Ok((0, stdout, _)) => match parse_output(video_id, &label, &stdout) {
                    Ok(stream) => {
                        let _ = self.inner.cache.store(video_id, quality, &stream, now);
                        return Ok((stream, label));
                    }
                    Err(detail) => errors.push(detail),
                },
                Ok((code, _, stderr)) => {
                    if fallback::is_rate_limited(&stderr) {
                        let delay = self.note_rate_limited(video_id);
                        if let Some(cached) = self.inner.cache.load(video_id, quality, now) {
                            return Ok((cached, "cache".to_string()));
                        }
                        return Err(ResolveError::RateLimited {
                            retry_after_secs: delay,
                        });
                    }
                    if fallback::is_retryable_failure(&stderr, Some(code)) {
                        errors.push(format!("{label}: {code}: {}", stderr.trim()));
                        continue;
                    }
                    errors.push(format!("{label}: {code}: {}", stderr.trim()));
                }
                Err(detail) => {
                    errors.push(format!("{label}: {detail}"));
                }
            }
        }
        if let Some(cached) = self.inner.cache.load(video_id, quality, cache::now_epoch()) {
            return Ok((cached, "cache".to_string()));
        }
        Err(ResolveError::Unavailable(errors.join(" | ")))
    }
}
