use serde::{Deserialize, Serialize};

use crate::de::{or_default, vec_filtered};

pub const SPONSORBLOCK_DEFAULT_API_URL: &str = "https://sponsor.ajay.app/api/skipSegments";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SponsorCategory {
    #[default]
    #[serde(rename = "sponsor")]
    Sponsor,
    #[serde(rename = "selfpromo")]
    SelfPromo,
    #[serde(rename = "interaction")]
    Interaction,
    #[serde(rename = "intro")]
    Intro,
    #[serde(rename = "outro")]
    Outro,
    #[serde(rename = "preview")]
    Preview,
    #[serde(rename = "music_offtopic")]
    MusicOfftopic,
    #[serde(rename = "filler")]
    Filler,
    #[serde(rename = "hook")]
    Hook,
}


pub const DEFAULT_SPONSOR_CATEGORIES: [SponsorCategory; 5] = [
    SponsorCategory::Sponsor,
    SponsorCategory::SelfPromo,
    SponsorCategory::Intro,
    SponsorCategory::Outro,
    SponsorCategory::MusicOfftopic,
];

pub fn sanitize_sponsor_categories(raw: &[SponsorCategory]) -> Vec<SponsorCategory> {
    let mut categories = Vec::new();
    for category in raw {
        if !categories.contains(category) {
            categories.push(*category);
        }
    }
    categories
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LastFmProvider {
    #[default]
    LastFm,
    LibreFm,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiscordActivityType {
    Playing,
    Streaming,
    #[default]
    Listening,
    Watching,
    Competing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiscordPresenceStatus {
    #[default]
    Online,
    Idle,
    Dnd,
    Invisible,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Integrations {
    #[serde(deserialize_with = "or_default")]
    pub sponsorblock_enabled: bool,
    #[serde(default, deserialize_with = "vec_filtered")]
    pub sponsorblock_categories: Vec<SponsorCategory>,
    #[serde(deserialize_with = "or_default")]
    pub sponsorblock_api_url: String,
    #[serde(deserialize_with = "or_default")]
    pub maloja_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub maloja_server: String,
    #[serde(deserialize_with = "or_default")]
    pub maloja_api_key: String,
    #[serde(deserialize_with = "or_default")]
    pub listenbrainz_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub listenbrainz_token: String,
    #[serde(deserialize_with = "or_default")]
    pub lastfm_scrobbling_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub lastfm_username: String,
    #[serde(deserialize_with = "or_default")]
    pub lastfm_provider: LastFmProvider,
    #[serde(deserialize_with = "or_default")]
    pub lastfm_custom_endpoint: String,
    #[serde(deserialize_with = "or_default")]
    pub lastfm_use_now_playing: bool,
    #[serde(deserialize_with = "or_default")]
    pub scrobble_delay_percent: f32,
    #[serde(deserialize_with = "or_default")]
    pub scrobble_min_song_duration_secs: i32,
    #[serde(deserialize_with = "or_default")]
    pub scrobble_delay_secs: i32,
    #[serde(deserialize_with = "or_default")]
    pub discord_rpc_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub discord_client_id: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_username: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_display_name: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_avatar_url: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_info_dismissed: bool,
    #[serde(deserialize_with = "or_default")]
    pub discord_activity_name: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_activity_details: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_activity_state: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_button1_label: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_button1_url_source: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_button1_custom_url: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_button1_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub discord_button2_label: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_button2_url_source: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_button2_custom_url: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_button2_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub discord_show_when_paused: bool,
    #[serde(deserialize_with = "or_default")]
    pub discord_activity_type: DiscordActivityType,
    #[serde(deserialize_with = "or_default")]
    pub discord_presence_status: DiscordPresenceStatus,
    #[serde(deserialize_with = "or_default")]
    pub discord_large_image_type: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_large_text_source: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_large_text_custom: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_large_image_custom_url: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_small_image_type: String,
    #[serde(deserialize_with = "or_default")]
    pub discord_small_image_custom_url: String,
    #[serde(deserialize_with = "or_default")]
    pub show_spotify_playlists: bool,
    #[serde(deserialize_with = "or_default")]
    pub spotify_account_name: String,
    #[serde(deserialize_with = "or_default")]
    pub spotify_account_avatar_url: String,
}

impl Default for Integrations {
    fn default() -> Self {
        Self {
            sponsorblock_enabled: true,
            sponsorblock_categories: DEFAULT_SPONSOR_CATEGORIES.to_vec(),
            sponsorblock_api_url: SPONSORBLOCK_DEFAULT_API_URL.to_owned(),
            maloja_enabled: false,
            maloja_server: String::new(),
            maloja_api_key: String::new(),
            listenbrainz_enabled: false,
            listenbrainz_token: String::new(),
            lastfm_scrobbling_enabled: false,
            lastfm_username: String::new(),
            lastfm_provider: LastFmProvider::LastFm,
            lastfm_custom_endpoint: String::new(),
            lastfm_use_now_playing: true,
            scrobble_delay_percent: 50.0,
            scrobble_min_song_duration_secs: 30,
            scrobble_delay_secs: 10,
            discord_rpc_enabled: false,
            discord_client_id: String::new(),
            discord_username: String::new(),
            discord_display_name: String::new(),
            discord_avatar_url: String::new(),
            discord_info_dismissed: false,
            discord_activity_name: String::new(),
            discord_activity_details: String::new(),
            discord_activity_state: String::new(),
            discord_button1_label: String::new(),
            discord_button1_url_source: String::new(),
            discord_button1_custom_url: String::new(),
            discord_button1_enabled: false,
            discord_button2_label: String::new(),
            discord_button2_url_source: String::new(),
            discord_button2_custom_url: String::new(),
            discord_button2_enabled: false,
            discord_show_when_paused: true,
            discord_activity_type: DiscordActivityType::Listening,
            discord_presence_status: DiscordPresenceStatus::Online,
            discord_large_image_type: String::new(),
            discord_large_text_source: String::new(),
            discord_large_text_custom: String::new(),
            discord_large_image_custom_url: String::new(),
            discord_small_image_type: String::new(),
            discord_small_image_custom_url: String::new(),
            show_spotify_playlists: false,
            spotify_account_name: String::new(),
            spotify_account_avatar_url: String::new(),
        }
    }
}

impl Integrations {
    pub fn sanitize(&mut self) {
        self.sponsorblock_categories =
            sanitize_sponsor_categories(&self.sponsorblock_categories);
        self.sponsorblock_api_url =
            crate::sanitize::non_empty_or(&self.sponsorblock_api_url, SPONSORBLOCK_DEFAULT_API_URL);
        self.maloja_server = self.maloja_server.trim().to_owned();
        self.maloja_api_key = self.maloja_api_key.trim().to_owned();
        self.listenbrainz_token = self.listenbrainz_token.trim().to_owned();
        self.lastfm_username = self.lastfm_username.trim().to_owned();
        self.lastfm_custom_endpoint = self.lastfm_custom_endpoint.trim().to_owned();
        self.scrobble_delay_percent =
            crate::sanitize::clamp_f32(self.scrobble_delay_percent, 0.0, 100.0, 50.0);
        self.scrobble_min_song_duration_secs =
            crate::sanitize::clamp_i32(self.scrobble_min_song_duration_secs, 0, 3600);
        self.scrobble_delay_secs =
            crate::sanitize::clamp_i32(self.scrobble_delay_secs, 0, 3600);
        self.spotify_account_name = self.spotify_account_name.trim().to_owned();
        self.spotify_account_avatar_url =
            self.spotify_account_avatar_url.trim().to_owned();
        self.discord_client_id = self.discord_client_id.trim().to_owned();
        self.discord_large_image_custom_url =
            self.discord_large_image_custom_url.trim().to_owned();
        self.discord_small_image_custom_url =
            self.discord_small_image_custom_url.trim().to_owned();
        self.discord_button1_custom_url =
            self.discord_button1_custom_url.trim().to_owned();
        self.discord_button2_custom_url =
            self.discord_button2_custom_url.trim().to_owned();
    }
}
