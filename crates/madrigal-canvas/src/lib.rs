use std::fmt;
use std::sync::OnceLock;

use madrigal_core::Track;
use regex::Regex;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

pub mod apple;
pub mod betterlyrics;
pub mod cache;
pub mod spotify;
pub mod tidal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CanvasSource {
    BetterLyrics,
    AppleMusic,
    Tidal,
    Spotify,
    #[default]
    All,
}

impl CanvasSource {
    pub fn accepts(self, provider: CanvasSource) -> bool {
        provider != CanvasSource::All && (self == CanvasSource::All || self == provider)
    }

    pub fn from_preference(value: &str) -> CanvasSource {
        if value == "BOTH" {
            return CanvasSource::All;
        }
        match value {
            "BETTER_LYRICS" => CanvasSource::BetterLyrics,
            "APPLE_MUSIC" => CanvasSource::AppleMusic,
            "TIDAL" => CanvasSource::Tidal,
            "SPOTIFY" => CanvasSource::Spotify,
            _ => CanvasSource::All,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            CanvasSource::BetterLyrics => "BETTER_LYRICS",
            CanvasSource::AppleMusic => "APPLE_MUSIC",
            CanvasSource::Tidal => "TIDAL",
            CanvasSource::Spotify => "SPOTIFY",
            CanvasSource::All => "ALL",
        }
    }
}

impl fmt::Display for CanvasSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasEntry {
    pub source: CanvasSource,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album_id: Option<String>,
    #[serde(default)]
    pub album_name: Option<String>,
    #[serde(default)]
    pub static_url: Option<String>,
    #[serde(default)]
    pub animated: Option<String>,
    #[serde(default)]
    pub animated_vertical: Option<String>,
    #[serde(default)]
    pub video_url: Option<String>,
    #[serde(default)]
    pub video_url_vertical: Option<String>,
}

impl CanvasEntry {
    pub fn preferred_animation_url(&self) -> Option<&str> {
        self.animated
            .as_deref()
            .or(self.video_url.as_deref())
            .filter(|url| !url.is_empty())
    }

    pub fn preferred_vertical_animation_url(&self) -> Option<&str> {
        self.animated_vertical
            .as_deref()
            .or(self.video_url_vertical.as_deref())
            .filter(|url| !url.is_empty())
    }

    pub fn has_animation(&self) -> bool {
        self.preferred_animation_url().is_some()
    }

    pub fn matches_identity(&self, song: &str, artist: &str) -> bool {
        let want_song = song_identity(song);
        let want_artist = primary_artist_identity(artist);
        let got_song = self.name.as_deref().map(song_identity).unwrap_or_default();
        let got_artist = self
            .artist
            .as_deref()
            .map(primary_artist_identity)
            .unwrap_or_default();
        !want_song.is_empty()
            && !want_artist.is_empty()
            && got_song == want_song
            && got_artist == want_artist
    }
}

fn song_identity(raw: &str) -> String {
    let no_feat = featuring_qualifier().replace_all(raw, "");
    let no_pres = presentation_qualifier().replace_all(&no_feat, "");
    canvas_identity(&no_pres)
}

fn primary_artist_identity(raw: &str) -> String {
    let first = artist_separator()
        .split(raw)
        .next()
        .unwrap_or_default()
        .to_owned();
    canvas_identity(&first)
}

fn canvas_identity(raw: &str) -> String {
    let nfkd: String = raw.nfkd().collect();
    let stripped: String = nfkd
        .chars()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect();
    identity_separator()
        .replace_all(&stripped.to_lowercase(), " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn featuring_qualifier() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\s*[\[(](?:feat\.?|ft\.?|featuring|with)\s+[^\]\)]*[\]\)]").expect("regex")
    })
}

fn presentation_qualifier() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\s*(?:[-–—:]\s*)?[\[(]?(?:official\s+)?(?:music\s+)?(?:lyric\s+video|video|audio|lyrics?|visualizer|visualiser)[\])]?\s*$")
            .expect("regex")
    })
}

fn artist_separator() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(?:\s*,\s*|\s*&\s*|\s+x\s+|\s+(?:feat\.?|ft\.?|featuring|with)\s+)")
            .expect("regex")
    })
}

fn identity_separator() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[^\p{L}\p{N}]+").expect("regex"))
}

#[derive(Debug)]
pub enum Error {
    Transport(String),
    Status(u16),
    Protocol(String),
    Unsupported(String),
    Io(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Transport(detail) => write!(f, "canvas transport: {detail}"),
            Error::Status(code) => write!(f, "canvas status: {code}"),
            Error::Protocol(detail) => write!(f, "canvas response: {detail}"),
            Error::Unsupported(detail) => write!(f, "canvas unsupported: {detail}"),
            Error::Io(detail) => write!(f, "canvas io: {detail}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<madrigal_tidal::Error> for Error {
    fn from(err: madrigal_tidal::Error) -> Error {
        Error::Transport(err.to_string())
    }
}

pub trait SpotifyCanvasAuth {
    fn canvas_credentials(&self) -> Result<spotify::Credentials, Error>;
}

pub struct NoSpotifyAuth;

impl SpotifyCanvasAuth for NoSpotifyAuth {
    fn canvas_credentials(&self) -> Result<spotify::Credentials, Error> {
        Err(Error::Unsupported(
            "spotify canvas needs the spotifycore session (pending F10-4)".to_owned(),
        ))
    }
}

pub struct Context<'a> {
    pub tidal: Option<(&'a madrigal_tidal::auth::Client, &'a madrigal_tidal::auth::TokenSet)>,
    pub apple: apple::Fetcher,
    pub better_lyrics_base: String,
    pub spotify: spotify::Fetcher<'a>,
    pub http: reqwest::blocking::Client,
}

impl<'a> Context<'a> {
    pub fn offline() -> Result<Context<'a>, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .connect_timeout(std::time::Duration::from_secs(5))
            .no_proxy()
            .build()
            .map_err(|err| Error::Transport(err.to_string()))?;
        Ok(Context {
            tidal: None,
            apple: apple::Fetcher::new(),
            better_lyrics_base: betterlyrics::BASE_URL.to_owned(),
            spotify: spotify::Fetcher::offline(),
            http,
        })
    }
}

pub struct Canvas<'a> {
    ctx: Context<'a>,
    cache: cache::CanvasCache,
    ttl_secs: u64,
}

impl<'a> Canvas<'a> {
    pub fn new(ctx: Context<'a>, cache: cache::CanvasCache) -> Canvas<'a> {
        Canvas {
            ctx,
            cache,
            ttl_secs: 24 * 3600,
        }
    }

    pub fn with_ttl(mut self, ttl_secs: u64) -> Self {
        self.ttl_secs = ttl_secs;
        self
    }

    pub fn cache(&self) -> &cache::CanvasCache {
        &self.cache
    }

    pub fn fetch(&self, track: &Track, source: CanvasSource) -> Result<Option<CanvasEntry>, Error> {
        let order = match source {
            CanvasSource::All => vec![
                CanvasSource::BetterLyrics,
                CanvasSource::AppleMusic,
                CanvasSource::Tidal,
                CanvasSource::Spotify,
            ],
            single => vec![single],
        };
        for provider in order {
            if !source.accepts(provider) {
                continue;
            }
            match self.fetch_one(track, provider) {
                Ok(Some(entry)) => return Ok(Some(entry)),
                Ok(None) => continue,
                Err(Error::Unsupported(_)) if source == CanvasSource::All => continue,
                Err(err) => return Err(err),
            }
        }
        Ok(None)
    }

    fn fetch_one(
        &self,
        track: &Track,
        source: CanvasSource,
    ) -> Result<Option<CanvasEntry>, Error> {
        if let Some(hit) = self.cache.load(&cache::key_for(track, source)) {
            return Ok(Some(hit));
        }
        let entry = match source {
            CanvasSource::BetterLyrics => {
                betterlyrics::fetch(&self.ctx.http, &self.ctx.better_lyrics_base, track)?
            }
            CanvasSource::AppleMusic => self.ctx.apple.fetch(&self.ctx.http, track)?,
            CanvasSource::Tidal => match self.ctx.tidal {
                Some((client, token)) => tidal::fetch(client, token, track)?,
                None => return Err(Error::Unsupported("tidal canvas needs a login".to_owned())),
            },
            CanvasSource::Spotify => {
                let creds = self.ctx.spotify.credentials()?;
                spotify::fetch(&self.ctx.http, &self.ctx.spotify.bases, &creds, track)?
            }
            CanvasSource::All => return Ok(None),
        };
        if let Some(entry) = entry {
            let ttl = match source {
                CanvasSource::BetterLyrics => 60,
                _ => self.ttl_secs,
            };
            let _ = self.cache.store(&cache::key_for(track, source), &entry, ttl);
            return Ok(Some(entry));
        }
        Ok(None)
    }

    pub fn refetch(&self, track: &Track, source: CanvasSource) -> Result<Option<CanvasEntry>, Error> {
        let order = match source {
            CanvasSource::All => vec![
                CanvasSource::BetterLyrics,
                CanvasSource::AppleMusic,
                CanvasSource::Tidal,
                CanvasSource::Spotify,
            ],
            single => vec![single],
        };
        for provider in &order {
            self.cache.invalidate(&cache::key_for(track, *provider));
        }
        self.fetch(track, source)
    }

    pub fn resolve_playback(
        &self,
        track: &Track,
        source: CanvasSource,
    ) -> Result<Option<String>, Error> {
        Ok(self
            .fetch(track, source)?
            .and_then(|entry| entry.preferred_animation_url().map(str::to_owned)))
    }
}
