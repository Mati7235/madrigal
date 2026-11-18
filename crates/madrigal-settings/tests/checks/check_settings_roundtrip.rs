use std::env;

use madrigal_settings::{
    AudioQuality, Language, LyricsProvider, Settings, SponsorCategory, Theme, UpdateChannel,
    set_active_account_in,
};

fn isolate_profile() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    env::remove_var("XDG_CONFIG_HOME");
    env::remove_var("XDG_DATA_HOME");
    env::remove_var("XDG_CACHE_HOME");
    root
}

#[test]
fn settings_roundtrip() {
    let _root = isolate_profile();
    let path = madrigal_paths::settings_path();

    let defaults = Settings::load();
    assert_eq!(defaults, Settings::default());
    assert_eq!(defaults.player.volume, 70);
    assert_eq!(defaults.account.active_account_id, "default");
    assert!(!path.exists());

    let mut edited = Settings::default();
    edited.appearance.theme = Theme::Dark;
    edited.appearance.language = Language::En;
    edited.player.audio_quality = AudioQuality::Medium;
    edited.player.volume = 42;
    edited.lyrics.preferred_provider = Some(LyricsProvider::Lrclib);
    edited.integrations.sponsorblock_categories =
        vec![SponsorCategory::Sponsor, SponsorCategory::Hook];
    edited.updates.update_channel = UpdateChannel::Artifact;
    edited.save_to(&path).expect("save");
    let back = Settings::load_from(&path);
    assert_eq!(back, edited);

    let hostile = serde_json::json!({
        "appearance": {
            "theme": "neon",
            "language": "fr",
            "thumbnail_corner_radius": 500.0,
            "backdrop_blur_amount": -20,
            "swipe_sensitivity": 99.0,
        },
        "player": {
            "volume": 250,
            "audio_quality": "ultra",
            "playback_speed": 9.0,
            "pitch_semitones": -30.0,
            "equalizer_gains": [0.0, 1.0],
            "normalization": "loud",
            "transition_mode": "warp",
            "crossfade_seconds": -5.0,
        },
        "lyrics": {
            "font_size": 3.0,
            "provider_order": ["lrclib", "nope", "lrclib"],
            "providers_disabled": ["kugou", "nope"],
            "translation_endpoint": "   ",
            "translation_target": "",
        },
        "content": {"local_songs_min_duration_secs": -10},
        "storage": {"download_concurrency": 99},
        "privacy": {"history_duration_days": 1000},
        "account": {"active_account_id": "../escape"},
        "integrations": {
            "sponsorblock_categories": ["sponsor", "nope"],
            "sponsorblock_api_url": "",
            "scrobble_delay_percent": 500.0,
        },
        "updates": {"update_channel": "NIGHTLY", "launch_count": -4},
        "future_key": {"nested": true},
    });
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&hostile).expect("json"),
    )
    .expect("write hostile");
    let clean = Settings::load_from(&path);
    let fresh = Settings::default();
    assert_eq!(clean.appearance.theme, fresh.appearance.theme);
    assert_eq!(clean.appearance.language, fresh.appearance.language);
    assert_eq!(clean.player.volume, 130);
    assert_eq!(clean.player.audio_quality, AudioQuality::Best);
    assert_eq!(clean.player.playback_speed, 2.0);
    assert_eq!(clean.player.pitch_semitones, -12.0);
    assert_eq!(clean.player.equalizer_gains, vec![0.0; 10]);
    assert_eq!(clean.player.crossfade_seconds, 0.0);
    assert_eq!(clean.lyrics.font_size, 10.0);
    assert_eq!(
        clean.lyrics.provider_order.first(),
        Some(&LyricsProvider::Lrclib)
    );
    assert_eq!(clean.lyrics.provider_order.len(), 11);
    assert_eq!(
        clean.lyrics.providers_disabled,
        vec![LyricsProvider::Kugou]
    );
    assert_eq!(
        clean.lyrics.translation_endpoint,
        madrigal_settings::TRANSLATION_DEFAULT_ENDPOINT
    );
    assert_eq!(clean.lyrics.translation_target, "es");
    assert_eq!(clean.storage.download_concurrency, 6);
    assert_eq!(clean.privacy.history_duration_days, 60);
    assert_eq!(clean.account.active_account_id, "default");
    assert_eq!(
        clean.integrations.sponsorblock_categories,
        vec![SponsorCategory::Sponsor]
    );
    assert_eq!(
        clean.integrations.sponsorblock_api_url,
        madrigal_settings::SPONSORBLOCK_DEFAULT_API_URL
    );
    assert_eq!(clean.updates.update_channel, UpdateChannel::Artifact);
    assert_eq!(clean.updates.launch_count, 0);

    edited.save_to(&path).expect("save edited");
    let kept = Settings::load_from(&path);
    assert_eq!(kept, edited);

    clean.save_to(&path).expect("save sanitized");
    let kept = Settings::load_from(&path);
    assert_eq!(
        kept.extra.get("future_key"),
        Some(&serde_json::json!({"nested": true}))
    );

    set_active_account_in(&path, "work").expect("switch account");
    let switched = Settings::load_from(&path);
    assert_eq!(switched.account.active_account_id, "work");
    assert_eq!(switched.player.volume, clean.player.volume);
    assert_eq!(switched.appearance.theme, clean.appearance.theme);
    assert!(set_active_account_in(&path, "../escape").is_err());

    let other = madrigal_paths::profile_account_path("work").expect("account path");
    assert!(other.starts_with(
        std::path::Path::new(&env::var("MADRIGAL_PROFILE_ROOT").expect("root"))
    ));
    let second = madrigal_paths::profile_account_path("personal").expect("account path");
    assert_ne!(other, second);
}
