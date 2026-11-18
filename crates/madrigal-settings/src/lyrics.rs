use serde::{Deserialize, Serialize};

use crate::de::{or_default, vec_filtered};

pub const TRANSLATION_DEFAULT_ENDPOINT: &str =
    "https://translate.googleapis.com/translate_a/single";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum LyricsProvider {
    #[default]
    #[serde(rename = "betterlyrics")]
    BetterLyrics,
    #[serde(rename = "betterlyrics_portato")]
    BetterLyricsPortato,
    #[serde(rename = "youlyplus")]
    YouLyPlus,
    #[serde(rename = "lrclib")]
    Lrclib,
    #[serde(rename = "kugou")]
    Kugou,
    #[serde(rename = "megalobiz")]
    Megalobiz,
    #[serde(rename = "simpmusic")]
    SimpMusic,
    #[serde(rename = "unison")]
    Unison,
    #[serde(rename = "paxsenix_apple_music")]
    PaxsenixAppleMusic,
    #[serde(rename = "paxsenix_spotify")]
    PaxsenixSpotify,
    #[serde(rename = "paxsenix_musixmatch")]
    PaxsenixMusixmatch,
}


pub const DEFAULT_PROVIDER_ORDER: [LyricsProvider; 11] = [
    LyricsProvider::BetterLyrics,
    LyricsProvider::BetterLyricsPortato,
    LyricsProvider::YouLyPlus,
    LyricsProvider::Lrclib,
    LyricsProvider::Kugou,
    LyricsProvider::Megalobiz,
    LyricsProvider::SimpMusic,
    LyricsProvider::Unison,
    LyricsProvider::PaxsenixAppleMusic,
    LyricsProvider::PaxsenixSpotify,
    LyricsProvider::PaxsenixMusixmatch,
];

const LEGACY_ORDER_PREFIX: [LyricsProvider; 3] = [
    LyricsProvider::Lrclib,
    LyricsProvider::Kugou,
    LyricsProvider::BetterLyrics,
];

pub fn sanitize_provider_order(raw: &[LyricsProvider]) -> Vec<LyricsProvider> {
    let mut parsed: Vec<LyricsProvider> = Vec::new();
    for provider in raw {
        if !parsed.contains(provider) {
            parsed.push(*provider);
        }
    }
    if parsed.len() >= LEGACY_ORDER_PREFIX.len()
        && parsed[..LEGACY_ORDER_PREFIX.len()] == LEGACY_ORDER_PREFIX
    {
        parsed.retain(|provider| *provider != LyricsProvider::BetterLyrics);
        parsed.insert(0, LyricsProvider::BetterLyrics);
    }
    for provider in DEFAULT_PROVIDER_ORDER {
        if !parsed.contains(&provider) {
            parsed.push(provider);
        }
    }
    parsed
}

pub fn sanitize_providers_disabled(raw: &[LyricsProvider]) -> Vec<LyricsProvider> {
    let mut disabled = Vec::new();
    for provider in raw {
        if !disabled.contains(provider) {
            disabled.push(*provider);
        }
    }
    disabled
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LyricsPosition {
    Top,
    #[default]
    Center,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LyricsMode {
    #[default]
    V2,
    Enhanced,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lyrics {
    #[serde(deserialize_with = "or_default")]
    pub enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub show_lyrics: bool,
    #[serde(deserialize_with = "or_default")]
    pub font_size: f32,
    #[serde(deserialize_with = "or_default")]
    pub line_spacing: f32,
    #[serde(deserialize_with = "or_default")]
    pub line_blur: bool,
    #[serde(deserialize_with = "or_default")]
    pub show_player_controls: bool,
    #[serde(deserialize_with = "or_default")]
    pub text_position: LyricsPosition,
    #[serde(deserialize_with = "or_default")]
    pub click_to_seek: bool,
    #[serde(deserialize_with = "or_default")]
    pub auto_scroll: bool,
    #[serde(deserialize_with = "or_default")]
    pub prefer_word_timing: bool,
    #[serde(default, deserialize_with = "vec_filtered")]
    pub provider_order: Vec<LyricsProvider>,
    #[serde(default, deserialize_with = "vec_filtered")]
    pub providers_disabled: Vec<LyricsProvider>,
    #[serde(deserialize_with = "or_default")]
    pub preferred_provider: Option<LyricsProvider>,
    #[serde(deserialize_with = "or_default")]
    pub youtube_subtitle_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub paxsenix_api_key: String,
    #[serde(deserialize_with = "or_default")]
    pub translation_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub translation_endpoint: String,
    #[serde(deserialize_with = "or_default")]
    pub translation_api_key: String,
    #[serde(deserialize_with = "or_default")]
    pub translation_target: String,
    #[serde(deserialize_with = "or_default")]
    pub translator_contexts: String,
    #[serde(deserialize_with = "or_default")]
    pub romanize_japanese: bool,
    #[serde(deserialize_with = "or_default")]
    pub romanize_korean: bool,
    #[serde(deserialize_with = "or_default")]
    pub romanize_chinese: bool,
    #[serde(deserialize_with = "or_default")]
    pub romanize_hindi: bool,
    #[serde(deserialize_with = "or_default")]
    pub romanize_other: bool,
    #[serde(deserialize_with = "or_default")]
    pub lyrics_mode: LyricsMode,
    #[serde(deserialize_with = "or_default")]
    pub use_lyrics_v2: bool,
    #[serde(deserialize_with = "or_default")]
    pub v2_bounce_factor: f32,
    #[serde(deserialize_with = "or_default")]
    pub v2_glow_factor: f32,
    #[serde(deserialize_with = "or_default")]
    pub v2_fill_transition_width: f32,
    #[serde(deserialize_with = "or_default")]
    pub v2_lrc_bounce_enabled: bool,
}

impl Default for Lyrics {
    fn default() -> Self {
        Self {
            enabled: true,
            show_lyrics: true,
            font_size: 14.0,
            line_spacing: 1.5,
            line_blur: true,
            show_player_controls: true,
            text_position: LyricsPosition::Center,
            click_to_seek: true,
            auto_scroll: true,
            prefer_word_timing: true,
            provider_order: DEFAULT_PROVIDER_ORDER.to_vec(),
            providers_disabled: Vec::new(),
            preferred_provider: None,
            youtube_subtitle_enabled: false,
            paxsenix_api_key: String::new(),
            translation_enabled: false,
            translation_endpoint: TRANSLATION_DEFAULT_ENDPOINT.to_owned(),
            translation_api_key: String::new(),
            translation_target: "es".to_owned(),
            translator_contexts: String::new(),
            romanize_japanese: true,
            romanize_korean: true,
            romanize_chinese: true,
            romanize_hindi: false,
            romanize_other: false,
            lyrics_mode: LyricsMode::V2,
            use_lyrics_v2: true,
            v2_bounce_factor: 1.0,
            v2_glow_factor: 1.0,
            v2_fill_transition_width: 0.5,
            v2_lrc_bounce_enabled: true,
        }
    }
}

impl Lyrics {
    pub fn sanitize(&mut self) {
        self.font_size = crate::sanitize::clamp_f32(self.font_size, 10.0, 28.0, 14.0);
        self.line_spacing =
            crate::sanitize::clamp_f32(self.line_spacing, 0.5, 3.0, 1.5);
        self.v2_bounce_factor =
            crate::sanitize::clamp_f32(self.v2_bounce_factor, 0.0, 2.0, 1.0);
        self.v2_glow_factor =
            crate::sanitize::clamp_f32(self.v2_glow_factor, 0.0, 2.0, 1.0);
        self.v2_fill_transition_width =
            crate::sanitize::clamp_f32(self.v2_fill_transition_width, 0.0, 1.0, 0.5);
        self.provider_order = sanitize_provider_order(&self.provider_order);
        self.providers_disabled = sanitize_providers_disabled(&self.providers_disabled);
        self.paxsenix_api_key = self.paxsenix_api_key.trim().to_owned();
        self.translation_api_key = self.translation_api_key.trim().to_owned();
        self.translation_endpoint =
            crate::sanitize::non_empty_or(&self.translation_endpoint, TRANSLATION_DEFAULT_ENDPOINT);
        self.translation_target =
            crate::sanitize::non_empty_or(&self.translation_target, "es");
        self.translator_contexts = self.translator_contexts.trim().to_owned();
    }
}
