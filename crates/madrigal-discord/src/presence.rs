use madrigal_core::{Track, TrackSource};
use madrigal_settings::{DiscordActivityType, Integrations};
use serde_json::{Map, Value};

use crate::APP_NAME;

pub const PAUSED_TEXT: &str = "Paused";
const MAX_TEXT: usize = 128;
const MAX_URL: usize = 256;
const MAX_LABEL: usize = 32;

fn or_default(value: String, default: &str) -> String {
    if value.trim().is_empty() {
        default.to_owned()
    } else {
        value
    }
}

fn source_value(pref: &str, track: &Track, default: &str) -> String {
    match pref.trim().to_ascii_uppercase().as_str() {
        "ARTIST" => or_default(track.artists.join(", "), default),
        "ALBUM" => or_default(track.album.clone().unwrap_or_default(), default),
        "SONG" => or_default(track.title.clone(), default),
        "APP" => APP_NAME.to_owned(),
        _ => default.to_owned(),
    }
}

fn discord_text(raw: &str, max: usize, fallback: &str) -> Option<String> {
    let trimmed = raw.trim();
    let value = if trimmed.is_empty() {
        fallback.trim()
    } else {
        trimmed
    };
    if value.chars().count() < 2 {
        return None;
    }
    Some(value.chars().take(max).collect())
}

fn normalize_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let full = if lower.starts_with("http://") || lower.starts_with("https://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    };
    Some(full.chars().take(MAX_URL).collect())
}

pub fn song_url(track: &Track) -> Option<String> {
    if track.source() != TrackSource::Youtube || track.identity.id.trim().is_empty() {
        return None;
    }
    normalize_url(&format!(
        "https://music.youtube.com/watch?v={}",
        track.identity.id.trim()
    ))
}

fn activity_type_value(kind: DiscordActivityType) -> u8 {
    match kind {
        DiscordActivityType::Playing => 0,
        DiscordActivityType::Streaming => 1,
        DiscordActivityType::Listening => 2,
        DiscordActivityType::Watching => 3,
        DiscordActivityType::Competing => 5,
    }
}

fn large_image(config: &Integrations, track: &Track) -> Option<String> {
    match config.discord_large_image_type.trim().to_ascii_lowercase().as_str() {
        "thumbnail" | "artist" => track.thumbnail.clone(),
        "custom" => normalize_url(&config.discord_large_image_custom_url),
        _ => None,
    }
}

fn large_text(config: &Integrations, track: &Track) -> Option<String> {
    let album = track.album.clone().unwrap_or_default();
    match config
        .discord_large_text_source
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "song" => discord_text(&track.title, MAX_TEXT, &track.title),
        "artist" => {
            let artists = track.artists.join(", ");
            if artists.trim().is_empty() {
                None
            } else {
                discord_text(&artists, MAX_TEXT, &artists)
            }
        }
        "album" => discord_text(&album, MAX_TEXT, &track.title),
        "app" => discord_text(APP_NAME, MAX_TEXT, APP_NAME),
        "custom" => {
            let custom = config.discord_large_text_custom.trim();
            if custom.is_empty() {
                None
            } else {
                discord_text(custom, MAX_TEXT, custom)
            }
        }
        _ => None,
    }
}

fn small_image(config: &Integrations, track: &Track) -> Option<String> {
    match config
        .discord_small_image_type
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "thumbnail" | "artist" => track.thumbnail.clone(),
        "custom" => normalize_url(&config.discord_small_image_custom_url),
        _ => None,
    }
}

fn small_text(config: &Integrations, track: &Track, paused: bool) -> Option<String> {
    if paused {
        return discord_text(PAUSED_TEXT, MAX_TEXT, PAUSED_TEXT);
    }
    let image_type = config
        .discord_small_image_type
        .trim()
        .to_ascii_lowercase();
    let base = match image_type.as_str() {
        "song" => track.title.clone(),
        "artist" => track.artists.join(", "),
        "thumbnail" | "album" => track.album.clone().unwrap_or_default(),
        "appicon" | "app" => APP_NAME.to_owned(),
        _ => return None,
    };
    if base.trim().is_empty() {
        return None;
    }
    discord_text(&format!("{base} on {APP_NAME}"), MAX_TEXT, &base)
}

fn button(
    label: &str,
    enabled: bool,
    source: &str,
    custom: &str,
    track: &Track,
) -> Option<Value> {
    if !enabled || label.trim().is_empty() {
        return None;
    }
    let url = match source.trim().to_ascii_lowercase().as_str() {
        "songurl" => song_url(track),
        "custom" => normalize_url(custom),
        _ => None,
    }?;
    let label: String = label.trim().chars().take(MAX_LABEL).collect();
    let label = or_default(label, APP_NAME);
    Some(serde_json::json!({"label": label, "url": url}))
}

fn buttons(config: &Integrations, track: &Track) -> Vec<Value> {
    let mut resolved = Vec::new();
    if let Some(first) = button(
        &config.discord_button1_label,
        config.discord_button1_enabled,
        &config.discord_button1_url_source,
        &config.discord_button1_custom_url,
        track,
    ) {
        resolved.push(first);
    }
    if let Some(second) = button(
        &config.discord_button2_label,
        config.discord_button2_enabled,
        &config.discord_button2_url_source,
        &config.discord_button2_custom_url,
        track,
    ) {
        resolved.push(second);
    }
    resolved.truncate(2);
    resolved
}

fn timestamps(
    track: &Track,
    position_ms: u64,
    paused: bool,
    now_ms: u64,
) -> (Option<u64>, Option<u64>) {
    let start_ms = now_ms.saturating_sub(position_ms);
    if paused {
        return (Some(start_ms / 1000), None);
    }
    match track.duration_secs {
        Some(duration) if duration > 0 => (
            Some(start_ms / 1000),
            Some((start_ms + u64::from(duration) * 1000) / 1000),
        ),
        _ => (None, None),
    }
}

pub fn build_activity(
    config: &Integrations,
    track: &Track,
    position_ms: u64,
    paused: bool,
    now_ms: u64,
) -> Value {
    let name_default = APP_NAME.to_owned();
    let title_default = or_default(track.title.clone(), APP_NAME);
    let artist_default = or_default(track.artists.join(", "), APP_NAME);
    let name = source_value(&config.discord_activity_name, track, &name_default);
    let details = source_value(&config.discord_activity_details, track, &title_default);
    let state = source_value(&config.discord_activity_state, track, &artist_default);
    let (start, end) = timestamps(track, position_ms, paused, now_ms);
    let mut activity = Map::new();
    activity.insert(
        "name".to_owned(),
        Value::String(discord_text(&name, MAX_TEXT, APP_NAME).unwrap_or_else(|| APP_NAME.to_owned())),
    );
    activity.insert(
        "type".to_owned(),
        Value::Number(activity_type_value(config.discord_activity_type).into()),
    );
    if let Some(details) = discord_text(&details, MAX_TEXT, &title_default) {
        activity.insert("details".to_owned(), Value::String(details));
    }
    if let Some(state) = discord_text(&state, MAX_TEXT, APP_NAME) {
        activity.insert("state".to_owned(), Value::String(state));
    }
    let mut stamps = Map::new();
    if let Some(start) = start {
        stamps.insert("start".to_owned(), Value::Number(start.into()));
    }
    if let Some(end) = end {
        stamps.insert("end".to_owned(), Value::Number(end.into()));
    }
    if !stamps.is_empty() {
        activity.insert("timestamps".to_owned(), Value::Object(stamps));
    }
    let mut assets = Map::new();
    if let Some(large) = large_image(config, track) {
        assets.insert("large_image".to_owned(), Value::String(large));
    }
    if let Some(text) = large_text(config, track) {
        assets.insert("large_text".to_owned(), Value::String(text));
    }
    if let Some(small) = small_image(config, track) {
        assets.insert("small_image".to_owned(), Value::String(small));
    }
    if let Some(text) = small_text(config, track, paused) {
        assets.insert("small_text".to_owned(), Value::String(text));
    }
    if !assets.is_empty() {
        activity.insert("assets".to_owned(), Value::Object(assets));
    }
    let button_list = buttons(config, track);
    if !button_list.is_empty() {
        activity.insert("buttons".to_owned(), Value::Array(button_list));
    }
    Value::Object(activity)
}

pub struct Preview<'a> {
    config: &'a Integrations,
}

impl<'a> Preview<'a> {
    pub fn new(config: &'a Integrations) -> Self {
        Self { config }
    }

    pub fn render(
        &self,
        track: &Track,
        position_ms: u64,
        paused: bool,
        now_ms: u64,
    ) -> Value {
        build_activity(self.config, track, position_ms, paused, now_ms)
    }
}
