use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::{ComponentHandle, SharedString, VecModel};

use crate::{MainShell, SettingsRow as UiSettingsRow};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsGroup {
    Appearance,
    Content,
    Player,
    Lyrics,
    Internet,
    Storage,
    Privacy,
    Account,
    Integrations,
    Ai,
    Updates,
}

pub const GROUP_COUNT: usize = 11;
pub const DIAG_INDEX: i32 = 11;

pub fn all_groups() -> [SettingsGroup; GROUP_COUNT] {
    use SettingsGroup::*;
    [
        Appearance,
        Content,
        Player,
        Lyrics,
        Internet,
        Storage,
        Privacy,
        Account,
        Integrations,
        Ai,
        Updates,
    ]
}

pub fn group_index(group: SettingsGroup) -> i32 {
    match group {
        SettingsGroup::Appearance => 0,
        SettingsGroup::Content => 1,
        SettingsGroup::Player => 2,
        SettingsGroup::Lyrics => 3,
        SettingsGroup::Internet => 4,
        SettingsGroup::Storage => 5,
        SettingsGroup::Privacy => 6,
        SettingsGroup::Account => 7,
        SettingsGroup::Integrations => 8,
        SettingsGroup::Ai => 9,
        SettingsGroup::Updates => 10,
    }
}

pub fn group_from_index(index: i32) -> SettingsGroup {
    match index {
        1 => SettingsGroup::Content,
        2 => SettingsGroup::Player,
        3 => SettingsGroup::Lyrics,
        4 => SettingsGroup::Internet,
        5 => SettingsGroup::Storage,
        6 => SettingsGroup::Privacy,
        7 => SettingsGroup::Account,
        8 => SettingsGroup::Integrations,
        9 => SettingsGroup::Ai,
        10 => SettingsGroup::Updates,
        _ => SettingsGroup::Appearance,
    }
}

pub fn group_key(group: SettingsGroup) -> &'static str {
    match group {
        SettingsGroup::Appearance => "appearance",
        SettingsGroup::Content => "content",
        SettingsGroup::Player => "player",
        SettingsGroup::Lyrics => "lyrics",
        SettingsGroup::Internet => "internet",
        SettingsGroup::Storage => "storage",
        SettingsGroup::Privacy => "privacy",
        SettingsGroup::Account => "account",
        SettingsGroup::Integrations => "integrations",
        SettingsGroup::Ai => "ai",
        SettingsGroup::Updates => "updates",
    }
}

pub fn group_title_key(group: SettingsGroup) -> &'static str {
    match group {
        SettingsGroup::Appearance => "settings.group.appearance",
        SettingsGroup::Content => "settings.group.content",
        SettingsGroup::Player => "settings.group.player",
        SettingsGroup::Lyrics => "settings.group.lyrics",
        SettingsGroup::Internet => "settings.group.internet",
        SettingsGroup::Storage => "settings.group.storage",
        SettingsGroup::Privacy => "settings.group.privacy",
        SettingsGroup::Account => "settings.group.account",
        SettingsGroup::Integrations => "settings.group.integrations",
        SettingsGroup::Ai => "settings.group.ai",
        SettingsGroup::Updates => "settings.group.updates",
    }
}

pub fn group_titles() -> Vec<String> {
    all_groups()
        .iter()
        .map(|group| group_title_key(*group).to_owned())
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingKind {
    Switch,
    Text,
    Slider,
    Choice,
    ApiKey,
    Providers,
}

pub fn kind_tag(kind: SettingKind) -> &'static str {
    match kind {
        SettingKind::Switch => "switch",
        SettingKind::Text => "text",
        SettingKind::Slider => "slider",
        SettingKind::Choice => "choice",
        SettingKind::ApiKey => "apikey",
        SettingKind::Providers => "providers",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Plain,
    Ratio,
    Semitones,
    Decibels,
    Db,
    Percent,
    Seconds,
    Days,
    Pixels,
    Megabytes,
}

#[derive(Clone, Debug)]
pub struct SettingRow {
    pub key: &'static str,
    pub label_key: &'static str,
    pub kind: SettingKind,
    pub bool_val: bool,
    pub text_val: String,
    pub num_val: f64,
    pub num_min: f64,
    pub num_max: f64,
    pub unit: Unit,
    pub options: Vec<String>,
    pub option_labels: Vec<String>,
    pub flags: Vec<String>,
    pub test_id: &'static str,
    pub section: String,
    pub advanced: bool,
    pub hidden: bool,
    pub hint: String,
    pub placeholder: String,
}

fn row(
    key: &'static str,
    label: &'static str,
    kind: SettingKind,
    test_id: &'static str,
) -> SettingRow {
    SettingRow {
        key,
        label_key: label,
        kind,
        bool_val: false,
        text_val: String::new(),
        num_val: 0.0,
        num_min: 0.0,
        num_max: 1.0,
        unit: Unit::Plain,
        options: Vec::new(),
        option_labels: Vec::new(),
        flags: Vec::new(),
        test_id,
        section: String::new(),
        advanced: false,
        hidden: false,
        hint: String::new(),
        placeholder: String::new(),
    }
}

fn option_label_key(field: &str, option: &str) -> String {
    if option.is_empty() {
        format!("settings.opt.{field}.auto")
    } else {
        format!("settings.opt.{field}.{}", option.to_lowercase())
    }
}

pub fn slider_text(unit: Unit, value: f64, lang: madrigal_i18n::Lang) -> String {
    let es = matches!(lang, madrigal_i18n::Lang::Es);
    let word = |key: &str| madrigal_i18n::tr(lang, key).to_owned();
    let number = |value: f64, min: usize, max: usize| {
        let text = trimmed(value, min, max);
        if es {
            text.replace('.', ",")
        } else {
            text
        }
    };
    match unit {
        Unit::Plain => number(value, 0, 2),
        Unit::Ratio => format!("{}×", number(value, 1, 2)),
        Unit::Semitones => {
            let steps = value.round() as i64;
            let sign = match steps {
                s if s > 0 => "+",
                s if s < 0 => "−",
                _ => "",
            };
            let name = if steps.abs() == 1 {
                word("settings.unit.semitone")
            } else {
                word("settings.unit.semitones")
            };
            format!("{sign}{} {name}", steps.abs())
        }
        Unit::Decibels => {
            let db = value / 100.0;
            let sign = match db {
                d if d > 0.0 => "+",
                d if d < 0.0 => "−",
                _ => "",
            };
            format!("{sign}{} {}", number(db.abs(), 1, 1), word("settings.unit.db"))
        }
        Unit::Db => {
            let sign = match value {
                d if d > 0.0 => "+",
                d if d < 0.0 => "−",
                _ => "",
            };
            format!(
                "{sign}{} {}",
                number(value.abs(), 1, 1),
                word("settings.unit.db")
            )
        }
        Unit::Percent => format!("{} {}", number(value, 0, 0), word("settings.unit.percent")),
        Unit::Seconds => format!("{} {}", number(value, 0, 1), word("settings.unit.seconds")),
        Unit::Days => format!("{} {}", number(value, 0, 0), word("settings.unit.days")),
        Unit::Pixels => format!("{} {}", number(value, 0, 0), word("settings.unit.px")),
        Unit::Megabytes if value >= 1024.0 => {
            format!("{} {}", number(value / 1024.0, 1, 1), word("settings.unit.gb"))
        }
        Unit::Megabytes => format!("{} {}", number(value, 0, 0), word("settings.unit.mb")),
    }
}

fn display_value(row: &SettingRow, lang: madrigal_i18n::Lang) -> String {
    match row.kind {
        SettingKind::Slider => slider_text(row.unit, row.num_val, lang),
        SettingKind::Choice => row
            .options
            .iter()
            .position(|option| *option == row.text_val)
            .and_then(|index| row.option_labels.get(index).cloned())
            .unwrap_or_else(|| row.text_val.clone()),
        _ => String::new(),
    }
}

fn trimmed(value: f64, min: usize, max: usize) -> String {
    let mut text = format!("{value:.max$}");
    if max > min {
        while text.ends_with('0') && text.len() - text.find('.').unwrap_or(0) - 1 > min {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}

fn slider_unit(key: &str) -> Unit {
    match key {
        "playback_speed" | "line_spacing" | "swipe_sensitivity" => Unit::Ratio,
        "pitch_semitones" => Unit::Semitones,
        "equalizer_band_0" | "equalizer_band_1" | "equalizer_band_2" | "equalizer_band_3"
        | "equalizer_band_4" | "equalizer_band_5" | "equalizer_band_6" | "equalizer_band_7"
        | "equalizer_band_8" | "equalizer_band_9" => Unit::Db,
        "equalizer_output_gain_mb" => Unit::Decibels,
        "volume"
        | "device_mute_recovery_volume"
        | "equalizer_bass_boost_strength"
        | "equalizer_virtualizer_strength"
        | "backdrop_blur_amount"
        | "scrobble_delay_percent" => Unit::Percent,
        "crossfade_seconds"
        | "local_songs_min_duration_secs"
        | "scrobble_min_song_duration_secs"
        | "scrobble_delay_secs" => Unit::Seconds,
        "history_duration_days" | "remind_after" => Unit::Days,
        "font_size" | "thumbnail_corner_radius" => Unit::Pixels,
        "max_image_cache_mb" | "max_song_cache_mb" | "max_canvas_cache_mb"
        | "max_lyrics_cache_mb" => Unit::Megabytes,
        _ => Unit::Plain,
    }
}

fn sw(key: &'static str, label: &'static str, value: bool) -> SettingRow {
    let mut out = row(key, label, SettingKind::Switch, "");
    out.bool_val = value;
    out
}

fn tx(key: &'static str, label: &'static str, value: &str) -> SettingRow {
    let mut out = row(key, label, SettingKind::Text, "");
    out.text_val = value.to_owned();
    out
}

fn sl(key: &'static str, label: &'static str, value: f64, min: f64, max: f64) -> SettingRow {
    let mut out = row(key, label, SettingKind::Slider, "");
    out.num_val = value;
    out.num_min = min;
    out.num_max = max;
    out.unit = slider_unit(key);
    out
}

fn ch(key: &'static str, label: &'static str, current: &str, options: &[&str]) -> SettingRow {
    let mut out = row(key, label, SettingKind::Choice, "");
    out.text_val = current.to_owned();
    out.options = options.iter().map(|option| option.to_string()).collect();
    out.option_labels = out
        .options
        .iter()
        .map(|option| option_label_key(key, option))
        .collect();
    out
}

fn pw(key: &'static str, label: &'static str, value: &str, test_id: &'static str) -> SettingRow {
    let mut out = row(key, label, SettingKind::ApiKey, test_id);
    out.text_val = value.to_owned();
    out
}

fn ev(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|encoded| encoded.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn provider_names(order: &[madrigal_settings::LyricsProvider]) -> Vec<String> {
    order.iter().map(ev).collect()
}

fn enabled_flags(
    order: &[madrigal_settings::LyricsProvider],
    disabled: &[madrigal_settings::LyricsProvider],
) -> Vec<String> {
    order
        .iter()
        .map(|provider| {
            if disabled.contains(provider) {
                "0".to_owned()
            } else {
                "1".to_owned()
            }
        })
        .collect()
}

fn joined(items: &[String]) -> String {
    items.join(", ")
}

fn string_model(items: &[String]) -> slint::ModelRc<SharedString> {
    Rc::new(VecModel::from(
        items
            .iter()
            .map(|item| SharedString::from(item.as_str()))
            .collect::<Vec<_>>(),
    ))
    .into()
}

pub fn settings_section(
    settings: &madrigal_settings::Settings,
    group: SettingsGroup,
    profiles: &[String],
) -> Vec<SettingRow> {
    let raw = match group {
        SettingsGroup::Appearance => appearance_rows(&settings.appearance),
        SettingsGroup::Content => content_rows(&settings.content),
        SettingsGroup::Player => player_rows(&settings.player),
        SettingsGroup::Lyrics => lyrics_rows(&settings.lyrics),
        SettingsGroup::Internet => internet_rows(&settings.internet),
        SettingsGroup::Storage => storage_rows(&settings.storage),
        SettingsGroup::Privacy => privacy_rows(&settings.privacy),
        SettingsGroup::Account => account_rows(&settings.account, profiles),
        SettingsGroup::Integrations => integrations_rows(&settings.integrations),
        SettingsGroup::Ai => ai_rows(&settings.ai),
        SettingsGroup::Updates => updates_rows(&settings.updates),
    };
    arrange(group, raw)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tier {
    Everyday,
    Advanced,
    Hidden,
}

struct Block {
    title: &'static str,
    tier: Tier,
    keys: &'static [&'static str],
}

fn blocks(group: SettingsGroup) -> &'static [Block] {
    match group {
        SettingsGroup::Appearance => &[
            Block { title: "settings.sub.appearance.theme", tier: Tier::Everyday, keys: &["theme", "dark_mode", "dynamic_theme", "pure_black"] },
            Block { title: "settings.sub.appearance.language", tier: Tier::Everyday, keys: &["language", "use_system_language"] },
            Block { title: "settings.sub.appearance.home", tier: Tier::Everyday, keys: &["default_open_tab", "quick_picks_display", "grid_item_size", "artist_view_type", "album_view_type"] },
            Block { title: "settings.sub.appearance.player", tier: Tier::Everyday, keys: &["player_design_style", "player_background", "mini_player_style", "show_player_volume_bar", "hide_player_thumbnail", "swipe_thumbnail", "notify_track_change"] },
            Block { title: "settings.sub.appearance.artwork", tier: Tier::Everyday, keys: &["artwork_shape_enabled", "artwork_shape_style"] },
            Block { title: "settings.sub.appearance.effects", tier: Tier::Everyday, keys: &["disable_animations", "disable_blur", "backdrop_enabled"] },
            Block { title: "settings.block.advanced", tier: Tier::Advanced, keys: &["custom_theme_color", "random_theme_on_startup", "use_system_font", "font_preference", "custom_font_name", "slider_style", "lyrics_background", "lyrics_animation", "player_buttons_style", "crop_thumbnail_to_square", "thumbnail_corner_radius", "blur_radius", "backdrop_blur_amount", "canvas_enabled", "canvas_source", "canvas_wifi_only", "swipe_sensitivity"] },
        ],
        SettingsGroup::Content => &[
            Block { title: "settings.sub.content.region", tier: Tier::Everyday, keys: &["content_language", "content_country"] },
            Block { title: "settings.sub.content.safety", tier: Tier::Everyday, keys: &["hide_explicit", "hide_video"] },
            Block { title: "settings.sub.content.search", tier: Tier::Everyday, keys: &["search_source", "quick_picks", "top_range", "show_home_category_chips", "auto_load_more"] },
            Block { title: "settings.sub.content.sort", tier: Tier::Everyday, keys: &["song_sort_type", "song_sort_descending", "album_sort_type", "album_sort_descending", "artist_sort_type", "artist_sort_descending", "playlist_sort_type", "playlist_sort_descending"] },
            Block { title: "settings.sub.content.library", tier: Tier::Everyday, keys: &["song_filter", "artist_filter", "album_filter", "show_tags_in_library", "prefer_local_on_import", "playlist_edit_lock"] },
            Block { title: "settings.block.advanced", tier: Tier::Advanced, keys: &["playlist_song_sort_type", "playlist_song_sort_descending", "auto_playlist_song_sort_type", "auto_playlist_song_sort_descending", "artist_song_sort_type", "artist_song_sort_descending", "chip_sort_type", "playlist_tags_filter", "library_chip_order", "playlist_tag_order", "artist_separators", "playlist_suggestion_source", "local_songs_min_duration_secs"] },
            Block { title: "", tier: Tier::Hidden, keys: &["speed_dial_song_ids"] },
        ],
        SettingsGroup::Player => &[
            Block { title: "settings.sub.player.volume", tier: Tier::Everyday, keys: &["volume", "audio_quality", "normalization"] },
            Block { title: "settings.sub.player.speed", tier: Tier::Everyday, keys: &["playback_speed", "pitch_semitones"] },
            Block { title: "settings.sub.player.transitions", tier: Tier::Everyday, keys: &["transition_mode", "crossfade_seconds", "skip_silence"] },
            Block { title: "settings.sub.player.equalizer", tier: Tier::Everyday, keys: &["equalizer_enabled", "equalizer_selected_profile_id", "equalizer_band_0", "equalizer_band_1", "equalizer_band_2", "equalizer_band_3", "equalizer_band_4", "equalizer_band_5", "equalizer_band_6", "equalizer_band_7", "equalizer_band_8", "equalizer_band_9", "equalizer_bass_boost_enabled", "equalizer_bass_boost_strength", "equalizer_virtualizer_enabled", "equalizer_virtualizer_strength"] },
            Block { title: "settings.sub.player.resume", tier: Tier::Everyday, keys: &["resume_enabled", "resume_autoplay", "persistent_queue", "permanent_shuffle", "repeat_mode", "queue_edit_lock"] },
            Block { title: "settings.sub.player.listening", tier: Tier::Everyday, keys: &["auto_download_on_like", "auto_skip_next_on_error", "pause_on_device_mute", "seek_extra_seconds"] },
            Block { title: "settings.block.advanced", tier: Tier::Advanced, keys: &["equalizer_control_mode", "equalizer_auto_headroom_enabled", "equalizer_output_gain_enabled", "equalizer_output_gain_mb", "preload_next_song", "audio_offload", "device_mute_recovery_volume"] },
            Block { title: "", tier: Tier::Hidden, keys: &["equalizer_custom_profiles_json"] },
        ],
        SettingsGroup::Lyrics => &[
            Block { title: "settings.sub.lyrics.lookup", tier: Tier::Everyday, keys: &["enabled", "prefer_word_timing", "provider_order"] },
            Block { title: "settings.sub.lyrics.display", tier: Tier::Everyday, keys: &["show_lyrics", "text_position", "font_size", "line_spacing", "line_blur", "show_player_controls"] },
            Block { title: "settings.sub.lyrics.interaction", tier: Tier::Everyday, keys: &["click_to_seek", "auto_scroll"] },
            Block { title: "settings.sub.lyrics.translation", tier: Tier::Everyday, keys: &["translation_enabled", "translation_target"] },
            Block { title: "settings.block.advanced", tier: Tier::Advanced, keys: &["providers_disabled", "preferred_provider", "youtube_subtitle_enabled", "paxsenix_api_key", "translation_endpoint", "translation_api_key", "translator_contexts", "romanize_japanese", "romanize_korean", "romanize_chinese", "romanize_hindi", "romanize_other", "lyrics_mode", "use_lyrics_v2", "v2_bounce_factor", "v2_glow_factor", "v2_fill_transition_width", "v2_lrc_bounce_enabled"] },
        ],
        SettingsGroup::Internet => &[
            Block { title: "settings.sub.internet.connection", tier: Tier::Everyday, keys: &["offline_mode", "network_metered", "use_login_for_browse"] },
            Block { title: "settings.block.advanced", tier: Tier::Advanced, keys: &["proxy_enabled", "proxy_type", "proxy_host", "proxy_port", "proxy_username", "proxy_password", "stream_bypass_proxy", "dns_over_https_enabled", "dns_over_https_provider", "pot_server_home", "web_client_po_token_enabled", "use_visitor_data", "po_token_source_url"] },
        ],
        SettingsGroup::Storage => &[
            Block { title: "settings.sub.storage.downloads", tier: Tier::Everyday, keys: &["downloads_dir", "download_concurrency"] },
            Block { title: "settings.sub.storage.cache", tier: Tier::Everyday, keys: &["max_song_cache_mb", "max_image_cache_mb", "max_lyrics_cache_mb", "max_canvas_cache_mb", "smart_trimmer"] },
            Block { title: "settings.sub.storage.local", tier: Tier::Everyday, keys: &["local_songs_included_folders", "local_songs_sort_type", "local_songs_sort_descending"] },
            Block { title: "settings.block.advanced", tier: Tier::Advanced, keys: &["storage_folder_path", "storage_folder_display_name", "local_songs_excluded_folders"] },
        ],
        SettingsGroup::Privacy => &[
            Block { title: "settings.sub.privacy.history", tier: Tier::Everyday, keys: &["pause_listen_history", "pause_search_history", "history_duration_days"] },
            Block { title: "settings.sub.privacy.security", tier: Tier::Everyday, keys: &["disable_screenshot"] },
            Block { title: "settings.sub.privacy.filter", tier: Tier::Everyday, keys: &["ai_content_filter_enabled", "ai_content_filter_include_moderate"] },
            Block { title: "settings.block.advanced", tier: Tier::Advanced, keys: &["ai_content_filter_source_url"] },
            Block { title: "", tier: Tier::Hidden, keys: &["ai_content_filter_last_updated"] },
        ],
        SettingsGroup::Account => &[],
        SettingsGroup::Integrations => &[
            Block { title: "settings.sub.integrations.discord", tier: Tier::Everyday, keys: &["discord_rpc_enabled", "discord_show_when_paused", "discord_activity_type", "discord_presence_status"] },
            Block { title: "settings.sub.integrations.lastfm", tier: Tier::Everyday, keys: &["lastfm_scrobbling_enabled", "lastfm_use_now_playing", "lastfm_username", "lastfm_provider"] },
            Block { title: "settings.sub.integrations.listenbrainz", tier: Tier::Everyday, keys: &["listenbrainz_enabled", "listenbrainz_token"] },
            Block { title: "settings.sub.integrations.maloja", tier: Tier::Everyday, keys: &["maloja_enabled", "maloja_server", "maloja_api_key"] },
            Block { title: "settings.sub.integrations.sponsorblock", tier: Tier::Everyday, keys: &["sponsorblock_enabled"] },
            Block { title: "settings.sub.integrations.spotify", tier: Tier::Everyday, keys: &["show_spotify_playlists", "cast_enabled"] },
            Block { title: "settings.block.advanced", tier: Tier::Advanced, keys: &["sponsorblock_categories", "sponsorblock_api_url", "lastfm_custom_endpoint", "scrobble_delay_percent", "scrobble_min_song_duration_secs", "scrobble_delay_secs", "discord_client_id", "discord_username", "discord_display_name", "discord_activity_name", "discord_activity_details", "discord_activity_state", "discord_button1_enabled", "discord_button1_label", "discord_button1_url_source", "discord_button1_custom_url", "discord_button2_enabled", "discord_button2_label", "discord_button2_url_source", "discord_button2_custom_url", "discord_large_image_type", "discord_large_text_source", "discord_large_text_custom", "discord_large_image_custom_url", "discord_small_image_type", "discord_small_image_custom_url", "spotify_account_name", "cast_receiver_app_id"] },
            Block { title: "", tier: Tier::Hidden, keys: &["discord_avatar_url", "discord_info_dismissed", "spotify_account_avatar_url"] },
        ],
        SettingsGroup::Ai => &[
            Block { title: "settings.sub.ai.provider", tier: Tier::Everyday, keys: &["provider", "api_key", "selected_model"] },
            Block { title: "settings.block.advanced", tier: Tier::Advanced, keys: &["custom_endpoint", "custom_model", "custom_prompt", "validation_status"] },
        ],
        SettingsGroup::Updates => &[
            Block { title: "settings.sub.updates.updates", tier: Tier::Everyday, keys: &["automatic_update_check", "enable_update_notification", "update_channel", "remind_after"] },
            Block { title: "settings.block.advanced", tier: Tier::Advanced, keys: &["redownload_on_restore"] },
            Block { title: "", tier: Tier::Hidden, keys: &["last_update_check", "last_notified_version", "launch_count", "onboarding_completed", "onboarding_done", "has_pressed_star"] },
        ],
    }
}

fn arrange(group: SettingsGroup, rows: Vec<SettingRow>) -> Vec<SettingRow> {
    if group == SettingsGroup::Account {
        return rows;
    }
    let mut rest = rows;
    let mut out = Vec::with_capacity(rest.len());
    for block in blocks(group) {
        let mut first = true;
        for key in block.keys {
            let Some(position) = rest.iter().position(|row| row.key == *key) else {
                continue;
            };
            let mut row = rest.remove(position);
            if first && block.tier != Tier::Hidden {
                row.section = block.title.to_owned();
            }
            first = false;
            row.advanced = block.tier == Tier::Advanced;
            row.hidden = block.tier == Tier::Hidden;
            out.push(row);
        }
    }
    for mut row in rest {
        row.advanced = true;
        out.push(row);
    }
    for row in &mut out {
        let hint = format!("settings.hint.{}", row.key);
        if madrigal_i18n::contains(&hint) {
            row.hint = hint;
        }
        let placeholder = format!("settings.ph.{}", row.key);
        if madrigal_i18n::contains(&placeholder) {
            row.placeholder = placeholder;
        }
    }
    out
}

fn appearance_rows(a: &madrigal_settings::Appearance) -> Vec<SettingRow> {
    vec![
        ch("theme", "settings.theme", &ev(&a.theme), &["system", "light", "dark", "matugen"]),
        sw("dynamic_theme", "settings.dynamic_theme", a.dynamic_theme),
        ch("dark_mode", "settings.dark_mode", &ev(&a.dark_mode), &["system", "light", "dark"]),
        sw("pure_black", "settings.pure_black", a.pure_black),
        tx("custom_theme_color", "settings.custom_theme_color", &a.custom_theme_color),
        sw("random_theme_on_startup", "settings.random_theme_on_startup", a.random_theme_on_startup),
        sw("disable_animations", "settings.disable_animations", a.disable_animations),
        sw("use_system_font", "settings.use_system_font", a.use_system_font),
        ch("font_preference", "settings.font_preference", &ev(&a.font_preference), &["default", "system", "custom"]),
        tx("custom_font_name", "settings.custom_font_name", &a.custom_font_name),
        ch("language", "settings.language", &ev(&a.language), &["es", "en"]),
        sw("use_system_language", "settings.use_system_language", a.use_system_language),
        ch("default_open_tab", "settings.default_open_tab", &ev(&a.default_open_tab), &["HOME", "EXPLORE", "LIBRARY"]),
        ch("grid_item_size", "settings.grid_item_size", &ev(&a.grid_item_size), &["BIG", "SMALL"]),
        ch("slider_style", "settings.slider_style", &ev(&a.slider_style), &["STANDARD", "WAVY", "THICK", "CIRCULAR", "SIMPLE"]),
        ch("player_design_style", "settings.player_design_style", &ev(&a.player_design_style), &["V1", "V2", "V3", "V4", "V5", "V6", "V7", "V8", "V9", "V10"]),
        sw("show_player_volume_bar", "settings.show_player_volume_bar", a.show_player_volume_bar),
        sw("hide_player_thumbnail", "settings.hide_player_thumbnail", a.hide_player_thumbnail),
        sl("thumbnail_corner_radius", "settings.thumbnail_corner_radius", a.thumbnail_corner_radius as f64, 0.0, 32.0),
        sw("crop_thumbnail_to_square", "settings.crop_thumbnail_to_square", a.crop_thumbnail_to_square),
        sw("disable_blur", "settings.disable_blur", a.disable_blur),
        sl("blur_radius", "settings.blur_radius", a.blur_radius as f64, 0.0, 100.0),
        sw("backdrop_enabled", "settings.backdrop_enabled", a.backdrop_enabled),
        sl("backdrop_blur_amount", "settings.backdrop_blur_amount", a.backdrop_blur_amount as f64, 0.0, 100.0),
        ch("mini_player_style", "settings.mini_player_style", &ev(&a.mini_player_style), &["THEME", "GRADIENT", "GLOW"]),
        ch("player_background", "settings.player_background", &ev(&a.player_background), &["DEFAULT", "GRADIENT", "CUSTOM", "BLUR", "COLORING", "BLUR_GRADIENT", "GLOW", "GLOW_ANIMATED"]),
        ch("lyrics_background", "settings.lyrics_background", &ev(&a.lyrics_background), &["DEFAULT", "FOLLOW_THEME", "COLORING", "CUSTOM"]),
        ch("lyrics_animation", "settings.lyrics_animation", &ev(&a.lyrics_animation), &["FADE", "NONE", "GLOW", "SLIDE", "KARAOKE", "APPLE"]),
        ch("player_buttons_style", "settings.player_buttons_style", &ev(&a.player_buttons_style), &["DEFAULT", "SECONDARY"]),
        sw("artwork_shape_enabled", "settings.artworkShape", a.artwork_shape_enabled),
        ch("artwork_shape_style", "settings.artworkShapeStyle", &ev(&a.artwork_shape_style), &["background", "cover"]),
        sw("notify_track_change", "settings.notify", a.notify_track_change),
        ch("quick_picks_display", "settings.quick_picks_display", &ev(&a.quick_picks_display), &["CARD", "LIST"]),
        ch("artist_view_type", "settings.artist_view_type", &ev(&a.artist_view_type), &["LIST", "GRID"]),
        ch("album_view_type", "settings.album_view_type", &ev(&a.album_view_type), &["LIST", "GRID"]),
        sw("canvas_enabled", "settings.canvas_enabled", a.canvas_enabled),
        tx("canvas_source", "settings.canvas_source", &a.canvas_source),
        sw("canvas_wifi_only", "settings.canvas_wifi_only", a.canvas_wifi_only),
        sw("swipe_thumbnail", "settings.swipe_thumbnail", a.swipe_thumbnail),
        sl("swipe_sensitivity", "settings.swipe_sensitivity", a.swipe_sensitivity as f64, 0.1, 3.0),
    ]
}

fn content_rows(c: &madrigal_settings::Content) -> Vec<SettingRow> {
    vec![
        tx("content_language", "settings.content_language", &c.content_language),
        tx("content_country", "settings.content_country", &c.content_country),
        sw("hide_explicit", "settings.hide_explicit", c.hide_explicit),
        sw("hide_video", "settings.hide_video", c.hide_video),
        ch("search_source", "settings.search_source", &ev(&c.search_source), &["LOCAL", "ONLINE"]),
        ch("song_sort_type", "settings.song_sort_type", &ev(&c.song_sort_type), &["CREATE_DATE", "NAME", "ARTIST", "PLAY_TIME"]),
        sw("song_sort_descending", "settings.song_sort_descending", c.song_sort_descending),
        ch("playlist_song_sort_type", "settings.playlist_song_sort_type", &ev(&c.playlist_song_sort_type), &["CUSTOM", "CREATE_DATE", "NAME", "ARTIST", "PLAY_TIME"]),
        sw("playlist_song_sort_descending", "settings.playlist_song_sort_descending", c.playlist_song_sort_descending),
        ch("auto_playlist_song_sort_type", "settings.auto_playlist_song_sort_type", &ev(&c.auto_playlist_song_sort_type), &["CREATE_DATE", "NAME", "ARTIST", "PLAY_TIME"]),
        sw("auto_playlist_song_sort_descending", "settings.auto_playlist_song_sort_descending", c.auto_playlist_song_sort_descending),
        ch("artist_sort_type", "settings.artist_sort_type", &ev(&c.artist_sort_type), &["CREATE_DATE", "NAME", "SONG_COUNT", "PLAY_TIME"]),
        sw("artist_sort_descending", "settings.artist_sort_descending", c.artist_sort_descending),
        ch("artist_song_sort_type", "settings.artist_song_sort_type", &ev(&c.artist_song_sort_type), &["CREATE_DATE", "NAME", "PLAY_TIME"]),
        sw("artist_song_sort_descending", "settings.artist_song_sort_descending", c.artist_song_sort_descending),
        ch("album_sort_type", "settings.album_sort_type", &ev(&c.album_sort_type), &["CREATE_DATE", "NAME", "ARTIST", "YEAR", "SONG_COUNT", "LENGTH", "PLAY_TIME"]),
        sw("album_sort_descending", "settings.album_sort_descending", c.album_sort_descending),
        ch("playlist_sort_type", "settings.playlist_sort_type", &ev(&c.playlist_sort_type), &["CREATE_DATE", "NAME", "SONG_COUNT", "LAST_UPDATED", "CUSTOM"]),
        sw("playlist_sort_descending", "settings.playlist_sort_descending", c.playlist_sort_descending),
        ch("song_filter", "settings.song_filter", &ev(&c.song_filter), &["LIBRARY", "LIKED", "DOWNLOADED"]),
        ch("artist_filter", "settings.artist_filter", &ev(&c.artist_filter), &["LIBRARY", "LIKED"]),
        ch("album_filter", "settings.album_filter", &ev(&c.album_filter), &["LIBRARY", "LIKED", "DOWNLOADED", "DOWNLOADED_FULL"]),
        tx("chip_sort_type", "settings.chip_sort_type", &c.chip_sort_type),
        tx("playlist_tags_filter", "settings.playlist_tags_filter", &c.playlist_tags_filter),
        tx("library_chip_order", "settings.library_chip_order", &c.library_chip_order),
        tx("playlist_tag_order", "settings.playlist_tag_order", &c.playlist_tag_order),
        sw("show_home_category_chips", "settings.show_home_category_chips", c.show_home_category_chips),
        sw("show_tags_in_library", "settings.show_tags_in_library", c.show_tags_in_library),
        ch("quick_picks", "settings.quick_picks", &ev(&c.quick_picks), &["QUICK_PICKS", "LAST_LISTEN", "DONT_SHOW"]),
        tx("speed_dial_song_ids", "settings.speed_dial_song_ids", &joined(&c.speed_dial_song_ids)),
        ch("top_range", "settings.top_range", &ev(&c.top_range), &["ALL_TIME", "DAY", "WEEK", "MONTH", "YEAR"]),
        ch("playlist_suggestion_source", "settings.playlist_suggestion_source", &ev(&c.playlist_suggestion_source), &["PLAYLIST_TITLE", "PLAYLIST_CONTENT", "BOTH"]),
        tx("artist_separators", "settings.artist_separators", &c.artist_separators),
        sw("auto_load_more", "settings.auto_load_more", c.auto_load_more),
        sl("local_songs_min_duration_secs", "settings.local_songs_min_duration_secs", c.local_songs_min_duration_secs as f64, 0.0, 3600.0),
        sw("playlist_edit_lock", "settings.playlist_edit_lock", c.playlist_edit_lock),
        sw("prefer_local_on_import", "settings.prefer_local_on_import", c.prefer_local_on_import),
    ]
}

fn player_rows(p: &madrigal_settings::Player) -> Vec<SettingRow> {
    let mut rows = vec![
        sl("volume", "settings.player_volume", p.volume as f64, 0.0, 130.0),
        ch("audio_quality", "settings.quality", &ev(&p.audio_quality), &["best", "medium", "low"]),
        sl("playback_speed", "settings.playback_speed", p.playback_speed as f64, 0.5, 2.0),
        sl("pitch_semitones", "settings.pitch_semitones", p.pitch_semitones as f64, -12.0, 12.0),
        sw("equalizer_enabled", "settings.equalizer_enabled", p.equalizer_enabled),
        ch(
            "equalizer_selected_profile_id",
            "settings.equalizer_selected_profile_id",
            &p.equalizer_selected_profile_id,
            &["", "flat", "bass", "treble", "vocal"],
        ),
    ];
    for (index, key) in EQ_BAND_KEYS.iter().enumerate() {
        rows.push(sl(
            key,
            eq_band_label(index),
            p.equalizer_gains.get(index).copied().unwrap_or(0.0) as f64,
            -12.0,
            12.0,
        ));
    }
    rows.push(ch(
        "equalizer_control_mode",
        "settings.equalizer_control_mode",
        &p.equalizer_control_mode,
        &["", "simple", "advanced"],
    ));
    rows.push(sw(
        "equalizer_auto_headroom_enabled",
        "settings.equalizer_auto_headroom_enabled",
        p.equalizer_auto_headroom_enabled,
    ));
    rows.push(sw(
        "equalizer_output_gain_enabled",
        "settings.equalizer_output_gain_enabled",
        p.equalizer_output_gain_enabled,
    ));
    rows.push(sl(
        "equalizer_output_gain_mb",
        "settings.equalizer_output_gain_mb",
        p.equalizer_output_gain_mb as f64,
        -1500.0,
        1500.0,
    ));
    rows.push(sw(
        "equalizer_bass_boost_enabled",
        "settings.equalizer_bass_boost_enabled",
        p.equalizer_bass_boost_enabled,
    ));
    rows.push(sl(
        "equalizer_bass_boost_strength",
        "settings.equalizer_bass_boost_strength",
        p.equalizer_bass_boost_strength as f64,
        0.0,
        100.0,
    ));
    rows.push(sw(
        "equalizer_virtualizer_enabled",
        "settings.equalizer_virtualizer_enabled",
        p.equalizer_virtualizer_enabled,
    ));
    rows.push(sl(
        "equalizer_virtualizer_strength",
        "settings.equalizer_virtualizer_strength",
        p.equalizer_virtualizer_strength as f64,
        0.0,
        100.0,
    ));
    rows.push(tx(
        "equalizer_custom_profiles_json",
        "settings.equalizer_custom_profiles_json",
        &p.equalizer_custom_profiles_json,
    ));
    rows.push(ch(
        "normalization",
        "settings.normalization",
        &ev(&p.normalization),
        &["off", "replaygain_track", "replaygain_album", "dynamic"],
    ));
    rows.push(ch(
        "transition_mode",
        "settings.transition_mode",
        &ev(&p.transition_mode),
        &["normal", "crossfade", "gapless"],
    ));
    rows.push(sl(
        "crossfade_seconds",
        "settings.crossfade_seconds",
        p.crossfade_seconds as f64,
        0.0,
        12.0,
    ));
    rows.push(sw("resume_enabled", "settings.resume_enabled", p.resume_enabled));
    rows.push(sw("resume_autoplay", "settings.resume_autoplay", p.resume_autoplay));
    rows.push(sw("persistent_queue", "settings.persistent_queue", p.persistent_queue));
    rows.push(sw("preload_next_song", "settings.preload_next_song", p.preload_next_song));
    rows.push(sw("permanent_shuffle", "settings.permanent_shuffle", p.permanent_shuffle));
    rows.push(sw("skip_silence", "settings.skip_silence", p.skip_silence));
    rows.push(sw("audio_offload", "settings.audio_offload", p.audio_offload));
    rows.push(sw(
        "auto_download_on_like",
        "settings.auto_download_on_like",
        p.auto_download_on_like,
    ));
    rows.push(sw(
        "auto_skip_next_on_error",
        "settings.auto_skip_next_on_error",
        p.auto_skip_next_on_error,
    ));
    rows.push(sw("pause_on_device_mute", "settings.pause_on_device_mute", p.pause_on_device_mute));
    rows.push(sl(
        "device_mute_recovery_volume",
        "settings.device_mute_recovery_volume",
        p.device_mute_recovery_volume as f64,
        0.0,
        130.0,
    ));
    rows.push(ch(
        "repeat_mode",
        "settings.repeat_mode",
        &ev(&p.repeat_mode),
        &["off", "one", "all"],
    ));
    rows.push(sw("queue_edit_lock", "settings.queue_edit_lock", p.queue_edit_lock));
    rows.push(sw("seek_extra_seconds", "settings.seek_extra_seconds", p.seek_extra_seconds));
    rows
}

const EQ_BAND_KEYS: [&str; 10] = [
    "equalizer_band_0",
    "equalizer_band_1",
    "equalizer_band_2",
    "equalizer_band_3",
    "equalizer_band_4",
    "equalizer_band_5",
    "equalizer_band_6",
    "equalizer_band_7",
    "equalizer_band_8",
    "equalizer_band_9",
];

fn eq_band_label(index: usize) -> &'static str {
    match index {
        0 => "settings.eq.band0",
        1 => "settings.eq.band1",
        2 => "settings.eq.band2",
        3 => "settings.eq.band3",
        4 => "settings.eq.band4",
        5 => "settings.eq.band5",
        6 => "settings.eq.band6",
        7 => "settings.eq.band7",
        8 => "settings.eq.band8",
        _ => "settings.eq.band9",
    }
}

fn lyrics_rows(l: &madrigal_settings::Lyrics) -> Vec<SettingRow> {
    let mut order_row = row("provider_order", "settings.lyricsProviders", SettingKind::Providers, "");
    order_row.options = provider_names(&l.provider_order);
    order_row.flags = enabled_flags(&l.provider_order, &l.providers_disabled);
    let mut preferred = ch(
        "preferred_provider",
        "settings.preferred_provider",
        &l.preferred_provider.map(|provider| ev(&provider)).unwrap_or_default(),
        &[],
    );
    preferred.options = ["".to_owned()]
        .into_iter()
        .chain(provider_names(&madrigal_settings::DEFAULT_PROVIDER_ORDER))
        .collect();
    preferred.option_labels = preferred
        .options
        .iter()
        .map(|option| option_label_key("preferred_provider", option))
        .collect();
    for (label, option) in preferred.option_labels.iter_mut().zip(&preferred.options) {
        if !option.is_empty() {
            *label = format!("lyrics.provider.{option}");
        }
    }
    vec![
        sw("enabled", "settings.lyrics", l.enabled),
        sw("show_lyrics", "settings.show_lyrics", l.show_lyrics),
        sl("font_size", "settings.font_size", l.font_size as f64, 10.0, 28.0),
        sl("line_spacing", "settings.line_spacing", l.line_spacing as f64, 0.5, 3.0),
        sw("line_blur", "settings.line_blur", l.line_blur),
        sw("show_player_controls", "settings.show_player_controls", l.show_player_controls),
        ch("text_position", "settings.text_position", &ev(&l.text_position), &["TOP", "CENTER", "BOTTOM"]),
        sw("click_to_seek", "settings.click_to_seek", l.click_to_seek),
        sw("auto_scroll", "settings.auto_scroll", l.auto_scroll),
        sw("prefer_word_timing", "settings.lyricsWordTiming", l.prefer_word_timing),
        order_row,
        tx("providers_disabled", "settings.providers_disabled", &provider_names(&l.providers_disabled).join(", ")),
        preferred,
        sw("youtube_subtitle_enabled", "settings.youtube_subtitle_enabled", l.youtube_subtitle_enabled),
        pw("paxsenix_api_key", "settings.lyricsPaxsenix", &l.paxsenix_api_key, ""),
        sw("translation_enabled", "settings.translation_enabled", l.translation_enabled),
        tx("translation_endpoint", "settings.translation_endpoint", &l.translation_endpoint),
        pw("translation_api_key", "settings.translation_api_key", &l.translation_api_key, ""),
        tx("translation_target", "settings.translation_target", &l.translation_target),
        tx("translator_contexts", "settings.translator_contexts", &l.translator_contexts),
        sw("romanize_japanese", "settings.romanize_japanese", l.romanize_japanese),
        sw("romanize_korean", "settings.romanize_korean", l.romanize_korean),
        sw("romanize_chinese", "settings.romanize_chinese", l.romanize_chinese),
        sw("romanize_hindi", "settings.romanize_hindi", l.romanize_hindi),
        sw("romanize_other", "settings.romanize_other", l.romanize_other),
        ch("lyrics_mode", "settings.lyrics_mode", &ev(&l.lyrics_mode), &["V2", "ENHANCED"]),
        sw("use_lyrics_v2", "settings.use_lyrics_v2", l.use_lyrics_v2),
        sl("v2_bounce_factor", "settings.v2_bounce_factor", l.v2_bounce_factor as f64, 0.0, 2.0),
        sl("v2_glow_factor", "settings.v2_glow_factor", l.v2_glow_factor as f64, 0.0, 2.0),
        sl("v2_fill_transition_width", "settings.v2_fill_transition_width", l.v2_fill_transition_width as f64, 0.0, 1.0),
        sw("v2_lrc_bounce_enabled", "settings.v2_lrc_bounce_enabled", l.v2_lrc_bounce_enabled),
    ]
}

fn internet_rows(n: &madrigal_settings::Internet) -> Vec<SettingRow> {
    let mut pot = tx("pot_server_home", "settings.pot", &n.pot_server_home);
    pot.test_id = "po";
    vec![
        pot,
        sw("proxy_enabled", "settings.proxy_enabled", n.proxy_enabled),
        tx("proxy_host", "settings.proxy_host", &n.proxy_host),
        sl("proxy_port", "settings.proxy_port", n.proxy_port as f64, 0.0, 65535.0),
        tx("proxy_username", "settings.proxy_username", &n.proxy_username),
        pw("proxy_password", "settings.proxy_password", &n.proxy_password, ""),
        ch("proxy_type", "settings.proxy_type", &ev(&n.proxy_type), &["http", "socks5"]),
        sw("dns_over_https_enabled", "settings.dns_over_https_enabled", n.dns_over_https_enabled),
        tx("dns_over_https_provider", "settings.dns_over_https_provider", &n.dns_over_https_provider),
        sw("stream_bypass_proxy", "settings.stream_bypass_proxy", n.stream_bypass_proxy),
        sw("network_metered", "settings.network_metered", n.network_metered),
        sw("use_login_for_browse", "settings.use_login_for_browse", n.use_login_for_browse),
        sw("web_client_po_token_enabled", "settings.web_client_po_token_enabled", n.web_client_po_token_enabled),
        sw("use_visitor_data", "settings.use_visitor_data", n.use_visitor_data),
        tx("po_token_source_url", "settings.po_token_source_url", &n.po_token_source_url),
        sw("offline_mode", "settings.offline_mode", n.offline_mode),
    ]
}

fn storage_rows(s: &madrigal_settings::Storage) -> Vec<SettingRow> {
    let downloads_dir = if s.downloads_dir.trim().is_empty() {
        madrigal_downloads::default_downloads_dir()
            .to_string_lossy()
            .into_owned()
    } else {
        s.downloads_dir.clone()
    };
    vec![
        tx("downloads_dir", "settings.downloads", &downloads_dir),
        sl("download_concurrency", "settings.download_concurrency", s.download_concurrency as f64, 1.0, 6.0),
        sl("max_image_cache_mb", "settings.max_image_cache_mb", s.max_image_cache_mb as f64, 0.0, 1048576.0),
        sl("max_song_cache_mb", "settings.max_song_cache_mb", s.max_song_cache_mb as f64, 0.0, 1048576.0),
        sl("max_canvas_cache_mb", "settings.max_canvas_cache_mb", s.max_canvas_cache_mb as f64, 0.0, 1048576.0),
        sl("max_lyrics_cache_mb", "settings.max_lyrics_cache_mb", s.max_lyrics_cache_mb as f64, 0.0, 1048576.0),
        sw("smart_trimmer", "settings.smart_trimmer", s.smart_trimmer),
        tx("storage_folder_path", "settings.storage_folder_path", &s.storage_folder_path),
        tx("storage_folder_display_name", "settings.storage_folder_display_name", &s.storage_folder_display_name),
        tx("local_songs_included_folders", "settings.folders", &joined(&s.local_songs_included_folders)),
        tx("local_songs_excluded_folders", "settings.local_songs_excluded_folders", &joined(&s.local_songs_excluded_folders)),
        ch("local_songs_sort_type", "settings.local_songs_sort_type", &ev(&s.local_songs_sort_type), &["CREATE_DATE", "NAME", "ARTIST", "PLAY_TIME"]),
        sw("local_songs_sort_descending", "settings.local_songs_sort_descending", s.local_songs_sort_descending),
    ]
}

fn privacy_rows(p: &madrigal_settings::Privacy) -> Vec<SettingRow> {
    vec![
        sw("pause_listen_history", "settings.pause_listen_history", p.pause_listen_history),
        sw("pause_search_history", "settings.pause_search_history", p.pause_search_history),
        sw("disable_screenshot", "settings.disable_screenshot", p.disable_screenshot),
        sl("history_duration_days", "settings.history_duration_days", p.history_duration_days as f64, 5.0, 60.0),
        sw("ai_content_filter_enabled", "settings.ai_content_filter_enabled", p.ai_content_filter_enabled),
        sw("ai_content_filter_include_moderate", "settings.ai_content_filter_include_moderate", p.ai_content_filter_include_moderate),
        tx("ai_content_filter_last_updated", "settings.ai_content_filter_last_updated", &p.ai_content_filter_last_updated.to_string()),
        tx("ai_content_filter_source_url", "settings.ai_content_filter_source_url", &p.ai_content_filter_source_url),
    ]
}

fn account_rows(a: &madrigal_settings::Account, _profiles: &[String]) -> Vec<SettingRow> {
    vec![
        sw("ytm_sync", "settings.ytm_sync", a.ytm_sync),
        sw("force_sync_on_account_switch", "settings.force_sync_on_account_switch", a.force_sync_on_account_switch),
    ]
}

fn integrations_rows(g: &madrigal_settings::Integrations) -> Vec<SettingRow> {
    let maloja_key = pw("maloja_api_key", "settings.malojaKey", &g.maloja_api_key, "maloja");
    vec![
        sw("sponsorblock_enabled", "settings.sponsor", g.sponsorblock_enabled),
        tx("sponsorblock_categories", "settings.sponsorblock_categories", &sponsor_names(&g.sponsorblock_categories).join(", ")),
        tx("sponsorblock_api_url", "settings.sponsorblock_api_url", &g.sponsorblock_api_url),
        sw("maloja_enabled", "settings.maloja", g.maloja_enabled),
        tx("maloja_server", "settings.malojaServer", &g.maloja_server),
        maloja_key,
        sw("listenbrainz_enabled", "settings.listenbrainz_enabled", g.listenbrainz_enabled),
        pw("listenbrainz_token", "settings.listenbrainz_token", &g.listenbrainz_token, ""),
        sw("lastfm_scrobbling_enabled", "settings.lastfm_scrobbling_enabled", g.lastfm_scrobbling_enabled),
        tx("lastfm_username", "settings.lastfm_username", &g.lastfm_username),
        ch("lastfm_provider", "settings.lastfm_provider", &ev(&g.lastfm_provider), &["LASTFM", "LIBREFM", "CUSTOM"]),
        tx("lastfm_custom_endpoint", "settings.lastfm_custom_endpoint", &g.lastfm_custom_endpoint),
        sw("lastfm_use_now_playing", "settings.lastfm_use_now_playing", g.lastfm_use_now_playing),
        sl("scrobble_delay_percent", "settings.scrobble_delay_percent", g.scrobble_delay_percent as f64, 0.0, 100.0),
        sl("scrobble_min_song_duration_secs", "settings.scrobble_min_song_duration_secs", g.scrobble_min_song_duration_secs as f64, 0.0, 600.0),
        sl("scrobble_delay_secs", "settings.scrobble_delay_secs", g.scrobble_delay_secs as f64, 0.0, 600.0),
        sw("discord_rpc_enabled", "settings.discord_rpc_enabled", g.discord_rpc_enabled),
        tx("discord_client_id", "settings.discord_client_id", &g.discord_client_id),
        tx("discord_username", "settings.discord_username", &g.discord_username),
        tx("discord_display_name", "settings.discord_display_name", &g.discord_display_name),
        tx("discord_avatar_url", "settings.discord_avatar_url", &g.discord_avatar_url),
        sw("discord_info_dismissed", "settings.discord_info_dismissed", g.discord_info_dismissed),
        tx("discord_activity_name", "settings.discord_activity_name", &g.discord_activity_name),
        tx("discord_activity_details", "settings.discord_activity_details", &g.discord_activity_details),
        tx("discord_activity_state", "settings.discord_activity_state", &g.discord_activity_state),
        tx("discord_button1_label", "settings.discord_button1_label", &g.discord_button1_label),
        tx("discord_button1_url_source", "settings.discord_button1_url_source", &g.discord_button1_url_source),
        tx("discord_button1_custom_url", "settings.discord_button1_custom_url", &g.discord_button1_custom_url),
        sw("discord_button1_enabled", "settings.discord_button1_enabled", g.discord_button1_enabled),
        tx("discord_button2_label", "settings.discord_button2_label", &g.discord_button2_label),
        tx("discord_button2_url_source", "settings.discord_button2_url_source", &g.discord_button2_url_source),
        tx("discord_button2_custom_url", "settings.discord_button2_custom_url", &g.discord_button2_custom_url),
        sw("discord_button2_enabled", "settings.discord_button2_enabled", g.discord_button2_enabled),
        sw("discord_show_when_paused", "settings.discord_show_when_paused", g.discord_show_when_paused),
        ch("discord_activity_type", "settings.discord_activity_type", &ev(&g.discord_activity_type), &["PLAYING", "STREAMING", "LISTENING", "WATCHING", "COMPETING"]),
        ch("discord_presence_status", "settings.discord_presence_status", &ev(&g.discord_presence_status), &["ONLINE", "IDLE", "DND", "INVISIBLE"]),
        tx("discord_large_image_type", "settings.discord_large_image_type", &g.discord_large_image_type),
        tx("discord_large_text_source", "settings.discord_large_text_source", &g.discord_large_text_source),
        tx("discord_large_text_custom", "settings.discord_large_text_custom", &g.discord_large_text_custom),
        tx("discord_large_image_custom_url", "settings.discord_large_image_custom_url", &g.discord_large_image_custom_url),
        tx("discord_small_image_type", "settings.discord_small_image_type", &g.discord_small_image_type),
        tx("discord_small_image_custom_url", "settings.discord_small_image_custom_url", &g.discord_small_image_custom_url),
        sw("show_spotify_playlists", "settings.show_spotify_playlists", g.show_spotify_playlists),
        tx("spotify_account_name", "settings.spotify_account_name", &g.spotify_account_name),
        tx("spotify_account_avatar_url", "settings.spotify_account_avatar_url", &g.spotify_account_avatar_url),
        sw("cast_enabled", "settings.cast_enabled", g.cast_enabled),
        tx("cast_receiver_app_id", "settings.cast_receiver_app_id", &g.cast_receiver_app_id),
    ]
}

fn sponsor_names(categories: &[madrigal_settings::SponsorCategory]) -> Vec<String> {
    categories.iter().map(ev).collect()
}

fn ai_rows(a: &madrigal_settings::Ai) -> Vec<SettingRow> {
    vec![
        ch("provider", "settings.ai_provider", &ev(&a.provider), &["CHATGPT", "GEMINI", "OPENROUTER", "CUSTOM", "NONE"]),
        tx("custom_endpoint", "settings.custom_endpoint", &a.custom_endpoint),
        pw("api_key", "settings.malojaKey", &a.api_key, "ai"),
        ch("validation_status", "settings.validation_status", &ev(&a.validation_status), &["UNKNOWN", "SUCCESS", "FAILED"]),
        tx("selected_model", "settings.selected_model", &a.selected_model),
        tx("custom_model", "settings.custom_model", &a.custom_model),
        tx("custom_prompt", "settings.custom_prompt", &a.custom_prompt),
    ]
}

fn updates_rows(u: &madrigal_settings::Updates) -> Vec<SettingRow> {
    vec![
        sw("automatic_update_check", "settings.automatic_update_check", u.automatic_update_check),
        sw("enable_update_notification", "settings.enable_update_notification", u.enable_update_notification),
        ch("update_channel", "settings.update_channel", &ev(&u.update_channel), &["STABLE", "ARTIFACT"]),
        tx("last_update_check", "settings.last_update_check", &u.last_update_check.to_string()),
        tx("last_notified_version", "settings.last_notified_version", &u.last_notified_version),
        sw("redownload_on_restore", "settings.redownload_on_restore", u.redownload_on_restore),
        sl("launch_count", "settings.launch_count", u.launch_count as f64, 0.0, 100000.0),
        sw("onboarding_completed", "settings.onboarding_completed", u.onboarding_completed),
        sw("onboarding_done", "settings.onboarding_done", u.onboarding_done),
        sw("has_pressed_star", "settings.has_pressed_star", u.has_pressed_star),
        sl("remind_after", "settings.remind_after", u.remind_after as f64, 0.0, 365.0),
    ]
}

#[derive(Clone, Debug)]
pub enum SettingsValue {
    Bool(bool),
    Text(String),
    Num(f64),
    Choice(String),
    Move { from: usize, to: usize },
}

fn split_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

fn parse_provider_list(raw: &str) -> Result<Vec<madrigal_settings::LyricsProvider>, String> {
    let mut out = Vec::new();
    for name in split_list(raw) {
        let found = madrigal_settings::DEFAULT_PROVIDER_ORDER
            .iter()
            .find(|provider| ev(provider) == name)
            .copied()
            .ok_or_else(|| format!("unknown lyrics provider: {name}"))?;
        if !out.contains(&found) {
            out.push(found);
        }
    }
    Ok(out)
}

fn parse_sponsor_list(raw: &str) -> Result<Vec<madrigal_settings::SponsorCategory>, String> {
    let all = [
        madrigal_settings::SponsorCategory::Sponsor,
        madrigal_settings::SponsorCategory::SelfPromo,
        madrigal_settings::SponsorCategory::Interaction,
        madrigal_settings::SponsorCategory::Intro,
        madrigal_settings::SponsorCategory::Outro,
        madrigal_settings::SponsorCategory::Preview,
        madrigal_settings::SponsorCategory::MusicOfftopic,
        madrigal_settings::SponsorCategory::Filler,
        madrigal_settings::SponsorCategory::Hook,
    ];
    let mut out = Vec::new();
    for name in split_list(raw) {
        let found = all
            .iter()
            .find(|category| ev(category) == name)
            .copied()
            .ok_or_else(|| format!("unknown sponsor category: {name}"))?;
        if !out.contains(&found) {
            out.push(found);
        }
    }
    Ok(out)
}

fn parse_gains(raw: &str) -> Result<Vec<f32>, String> {
    split_list(raw)
        .iter()
        .map(|item| {
            item.parse::<f32>()
                .map_err(|_| format!("invalid gain value: {item}"))
        })
        .collect()
}

pub fn apply_value(
    settings: &mut madrigal_settings::Settings,
    group: SettingsGroup,
    field: &str,
    value: &SettingsValue,
) -> Result<(), String> {
    if group == SettingsGroup::Player {
        if let Some(band) = field.strip_prefix("equalizer_band_") {
            let index: usize = band
                .parse()
                .map_err(|_| format!("unknown field {field}"))?;
            let SettingsValue::Num(gain) = value else {
                return Err("equalizer band needs a num value".to_owned());
            };
            let gains = &mut settings.player.equalizer_gains;
            if index < gains.len() {
                let gain = *gain as f32;
                gains[index] = if gain.is_finite() {
                    gain.clamp(-12.0, 12.0)
                } else {
                    0.0
                };
                settings.player.equalizer_selected_profile_id.clear();
                return Ok(());
            }
            return Err(format!("unknown field {field}"));
        }
    }
    if group == SettingsGroup::Lyrics && field == "provider_order" {
        let SettingsValue::Move { from, to } = value else {
            return Err("provider_order needs a move value".to_owned());
        };
        let order = &mut settings.lyrics.provider_order;
        if order.is_empty() || *from >= order.len() || *to >= order.len() {
            return Err("provider move out of range".to_owned());
        }
        let moved = order.remove(*from);
        order.insert(*to, moved);
        return Ok(());
    }
    if group == SettingsGroup::Lyrics && field == "providers_disabled" {
        let SettingsValue::Text(raw) = value else {
            return Err("providers_disabled needs a text value".to_owned());
        };
        settings.lyrics.providers_disabled = parse_provider_list(raw)?;
        return Ok(());
    }
    let mut doc = serde_json::to_value(&settings).map_err(|err| err.to_string())?;
    let node = doc
        .get_mut(group_key(group))
        .and_then(|section| section.get_mut(field))
        .ok_or_else(|| format!("unknown field {field}"))?;
    let next = match (node.clone(), value) {
        (serde_json::Value::Bool(_), SettingsValue::Bool(flag)) => {
            serde_json::Value::Bool(*flag)
        }
        (serde_json::Value::Number(current), SettingsValue::Num(number)) => {
            if current.is_i64() || current.is_u64() {
                serde_json::Value::from(number.round() as i64)
            } else {
                serde_json::Value::from(*number)
            }
        }
        (serde_json::Value::String(_), SettingsValue::Text(text)) => {
            serde_json::Value::String(text.clone())
        }
        (serde_json::Value::String(_), SettingsValue::Choice(choice)) => {
            serde_json::Value::String(choice.clone())
        }
        (serde_json::Value::Null, SettingsValue::Choice(choice)) => {
            if choice.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(choice.clone())
            }
        }
        (serde_json::Value::Array(_), SettingsValue::Text(raw)) => {
            match (group, field) {
                (SettingsGroup::Player, "equalizer_gains") => {
                    serde_json::to_value(parse_gains(raw)?).map_err(|err| err.to_string())?
                }
                (SettingsGroup::Integrations, "sponsorblock_categories") => {
                    serde_json::to_value(parse_sponsor_list(raw)?).map_err(|err| err.to_string())?
                }
                _ => serde_json::Value::Array(
                    split_list(raw)
                        .iter()
                        .map(|item| serde_json::Value::String(item.clone()))
                        .collect(),
                ),
            }
        }
        _ => return Err(format!("type mismatch for field {field}")),
    };
    *node = next;
    let back: madrigal_settings::Settings =
        serde_json::from_value(doc).map_err(|err| err.to_string())?;
    *settings = back;
    if group == SettingsGroup::Player && field == "equalizer_selected_profile_id" {
        if let SettingsValue::Choice(name) = value {
            if let Some(gains) = crate::eq_preset_gains(name) {
                settings
                    .player
                    .equalizer_gains
                    .iter_mut()
                    .zip(gains.iter())
                    .for_each(|(slot, gain)| *slot = *gain);
            }
        }
    }
    Ok(())
}

pub fn apply_to_file(
    path: &Path,
    group: SettingsGroup,
    field: &str,
    value: &SettingsValue,
) -> Result<madrigal_settings::Settings, String> {
    let mut settings = madrigal_settings::Settings::load_from(path);
    apply_value(&mut settings, group, field, value)?;
    settings.save_to(path).map_err(|err| err.to_string())?;
    Ok(madrigal_settings::Settings::load_from(path))
}

pub fn field_count(group: SettingsGroup) -> usize {
    settings_section(&madrigal_settings::Settings::default(), group, &[]).len()
}

pub const MASKED_SECRET: &str = "••••••••";

pub fn detect_po_provider() -> bool {
    let probe = std::process::Command::new("pgrep")
        .arg("-f")
        .arg("bgutil")
        .output();
    match probe {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

pub struct SettingsPresenter {
    pub path: PathBuf,
    pub settings: madrigal_settings::Settings,
    pub profiles: Vec<String>,
    pub group: SettingsGroup,
    pub shown_secrets: HashSet<String>,
    pub test_calls: Vec<String>,
    pub login_calls: Vec<String>,
    pub scan_calls: Vec<String>,
    pub po_done: bool,
    pub po_ok: bool,
    pub po_detect: fn() -> bool,
    pub diag_dir: PathBuf,
    pub diag_level: i32,
    pub diag_export_path: PathBuf,
    pub diag_export_done: bool,
    pub latency: Option<madrigal_player::latency::LatencyReport>,
    pub single_primary: bool,
    pub test_result: String,
    pub ob_folder: String,
    pub account_connected: bool,
    pub account_name: String,
    pub account_email: String,
    pub account_busy: bool,
    pub account_error: String,
    pub account_choices: Vec<String>,
}

impl SettingsPresenter {
    pub fn load(path: &Path, profiles: Vec<String>, diag_dir: &Path) -> Self {
        let settings = madrigal_settings::Settings::load_from(path);
        let export = diag_dir.join("diag-export.log");
        Self {
            path: path.to_path_buf(),
            settings,
            profiles,
            group: SettingsGroup::Appearance,
            shown_secrets: HashSet::new(),
            test_calls: Vec::new(),
            login_calls: Vec::new(),
            scan_calls: Vec::new(),
            po_done: false,
            po_ok: false,
            po_detect: detect_po_provider,
            diag_dir: diag_dir.to_path_buf(),
            diag_level: -1,
            diag_export_path: export,
            diag_export_done: false,
            latency: None,
            single_primary: false,
            test_result: String::new(),
            ob_folder: String::new(),
            account_connected: false,
            account_name: String::new(),
            account_email: String::new(),
            account_busy: false,
            account_error: String::new(),
            account_choices: Vec::new(),
        }
    }

    pub fn begin_account_work(&mut self) {
        self.account_busy = true;
        self.account_error.clear();
        self.account_choices.clear();
    }

    pub fn set_account_choices(&mut self, choices: Vec<String>) {
        self.account_choices = choices;
    }

    pub fn finish_account_work(&mut self) {
        self.account_busy = false;
    }

    pub fn set_account_error(&mut self, key: &str) {
        self.account_error = key.to_owned();
    }

    pub fn set_account_connected(&mut self, name: &str, email: &str) {
        self.account_connected = true;
        self.account_name = name.to_owned();
        self.account_email = email.to_owned();
        self.account_error.clear();
        self.account_choices.clear();
    }

    pub fn clear_account(&mut self) {
        self.account_connected = false;
        self.account_name.clear();
        self.account_email.clear();
        self.account_error.clear();
        self.account_choices.clear();
    }

    pub fn refresh(&mut self) {
        self.settings = madrigal_settings::Settings::load_from(&self.path);
    }

    pub fn rows(&self) -> Vec<SettingRow> {
        settings_section(&self.settings, self.group, &self.profiles)
    }

    pub fn ui_rows(&self, lang: madrigal_i18n::Lang) -> Vec<UiSettingsRow> {
        self.rows()
            .iter()
            .map(|row| self.to_ui(row, lang))
            .collect()
    }

    fn to_ui(&self, row: &SettingRow, lang: madrigal_i18n::Lang) -> UiSettingsRow {
        let shown = row.kind != SettingKind::ApiKey || self.shown_secrets.contains(row.key);
        UiSettingsRow {
            key: row.key.into(),
            label: row.label_key.into(),
            kind: kind_tag(row.kind).into(),
            val_bool: row.bool_val,
            val_text: row.text_val.clone().into(),
            display: if shown {
                row.text_val.clone().into()
            } else {
                MASKED_SECRET.into()
            },
            val_num: row.num_val as f32,
            val_min: row.num_min as f32,
            val_max: row.num_max as f32,
            val_label: display_value(row, lang).into(),
            secret: shown,
            test: row.test_id.into(),
            options: string_model(&row.options),
            option_labels: string_model(&row.option_labels),
            flags: string_model(&row.flags),
            section: row.section.as_str().into(),
            advanced: row.advanced,
            hidden: row.hidden,
            hint: row.hint.as_str().into(),
            placeholder: row.placeholder.as_str().into(),
        }
    }

    pub fn apply(&mut self, group: SettingsGroup, field: &str, value: SettingsValue) -> Result<(), String> {
        let next = apply_to_file(&self.path, group, field, &value)?;
        self.settings = next;
        Ok(())
    }

    pub fn toggle_secret(&mut self, key: &str) {
        if !self.shown_secrets.remove(key) {
            self.shown_secrets.insert(key.to_owned());
        }
    }

    pub fn run_test(&mut self, id: &str) {
        self.test_calls.push(id.to_owned());
        let ok = match id {
            "po" => {
                let found = (self.po_detect)();
                self.po_done = true;
                self.po_ok = found;
                found
            }
            _ => true,
        };
        self.test_result = if ok {
            "settings.test.ok".to_owned()
        } else {
            "settings.test.fail".to_owned()
        };
    }

    pub fn check_po(&mut self) {
        self.run_test("po");
    }

    pub fn finish_onboarding(&mut self) -> Result<(), String> {
        let next = apply_to_file(
            &self.path,
            SettingsGroup::Updates,
            "onboarding_done",
            &SettingsValue::Bool(true),
        )?;
        self.settings = next;
        Ok(())
    }

    pub fn needs_onboarding(&self) -> bool {
        !self.settings.updates.onboarding_done
    }

    pub fn scan_folder(&mut self, folder: &str) {
        let folder = folder.trim().to_owned();
        if folder.is_empty() {
            return;
        }
        self.scan_calls.push(folder.clone());
        let mut folders = self.settings.storage.local_songs_included_folders.clone();
        if !folders.iter().any(|entry| entry == &folder) {
            folders.push(folder);
            if let Ok(next) = apply_to_file(
                &self.path,
                SettingsGroup::Storage,
                "local_songs_included_folders",
                &SettingsValue::Text(folders.join(", ")),
            ) {
                self.settings = next;
            }
        }
    }

    pub fn diag_lines(&self) -> Vec<(i32, String)> {
        let log = madrigal_player::diag_log::DiagLog::open_in(
            &self.diag_dir,
            madrigal_player::diag_log::DiagLogConfig::default(),
        )
        .ok();
        let min = match self.diag_level {
            1 => madrigal_player::diag_log::Level::Warn,
            2 => madrigal_player::diag_log::Level::Error,
            _ => madrigal_player::diag_log::Level::Info,
        };
        let entries = log.and_then(|log| log.read(min).ok()).unwrap_or_default();
        entries
            .iter()
            .map(|entry| {
                let level = match entry.level {
                    madrigal_player::diag_log::Level::Info => 0,
                    madrigal_player::diag_log::Level::Warn => 1,
                    madrigal_player::diag_log::Level::Error => 2,
                };
                (level, format!("{} [{}] {}", entry.epoch_secs, entry.prefix, entry.text))
            })
            .collect()
    }

    pub fn export_diag(&mut self) -> std::io::Result<PathBuf> {
        let log = madrigal_player::diag_log::DiagLog::open_in(
            &self.diag_dir,
            madrigal_player::diag_log::DiagLogConfig::default(),
        )?;
        let dest = log.export(&self.diag_export_path)?;
        self.diag_export_done = true;
        Ok(dest)
    }

    pub fn latency_text(&self) -> String {
        match self.latency {
            Some(report) => format!(
                "p50 {:.1} ms · p95 {:.1} ms",
                report.total.p50_ms, report.total.p95_ms
            ),
            None => "p50 — · p95 —".to_owned(),
        }
    }

    pub fn single_text(&self) -> String {
        if self.single_primary {
            "settings.diag.singlePrimary".to_owned()
        } else {
            "settings.diag.singleSecondary".to_owned()
        }
    }
}

fn set_model_rows(shell: &MainShell, state: &SettingsPresenter) {
    let lang = if shell.global::<crate::I18n>().get_lang().as_str() == "en" {
        madrigal_i18n::Lang::En
    } else {
        madrigal_i18n::Lang::Es
    };
    let items: Vec<UiSettingsRow> = state.ui_rows(lang);
    shell.set_settings_rows(Rc::new(VecModel::from(items)).into());
}

fn set_diag_entries(shell: &MainShell, state: &SettingsPresenter) {
    use crate::DiagLine;
    let items: Vec<DiagLine> = state
        .diag_lines()
        .iter()
        .map(|(level, text)| DiagLine {
            level: *level,
            text: text.clone().into(),
        })
        .collect();
    shell.set_settings_diag_entries(Rc::new(VecModel::from(items)).into());
}

fn refresh_shell(shell: &MainShell, state: &SettingsPresenter) {
    use slint::Model as _;
    shell.set_settings_group(group_index(state.group));
    set_model_rows(shell, state);
    shell.set_settings_show_onboarding(state.needs_onboarding());
    shell.set_settings_test_result(state.test_result.clone().into());
    shell.set_settings_po_done(state.po_done);
    shell.set_settings_po_ok(state.po_ok);
    shell.set_settings_account_connected(state.account_connected);
    shell.set_settings_account_name(state.account_name.clone().into());
    shell.set_settings_account_email(state.account_email.clone().into());
    shell.set_settings_account_busy(state.account_busy);
    shell.set_settings_account_error(state.account_error.clone().into());
    shell.set_settings_account_choices(
        Rc::new(VecModel::from(
            state
                .account_choices
                .iter()
                .map(|choice| SharedString::from(choice.as_str()))
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    shell.set_settings_diag_level(state.diag_level);
    set_diag_entries(shell, state);
    shell.set_settings_diag_entry_count(shell.get_settings_diag_entries().row_count() as i32);
    shell.set_settings_diag_exported(state.diag_export_done);
    shell.set_settings_diag_export_status(state.diag_export_path.to_string_lossy().into_owned().into());
    shell.set_settings_diag_latency(state.latency_text().into());
    shell.set_settings_diag_single(state.single_text().into());
    if shell.get_settings_ob_folder().as_str() != state.ob_folder {
        shell.set_settings_ob_folder(state.ob_folder.clone().into());
    }
}

pub fn refresh_settings(shell: &MainShell, state: &SettingsPresenter) {
    refresh_shell(shell, state);
}

pub fn wire_shell(
    shell: &MainShell,
    state: Rc<RefCell<SettingsPresenter>>,
    lang_slot: &Rc<Cell<madrigal_i18n::Lang>>,
) {
    shell.set_settings_group_titles(
        Rc::new(VecModel::from(
            group_titles()
                .iter()
                .map(|title| SharedString::from(title.as_str()))
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    refresh_shell(shell, &state.borrow());
    let lang_slot = lang_slot.clone();

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_group_selected(move |index| {
        let mut presenter = data.borrow_mut();
        if index == DIAG_INDEX {
            presenter.group = SettingsGroup::Appearance;
            if let Some(shell) = weak.upgrade() {
                shell.set_settings_show_diag(true);
                refresh_shell(&shell, &presenter);
            }
            return;
        }
        presenter.group = group_from_index(index);
        if let Some(shell) = weak.upgrade() {
            shell.set_settings_show_diag(false);
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_toggled(move |key, value| {
        let mut presenter = data.borrow_mut();
        let group = presenter.group;
        let _ = presenter.apply(group, &key, SettingsValue::Bool(value));
        if let Some(shell) = weak.upgrade() {
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_edited(move |key, value| {
        let mut presenter = data.borrow_mut();
        let group = presenter.group;
        let _ = presenter.apply(group, &key, SettingsValue::Text(value.to_string()));
        if let Some(shell) = weak.upgrade() {
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_stepped(move |key, value| {
        let mut presenter = data.borrow_mut();
        let group = presenter.group;
        let _ = presenter.apply(group, &key, SettingsValue::Num(value as f64));
        if let Some(shell) = weak.upgrade() {
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_chosen(move |key, value| {
        let mut presenter = data.borrow_mut();
        let applied = if key == "providers_disabled" {
            presenter.apply(
                SettingsGroup::Lyrics,
                "providers_disabled",
                SettingsValue::Text(value.to_string()),
            )
        } else {
            let group = presenter.group;
            presenter.apply(group, &key, SettingsValue::Choice(value.to_string()))
        };
        let _ = applied;
        if let Some(shell) = weak.upgrade() {
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_cycled(move |key| {
        let mut presenter = data.borrow_mut();
        let group = presenter.group;
        let rows = settings_section(&presenter.settings, group, &presenter.profiles);
        if let Some(row) = rows.iter().find(|row| row.key == key.as_str()) {
            if !row.options.is_empty() {
                let position = row
                    .options
                    .iter()
                    .position(|option| *option == row.text_val)
                    .unwrap_or(0);
                let next = row.options[(position + 1) % row.options.len()].clone();
                let _ = presenter.apply(group, &key, SettingsValue::Choice(next));
            }
        }
        if let Some(shell) = weak.upgrade() {
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_secret_toggled(move |key| {
        let mut presenter = data.borrow_mut();
        presenter.toggle_secret(&key);
        if let Some(shell) = weak.upgrade() {
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_provider_moved(move |key, from, dy| {
        let mut presenter = data.borrow_mut();
        if key == "provider_order" {
            let count = presenter.settings.lyrics.provider_order.len() as f32;
            let to = ((from as f32 + (dy / 52.0).round()).max(0.0)).min((count - 1.0).max(0.0)) as usize;
            let _ = presenter.apply(
                SettingsGroup::Lyrics,
                "provider_order",
                SettingsValue::Move {
                    from: from.max(0) as usize,
                    to,
                },
            );
        }
        if let Some(shell) = weak.upgrade() {
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_provider_toggled(move |key, provider, enabled| {
        let mut presenter = data.borrow_mut();
        if key == "provider_order" {
            let mut disabled: Vec<String> =
                provider_names(&presenter.settings.lyrics.providers_disabled);
            if enabled {
                disabled.retain(|name| name.as_str() != provider.as_str());
            } else if !disabled.iter().any(|name| name.as_str() == provider.as_str()) {
                disabled.push(provider.to_string());
            }
            let _ = presenter.apply(
                SettingsGroup::Lyrics,
                "providers_disabled",
                SettingsValue::Text(disabled.join(", ")),
            );
        }
        if let Some(shell) = weak.upgrade() {
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_test(move |id| {
        let mut presenter = data.borrow_mut();
        presenter.run_test(&id);
        if let Some(shell) = weak.upgrade() {
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_login(move |id| {
        let mut presenter = data.borrow_mut();
        presenter.login_calls.push(id.to_string());
        if let Some(shell) = weak.upgrade() {
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_scan_folder(move || {
        let folder = weak
            .upgrade()
            .map(|shell| shell.get_settings_ob_folder().to_string())
            .unwrap_or_default();
        let mut presenter = data.borrow_mut();
        presenter.ob_folder = folder.clone();
        presenter.scan_folder(&folder);
        if let Some(shell) = weak.upgrade() {
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_po_check(move || {
        let mut presenter = data.borrow_mut();
        presenter.check_po();
        if let Some(shell) = weak.upgrade() {
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_onboarding_finished(move || {
        let mut presenter = data.borrow_mut();
        let _ = presenter.finish_onboarding();
        if let Some(shell) = weak.upgrade() {
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_diag_level_selected(move |level| {
        let mut presenter = data.borrow_mut();
        presenter.diag_level = level;
        if let Some(shell) = weak.upgrade() {
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    shell.on_settings_diag_export(move || {
        let mut presenter = data.borrow_mut();
        let _ = presenter.export_diag();
        if let Some(shell) = weak.upgrade() {
            refresh_shell(&shell, &presenter);
        }
    });

    let weak = shell.as_weak();
    let data = state.clone();
    let slot = lang_slot.clone();
    shell.on_settings_lang_picked(move |code| {
        let lang = if code.as_str() == "en" {
            madrigal_i18n::Lang::En
        } else {
            madrigal_i18n::Lang::Es
        };
        let mut presenter = data.borrow_mut();
        let _ = presenter.apply(
            SettingsGroup::Appearance,
            "language",
            SettingsValue::Choice(code.to_string()),
        );
        if let Some(shell) = weak.upgrade() {
            crate::set_shell_lang(&shell, &slot, lang);
            refresh_shell(&shell, &presenter);
        }
    });
}

pub fn latencies_from_marks(total_ms: f64, resolve_ms: f64) -> madrigal_player::latency::LatencyReport {
    use madrigal_player::latency::LatencyTracker;
    use std::time::{Duration, Instant};
    let base = Instant::now();
    let mut tracker = LatencyTracker::new(8);
    tracker.enqueue_at(1, base);
    tracker.resolved_at(1, base + Duration::from_secs_f64(resolve_ms / 1000.0));
    tracker.started_at(1, base + Duration::from_secs_f64(total_ms / 1000.0));
    tracker.stats().expect("one latency sample")
}

#[cfg(test)]
mod tests {
    use super::*;
    use madrigal_i18n::Lang;

    fn rows_of(group: SettingsGroup) -> Vec<SettingRow> {
        let settings = madrigal_settings::Settings::default();
        settings_section(&settings, group, &["fixture".to_owned()])
    }

    #[test]
    fn blocks_place_every_row_exactly_once() {
        for group in all_groups() {
            if group == SettingsGroup::Account {
                continue;
            }
            let mut declared: Vec<&str> = blocks(group)
                .iter()
                .flat_map(|block| block.keys.iter().copied())
                .collect();
            let mut actual: Vec<&str> = rows_of(group).iter().map(|row| row.key).collect();
            declared.sort_unstable();
            actual.sort_unstable();
            assert_eq!(declared, actual, "{group:?}");
        }
    }

    #[test]
    fn labels_options_and_sections_resolve() {
        for group in all_groups() {
            for row in rows_of(group) {
                assert!(madrigal_i18n::contains(row.label_key), "{}", row.label_key);
                if group == SettingsGroup::Account {
                    continue;
                }
                for label in &row.option_labels {
                    assert!(madrigal_i18n::contains(label), "{label}");
                }
                if !row.section.is_empty() {
                    assert!(madrigal_i18n::contains(&row.section), "{}", row.section);
                }
            }
        }
    }

    #[test]
    fn choices_are_localized_not_raw_values() {
        for group in all_groups() {
            if group == SettingsGroup::Account {
                continue;
            }
            for row in rows_of(group) {
                for (option, label) in row.options.iter().zip(&row.option_labels) {
                    assert_ne!(madrigal_i18n::tr(Lang::Es, label), option.as_str(), "{}", row.key);
                    assert_ne!(madrigal_i18n::tr(Lang::En, label), option.as_str(), "{}", row.key);
                }
            }
        }
    }

    #[test]
    fn advanced_and_hidden_rows_stay_out_of_everyday_groups() {
        let player = rows_of(SettingsGroup::Player);
        let by_key = |key: &str| player.iter().find(|row| row.key == key).unwrap();
        assert!(!by_key("volume").advanced);
        assert!(!by_key("equalizer_band_0").advanced);
        assert!(!by_key("equalizer_selected_profile_id").advanced);
        assert!(by_key("equalizer_control_mode").advanced);
        assert!(!by_key("equalizer_control_mode").hidden);
        assert!(by_key("equalizer_custom_profiles_json").hidden);
        let updates = rows_of(SettingsGroup::Updates);
        assert!(updates.iter().find(|row| row.key == "launch_count").unwrap().hidden);
    }

    #[test]
    fn slider_text_shows_real_units() {
        assert_eq!(slider_text(Unit::Ratio, 1.0, Lang::Es), "1,0×");
        assert_eq!(slider_text(Unit::Ratio, 1.25, Lang::En), "1.25×");
        assert_eq!(slider_text(Unit::Semitones, 0.0, Lang::Es), "0 semitonos");
        assert_eq!(slider_text(Unit::Semitones, -1.0, Lang::Es), "−1 semitono");
        assert_eq!(slider_text(Unit::Semitones, 3.0, Lang::En), "+3 semitones");
        assert_eq!(slider_text(Unit::Decibels, 150.0, Lang::Es), "+1,5 dB");
        assert_eq!(slider_text(Unit::Decibels, -1500.0, Lang::En), "−15.0 dB");
        assert_eq!(slider_text(Unit::Db, 5.0, Lang::Es), "+5,0 dB");
        assert_eq!(slider_text(Unit::Db, -3.0, Lang::En), "−3.0 dB");
        assert_eq!(slider_text(Unit::Db, 0.0, Lang::Es), "0,0 dB");
        assert_eq!(slider_text(Unit::Percent, 130.0, Lang::Es), "130 %");
        assert_eq!(slider_text(Unit::Megabytes, 4096.0, Lang::Es), "4,0 GB");
        assert_eq!(slider_text(Unit::Megabytes, 512.0, Lang::En), "512 MB");
        assert_eq!(slider_text(Unit::Seconds, 3.0, Lang::Es), "3 s");
        assert_eq!(slider_text(Unit::Seconds, 2.5, Lang::Es), "2,5 s");
    }

    #[test]
    fn eq_bands_map_named_rows_to_stored_gains() {
        let rows = rows_of(SettingsGroup::Player);
        for (index, key) in EQ_BAND_KEYS.iter().enumerate() {
            let row = rows.iter().find(|row| row.key == *key).unwrap();
            assert_eq!(row.num_min, -12.0);
            assert_eq!(row.num_max, 12.0);
            assert_eq!(row.unit, Unit::Db);
            assert!(madrigal_i18n::contains(row.label_key), "{}", row.label_key);
            let _ = index;
        }
        let mut settings = madrigal_settings::Settings::default();
        apply_value(
            &mut settings,
            SettingsGroup::Player,
            "equalizer_band_3",
            &SettingsValue::Num(-4.5),
        )
        .expect("band writes");
        assert_eq!(settings.player.equalizer_gains[3], -4.5);
        assert_eq!(settings.player.equalizer_gains[0], 0.0);
        apply_value(
            &mut settings,
            SettingsGroup::Player,
            "equalizer_band_3",
            &SettingsValue::Num(99.0),
        )
        .expect("band clamps");
        assert_eq!(settings.player.equalizer_gains[3], 12.0);
    }

    #[test]
    fn eq_preset_choice_applies_stored_gains() {
        let mut settings = madrigal_settings::Settings::default();
        apply_value(
            &mut settings,
            SettingsGroup::Player,
            "equalizer_selected_profile_id",
            &SettingsValue::Choice("bass".to_owned()),
        )
        .expect("preset writes");
        assert_eq!(settings.player.equalizer_selected_profile_id, "bass");
        assert_eq!(
            settings.player.equalizer_gains,
            crate::eq_preset_gains("bass").unwrap().to_vec()
        );
        apply_value(
            &mut settings,
            SettingsGroup::Player,
            "equalizer_band_0",
            &SettingsValue::Num(2.0),
        )
        .expect("manual band clears preset");
        assert!(settings.player.equalizer_selected_profile_id.is_empty());
        assert_eq!(settings.player.equalizer_gains[0], 2.0);
    }
}
