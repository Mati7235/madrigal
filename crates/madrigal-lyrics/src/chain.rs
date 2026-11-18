use std::time::{SystemTime, UNIX_EPOCH};

use madrigal_core::{Track, TrackSource};
use madrigal_settings::{Lyrics as SettingsLyrics, LyricsProvider};
use reqwest::Client;

use crate::providers::{
    Provider, Query, betterlyrics::BetterLyricsProvider, kugou::KugouProvider,
    lrclib::LrclibProvider, megalobiz::MegalobizProvider, paxsenix::PaxsenixProvider,
    simpmusic::SimpMusicProvider, tidal::TidalProvider, unison::UnisonProvider,
    youlyplus::YouLyPlusProvider, youtube::YoutubeProvider,
    youtube_subtitle::YoutubeSubtitleProvider,
};
use crate::{LyricLine, Lyrics};

const MISS_MARKER: &str = "v1:miss";
const SYNCED_PREFIX: &str = "v1:lrc\n";
const PLAIN_PREFIX: &str = "v1:txt\n";

#[derive(Debug, Default)]
pub struct EndpointBases {
    pub lrclib: Option<String>,
    pub kugou_songs: Option<String>,
    pub kugou_lyrics: Option<String>,
    pub megalobiz: Option<String>,
    pub simpmusic: Option<String>,
    pub unison: Option<String>,
    pub youlyplus: Option<Vec<String>>,
    pub betterlyrics: Option<String>,
    pub youtube: Option<String>,
    pub paxsenix: Option<String>,
}

#[derive(Debug)]
pub struct ChainConfig {
    pub order: Vec<LyricsProvider>,
    pub disabled: Vec<LyricsProvider>,
    pub paxsenix_api_key: String,
    pub youtube_subtitle_enabled: bool,
    pub bases: EndpointBases,
}

impl From<&SettingsLyrics> for ChainConfig {
    fn from(settings: &SettingsLyrics) -> Self {
        Self {
            order: settings.provider_order.clone(),
            disabled: settings.providers_disabled.clone(),
            paxsenix_api_key: settings.paxsenix_api_key.clone(),
            youtube_subtitle_enabled: settings.youtube_subtitle_enabled,
            bases: EndpointBases::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainProvider {
    BetterLyrics,
    BetterLyricsPortato,
    YouLyPlus,
    Lrclib,
    Kugou,
    Megalobiz,
    SimpMusic,
    Unison,
    PaxsenixAppleMusic,
    PaxsenixSpotify,
    PaxsenixMusixmatch,
    YoutubeSubtitle,
    Youtube,
    Tidal,
}

impl ChainProvider {
    pub fn name(&self) -> &'static str {
        match self {
            ChainProvider::BetterLyrics => "betterlyrics",
            ChainProvider::BetterLyricsPortato => "betterlyrics_portato",
            ChainProvider::YouLyPlus => "youlyplus",
            ChainProvider::Lrclib => "lrclib",
            ChainProvider::Kugou => "kugou",
            ChainProvider::Megalobiz => "megalobiz",
            ChainProvider::SimpMusic => "simpmusic",
            ChainProvider::Unison => "unison",
            ChainProvider::PaxsenixAppleMusic => "paxsenix_apple_music",
            ChainProvider::PaxsenixSpotify => "paxsenix_spotify",
            ChainProvider::PaxsenixMusixmatch => "paxsenix_musixmatch",
            ChainProvider::YoutubeSubtitle => "youtube_subtitle",
            ChainProvider::Youtube => "youtube",
            ChainProvider::Tidal => "tidal",
        }
    }

    fn from_settings(name: LyricsProvider) -> Self {
        match name {
            LyricsProvider::BetterLyrics => ChainProvider::BetterLyrics,
            LyricsProvider::BetterLyricsPortato => ChainProvider::BetterLyricsPortato,
            LyricsProvider::YouLyPlus => ChainProvider::YouLyPlus,
            LyricsProvider::Lrclib => ChainProvider::Lrclib,
            LyricsProvider::Kugou => ChainProvider::Kugou,
            LyricsProvider::Megalobiz => ChainProvider::Megalobiz,
            LyricsProvider::SimpMusic => ChainProvider::SimpMusic,
            LyricsProvider::Unison => ChainProvider::Unison,
            LyricsProvider::PaxsenixAppleMusic => ChainProvider::PaxsenixAppleMusic,
            LyricsProvider::PaxsenixSpotify => ChainProvider::PaxsenixSpotify,
            LyricsProvider::PaxsenixMusixmatch => ChainProvider::PaxsenixMusixmatch,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChainOutcome {
    pub lyrics: Lyrics,
    pub provider: &'static str,
}

pub fn plan(config: &ChainConfig, source: TrackSource) -> Vec<ChainProvider> {
    let mut names: Vec<ChainProvider> = config
        .order
        .iter()
        .filter(|name| !config.disabled.contains(name))
        .map(|name| ChainProvider::from_settings(*name))
        .collect();
    if source == TrackSource::Tidal {
        let mut tidal_first = vec![ChainProvider::Tidal];
        tidal_first.extend(names);
        return tidal_first;
    }
    for tail in [ChainProvider::YoutubeSubtitle, ChainProvider::Youtube] {
        if tail == ChainProvider::YoutubeSubtitle && !config.youtube_subtitle_enabled {
            continue;
        }
        if !names.contains(&tail) {
            names.push(tail);
        }
    }
    names
        .into_iter()
        .filter(|name| provider_available(config, *name))
        .collect()
}

fn provider_available(config: &ChainConfig, name: ChainProvider) -> bool {
    match name {
        ChainProvider::PaxsenixAppleMusic
        | ChainProvider::PaxsenixSpotify
        | ChainProvider::PaxsenixMusixmatch => !config.paxsenix_api_key.trim().is_empty(),
        _ => true,
    }
}

pub fn query_for(track: &Track) -> Query {
    Query {
        title: track.title.clone(),
        artists: track.artists.clone(),
        album: track.album.clone(),
        duration_secs: track.duration_secs,
        video_id: match track.source() {
            TrackSource::Youtube => Some(track.identity.id.clone()),
            _ => None,
        },
    }
}

fn now_epoch() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn encode_cached(lyrics: &Lyrics) -> String {
    if lyrics.synced {
        format!("{SYNCED_PREFIX}{}", crate::to_lrc(lyrics))
    } else {
        format!("{PLAIN_PREFIX}{}", lyrics.plain_text())
    }
}

pub fn decode_cached(raw: &str) -> Option<Lyrics> {
    if raw == MISS_MARKER {
        return None;
    }
    if let Some(body) = raw.strip_prefix(SYNCED_PREFIX) {
        return crate::providers::parse_text(body);
    }
    if let Some(body) = raw.strip_prefix(PLAIN_PREFIX) {
        if body.trim().is_empty() {
            return Some(Lyrics::default());
        }
        return Some(Lyrics {
            lines: body
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| LyricLine {
                    start_ms: 0,
                    end_ms: None,
                    text: line.trim().to_owned(),
                    words: None,
                    markers: Default::default(),
                })
                .collect(),
            synced: false,
        });
    }
    None
}

pub async fn fetch_track(
    client: &Client,
    conn: &rusqlite::Connection,
    track: &Track,
    config: &ChainConfig,
) -> Result<Option<ChainOutcome>, madrigal_db::Error> {
    if let Some(cached) = madrigal_db::fetch_lyrics(conn, &track.identity)? {
        if cached == MISS_MARKER {
            return Ok(None);
        }
        if let Some(lyrics) = decode_cached(&cached) {
            return Ok(Some(ChainOutcome { lyrics, provider: "cache" }));
        }
    }
    let query = query_for(track);
    for name in plan(config, track.source()) {
        match fetch_named(client, config, name, &query).await {
            Ok(Some(lyrics)) => {
                let encoded = encode_cached(&lyrics);
                madrigal_db::cache_lyrics(conn, &track.identity, &encoded, now_epoch())?;
                return Ok(Some(ChainOutcome { lyrics, provider: name.name() }));
            }
            Ok(None) | Err(_) => continue,
        }
    }
    madrigal_db::cache_lyrics(conn, &track.identity, MISS_MARKER, now_epoch())?;
    Ok(None)
}

async fn fetch_named(
    client: &Client,
    config: &ChainConfig,
    name: ChainProvider,
    query: &Query,
) -> crate::providers::ProviderResult {
    let bases = &config.bases;
    let key = config.paxsenix_api_key.clone();
    match name {
        ChainProvider::BetterLyrics => match bases.betterlyrics.clone() {
            Some(base) => BetterLyricsProvider::with_base_url(base, false).fetch(client, query).await,
            None => BetterLyricsProvider::new().fetch(client, query).await,
        },
        ChainProvider::BetterLyricsPortato => match bases.betterlyrics.clone() {
            Some(base) => BetterLyricsProvider::with_base_url(base, true).fetch(client, query).await,
            None => BetterLyricsProvider::portato().fetch(client, query).await,
        },
        ChainProvider::YouLyPlus => match bases.youlyplus.clone() {
            Some(mirrors) => YouLyPlusProvider::with_mirrors(mirrors).fetch(client, query).await,
            None => YouLyPlusProvider::new().fetch(client, query).await,
        },
        ChainProvider::Lrclib => match bases.lrclib.clone() {
            Some(base) => LrclibProvider::with_base_url(base).fetch(client, query).await,
            None => LrclibProvider::new().fetch(client, query).await,
        },
        ChainProvider::Kugou => {
            let provider = match (bases.kugou_songs.clone(), bases.kugou_lyrics.clone()) {
                (Some(songs), Some(lyrics)) => KugouProvider::with_bases(songs, lyrics),
                _ => KugouProvider::new(),
            };
            provider.fetch(client, query).await
        }
        ChainProvider::Megalobiz => match bases.megalobiz.clone() {
            Some(base) => MegalobizProvider::with_base_url(base).fetch(client, query).await,
            None => MegalobizProvider::new().fetch(client, query).await,
        },
        ChainProvider::SimpMusic => match bases.simpmusic.clone() {
            Some(base) => SimpMusicProvider::with_base_url(base).fetch(client, query).await,
            None => SimpMusicProvider::new().fetch(client, query).await,
        },
        ChainProvider::Unison => match bases.unison.clone() {
            Some(base) => UnisonProvider::with_base_url(base).fetch(client, query).await,
            None => UnisonProvider::new().fetch(client, query).await,
        },
        ChainProvider::PaxsenixAppleMusic => match bases.paxsenix.clone() {
            Some(base) => PaxsenixProvider::apple_music(key).with_base_url(base).fetch(client, query).await,
            None => PaxsenixProvider::apple_music(key).fetch(client, query).await,
        },
        ChainProvider::PaxsenixSpotify => match bases.paxsenix.clone() {
            Some(base) => PaxsenixProvider::spotify(key).with_base_url(base).fetch(client, query).await,
            None => PaxsenixProvider::spotify(key).fetch(client, query).await,
        },
        ChainProvider::PaxsenixMusixmatch => match bases.paxsenix.clone() {
            Some(base) => PaxsenixProvider::musixmatch(key).with_base_url(base).fetch(client, query).await,
            None => PaxsenixProvider::musixmatch(key).fetch(client, query).await,
        },
        ChainProvider::Youtube => match bases.youtube.clone() {
            Some(base) => YoutubeProvider::with_base_url(base).fetch(client, query).await,
            None => YoutubeProvider::new().fetch(client, query).await,
        },
        ChainProvider::YoutubeSubtitle => YoutubeSubtitleProvider.fetch(client, query).await,
        ChainProvider::Tidal => TidalProvider.fetch(client, query).await,
    }
}
