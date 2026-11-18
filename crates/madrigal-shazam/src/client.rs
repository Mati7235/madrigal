use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use madrigal_core::{Recognition, TrackIdentity};
use serde_json::Value;

use crate::signature::Signature;

pub const DEFAULT_BASE_URL: &str = "https://amp.shazam.com";
pub const DISCOVERY_PREFIX: &str = "/discovery/v5/en/US/android/-/tag";
pub const QUERY: &str =
    "sync=true&webv3=true&sampling=true&connected=&shazamapiversion=v3&sharehub=true&video=v3";
pub const CONTENT_LANGUAGE: &str = "en_US";
pub const MIN_INTERVAL_MS: u64 = 1000;
pub const MAX_ATTEMPTS: u32 = 3;
pub const RETRY_BASE_DELAY_MS: u64 = 2000;

pub const USER_AGENTS: [&str; 5] = [
    "Dalvik/2.1.0 (Linux; U; Android 5.0.2; VS980 4G Build/LRX22G)",
    "Dalvik/1.6.0 (Linux; U; Android 4.4.2; SM-T210 Build/KOT49H)",
    "Dalvik/2.1.0 (Linux; U; Android 5.1.1; SM-P905V Build/LMY47X)",
    "Dalvik/2.1.0 (Linux; U; Android 6.0.1; SM-G920F Build/MMB29K)",
    "Dalvik/2.1.0 (Linux; U; Android 5.0; SM-G900F Build/LRX21T)",
];

pub const TIMEZONES: [&str; 6] = [
    "Europe/Paris",
    "Europe/London",
    "America/New_York",
    "America/Los_Angeles",
    "Asia/Tokyo",
    "Asia/Dubai",
];

#[derive(Debug, Clone)]
pub enum RecognitionOutcome {
    Matched(Box<ShazamMatch>),
    NoMatch,
}

#[derive(Debug, Clone)]
pub struct ShazamMatch {
    pub recognition: Recognition,
    pub links: RecognizedLinks,
}

#[derive(Debug, Clone, Default)]
pub struct RecognizedLinks {
    pub cover: Option<String>,
    pub cover_hq: Option<String>,
    pub genre: Option<String>,
    pub release_date: Option<String>,
    pub label: Option<String>,
    pub lyrics: Option<Vec<String>>,
    pub shazam_url: Option<String>,
    pub apple_music_url: Option<String>,
    pub spotify_url: Option<String>,
    pub isrc: Option<String>,
    pub youtube_video_id: Option<String>,
}

#[derive(Debug)]
pub enum Error {
    Transport(String),
    Status(u16),
    Malformed(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Transport(detail) => write!(f, "shazam transport failed: {detail}"),
            Error::Status(code) => write!(f, "shazam server returned HTTP {code}"),
            Error::Malformed(detail) => write!(f, "unexpected shazam response: {detail}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub struct ShazamClient {
    base_url: String,
    timeout: Duration,
    min_interval: Duration,
    max_attempts: u32,
    retry_base_delay: Duration,
    last_request: Mutex<Option<Instant>>,
}

impl ShazamClient {
    pub fn new(base_url: &str) -> Self {
        Self::with_limits(
            base_url,
            Duration::from_secs(30),
            Duration::from_millis(MIN_INTERVAL_MS),
            MAX_ATTEMPTS,
            Duration::from_millis(RETRY_BASE_DELAY_MS),
        )
    }

    pub fn with_limits(
        base_url: &str,
        timeout: Duration,
        min_interval: Duration,
        max_attempts: u32,
        retry_base_delay: Duration,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            timeout,
            min_interval,
            max_attempts: max_attempts.max(1),
            retry_base_delay,
            last_request: Mutex::new(None),
        }
    }

    pub fn recognize(&self, signature: &Signature) -> Result<RecognitionOutcome> {
        let seed = seed_entropy();
        let mut rng = seed;
        let stamp = make_stamp(&mut rng);
        let body = request_body(signature, &stamp);
        let url = format!(
            "{}{}/{}/{}?{QUERY}",
            self.base_url, DISCOVERY_PREFIX, stamp.uuid1, stamp.uuid2
        );
        let agent: reqwest::blocking::Client = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|err| Error::Transport(err.to_string()))?;
        let mut attempt = 0;
        loop {
            attempt += 1;
            self.enforce_interval();
            let response = agent
                .post(&url)
                .header("User-Agent", stamp.user_agent.clone())
                .header("Content-Language", CONTENT_LANGUAGE)
                .header("Content-Type", "application/json")
                .body(body.clone())
                .send();
            match response {
                Err(err) => return Err(Error::Transport(err.to_string())),
                Ok(reply) => {
                    let status = reply.status().as_u16();
                    if status == 200 {
                        let text =
                            reply.text().map_err(|err| Error::Transport(err.to_string()))?;
                        return parse_response_body(&text);
                    }
                    if status == 404 {
                        return Ok(RecognitionOutcome::NoMatch);
                    }
                    let retryable = status == 429 || (500..600).contains(&status);
                    if retryable && attempt < self.max_attempts {
                        std::thread::sleep(self.retry_base_delay * 2u32.pow(attempt - 1));
                        continue;
                    }
                    return Err(Error::Status(status));
                }
            }
        }
    }

    fn enforce_interval(&self) {
        let mut guard = self.last_request.lock().expect("interval mutex");
        if let Some(previous) = *guard {
            let elapsed = previous.elapsed();
            if elapsed < self.min_interval {
                std::thread::sleep(self.min_interval - elapsed);
            }
        }
        *guard = Some(Instant::now());
    }
}

pub struct Stamp {
    pub uuid1: String,
    pub uuid2: String,
    pub timestamp_secs: u64,
    pub timezone: String,
    pub user_agent: String,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
}

pub fn request_body(signature: &Signature, stamp: &Stamp) -> Vec<u8> {
    let body = serde_json::json!({
        "geolocation": {
            "altitude": stamp.altitude,
            "latitude": stamp.latitude,
            "longitude": stamp.longitude,
        },
        "signature": {
            "samplems": signature.sample_duration_ms,
            "timestamp": stamp.timestamp_secs,
            "uri": signature.uri,
        },
        "timestamp": stamp.timestamp_secs,
        "timezone": stamp.timezone,
    });
    serde_json::to_vec(&body).expect("json body serializes")
}

pub fn make_stamp(rng: &mut u64) -> Stamp {
    let uuid1 = format_uuid(next_random(rng), next_random(rng)).to_uppercase();
    let uuid2 = format_uuid(next_random(rng), next_random(rng));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Stamp {
        uuid1,
        uuid2,
        timestamp_secs: now,
        timezone: TIMEZONES[(next_random(rng) as usize) % TIMEZONES.len()].to_owned(),
        user_agent: USER_AGENTS[(next_random(rng) as usize) % USER_AGENTS.len()].to_owned(),
        latitude: next_random(rng) as f64 / u64::MAX as f64 * 180.0 - 90.0,
        longitude: next_random(rng) as f64 / u64::MAX as f64 * 360.0 - 180.0,
        altitude: next_random(rng) as f64 / u64::MAX as f64 * 400.0 + 100.0,
    }
}

fn seed_entropy() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    nanos ^ ((std::process::id() as u64) << 32).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

fn next_random(state: &mut u64) -> u64 {
    let mut x = *state | 1;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn format_uuid(high: u64, low: u64) -> String {
    let bytes = [high.to_be_bytes(), low.to_be_bytes()].concat();
    let mut shaped = bytes;
    shaped[6] = shaped[6] & 0x0F | 0x40;
    shaped[8] = shaped[8] & 0x3F | 0x80;
    let hex: String = shaped.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

pub fn parse_response_body(body: &str) -> Result<RecognitionOutcome> {
    let value: Value =
        serde_json::from_str(body).map_err(|err| Error::Malformed(err.to_string()))?;
    let Some(track) = value.get("track").filter(|t| !t.is_null()) else {
        return Ok(RecognitionOutcome::NoMatch);
    };
    let title = text_field(track, "title").unwrap_or_default();
    let artist = text_field(track, "subtitle").unwrap_or_default();
    let url = text_field(track, "url");
    let isrc = text_field(track, "isrc");
    let images = track.get("images");
    let cover = images.and_then(|i| text_field(i, "coverart"));
    let cover_hq = images.and_then(|i| text_field(i, "coverarthq"));
    let genre = track.get("genres").and_then(|g| text_field(g, "primary"));
    let sections = track
        .get("sections")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let song = sections.iter().find(|s| type_is(s, "SONG"));
    let metadata = song
        .and_then(|s| s.get("metadata"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let meta = |name: &str| {
        metadata
            .iter()
            .find(|m| text_field(m, "title") == Some(name.to_owned()))
            .and_then(|m| text_field(m, "text"))
    };
    let lyrics = sections
        .iter()
        .find(|s| type_is(s, "LYRICS"))
        .and_then(|s| s.get("text"))
        .and_then(Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .filter_map(|l| l.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .filter(|lines| !lines.is_empty());
    let hub = track.get("hub");
    let options = hub
        .and_then(|h| h.get("options"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let providers = hub
        .and_then(|h| h.get("providers"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let apple = options
        .iter()
        .find(|o| {
            o.get("providername")
                .and_then(Value::as_str)
                .is_some_and(|p| p.to_lowercase().contains("apple"))
        })
        .and_then(|o| o.get("actions"))
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|a| a.get("uri"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let spotify = providers
        .iter()
        .find(|p| {
            p.get("caption")
                .and_then(Value::as_str)
                .is_some_and(|c| c.to_lowercase().contains("spotify"))
        })
        .and_then(|p| p.get("actions"))
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|a| a.get("uri"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let youtube_uri = options
        .iter()
        .find(|o| {
            o.get("type")
                .and_then(Value::as_str)
                .is_some_and(|t| t.to_lowercase().contains("video"))
        })
        .and_then(|o| o.get("actions"))
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|a| a.get("uri"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let youtube_id = youtube_uri.as_deref().and_then(youtube_video_id);
    Ok(RecognitionOutcome::Matched(Box::new(ShazamMatch {
        recognition: Recognition {
            title,
            artists: if artist.is_empty() {
                Vec::new()
            } else {
                vec![artist]
            },
            album: meta("Album"),
            score: 1.0,
            track: youtube_id.clone().map(TrackIdentity::youtube),
        },
        links: RecognizedLinks {
            cover,
            cover_hq,
            genre,
            release_date: meta("Released"),
            label: meta("Label"),
            lyrics,
            shazam_url: url,
            apple_music_url: apple,
            spotify_url: spotify,
            isrc,
            youtube_video_id: youtube_id,
        },
    })))
}

fn text_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn type_is(value: &Value, expected: &str) -> bool {
    value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|t| t == expected)
}

pub fn youtube_video_id(uri: &str) -> Option<String> {
    if let Some((_, after)) = uri.rsplit_once("v=") {
        if !after.is_empty() {
            return Some(after.to_owned());
        }
    }
    if let Some((_, after)) = uri.rsplit_once('/') {
        if !after.is_empty() && after.len() == 11 {
            return Some(after.to_owned());
        }
    }
    None
}
