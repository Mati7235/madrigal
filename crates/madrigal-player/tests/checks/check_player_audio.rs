use std::time::{Duration, Instant};

use madrigal_player::audio::{
    ErrorSkipper, VolumeAction, ZeroVolumePause, build_af, custom_presets, equalizer_af,
    equalizer_segments, export_presets, headroom_db, import_presets, pitch_multiplier, preset_gains,
    resolve_gains, tempo_af, LOUDNORM_AF, MAX_AUTO_SKIPS, PRESET_BASS, PRESET_FLAT, PRESET_TREBLE,
    PRESET_VOCAL, SKIP_SILENCE_AF,
};
use madrigal_player::sleep::{SleepMode, SleepTimer};
use madrigal_settings::EQ_BANDS;
use madrigal_settings::{Normalization, Player as PlayerConfig};

#[test]
fn eq_bands_match_oracle_set() {
    assert_eq!(
        EQ_BANDS,
        [31.5, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0]
    );
}

#[test]
fn eq_segments_carry_each_band_gain() {
    let gains = [0.0, 3.0, 0.0, 0.0, 0.0, -2.0, 0.0, 0.0, 0.0, 0.0];
    let segments = equalizer_segments(&gains);
    assert_eq!(segments.len(), 2);
    assert!(segments[0].starts_with("equalizer=f=63:"));
    assert!(segments[0].ends_with("g=3"));
    assert!(segments[1].starts_with("equalizer=f=1000:"));
    assert!(segments[1].ends_with("g=-2"));
    assert!(segments.iter().all(|s| s.contains("width_type=o:width=1")));
}

#[test]
fn eq_headroom_and_chain_shape() {
    assert_eq!(headroom_db(&PRESET_FLAT), 0.0);
    assert_eq!(headroom_db(&PRESET_BASS), 5.0);
    assert_eq!(headroom_db(&PRESET_TREBLE), 5.0);
    assert_eq!(equalizer_af(&PRESET_FLAT), None);
    let chain = equalizer_af(&PRESET_BASS).expect("nonzero gains");
    assert!(chain.starts_with("volume=volume=-5dB,"));
    assert_eq!(chain.matches("equalizer=").count(), 8);
    let chain = equalizer_af(&PRESET_VOCAL).expect("nonzero gains");
    assert!(chain.starts_with("volume=volume=-4dB,"));
    let negative = [-1.0; 10];
    let chain = equalizer_af(&negative).expect("nonzero gains");
    assert!(!chain.contains("volume="));
    assert_eq!(chain.matches("equalizer=").count(), 10);
}

#[test]
fn builtin_presets_match_oracle_vectors() {
    assert_eq!(preset_gains("flat"), Some(PRESET_FLAT));
    assert_eq!(preset_gains("bass"), Some(PRESET_BASS));
    assert_eq!(preset_gains("treble"), Some(PRESET_TREBLE));
    assert_eq!(preset_gains("vocal"), Some(PRESET_VOCAL));
    assert_eq!(preset_gains("unknown"), None);
    assert_eq!(PRESET_BASS, [5.0, 5.0, 4.0, 3.0, 1.0, 0.0, 0.0, -1.0, -1.0, -1.0]);
    assert_eq!(PRESET_TREBLE, [-2.0, -2.0, -1.0, -1.0, 0.0, 1.0, 2.0, 4.0, 5.0, 5.0]);
    assert_eq!(PRESET_VOCAL, [-3.0, -3.0, -2.0, -1.0, 2.0, 4.0, 3.0, 2.0, 0.0, -1.0]);
}

#[test]
fn preset_import_export_roundtrip() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = root.path().join("presets.json");
    let mut presets = std::collections::BTreeMap::new();
    presets.insert("bass".to_owned(), PRESET_BASS);
    presets.insert("mine".to_owned(), [1.0; 10]);
    export_presets(&path, &presets).expect("export");
    assert!(path.is_file());
    let back = import_presets(&path).expect("import");
    assert_eq!(back, presets);
    std::fs::write(&path, r#"{"loud": [99.0, 0, 0, 0, 0, 0, 0, 0, 0, 0]}"#).expect("write");
    let back = import_presets(&path).expect("import");
    assert_eq!(back["loud"][0], 12.0);
}

#[test]
fn custom_presets_come_from_settings() {
    let mut config = PlayerConfig::default();
    assert!(custom_presets(&config).is_empty());
    config.equalizer_custom_profiles_json =
        r#"{"mine": [1, 1, 1, 1, 1, 1, 1, 1, 1, 1]}"#.to_owned();
    assert_eq!(resolve_gains(&config, "mine"), Some([1.0; 10]));
    assert_eq!(resolve_gains(&config, "bass"), Some(PRESET_BASS));
    assert_eq!(resolve_gains(&config, "missing"), None);
}

#[test]
fn tempo_maps_semitones_to_multiplier() {
    assert!((pitch_multiplier(0.0) - 1.0).abs() < 1e-9);
    assert!((pitch_multiplier(12.0) - 2.0).abs() < 1e-9);
    assert!((pitch_multiplier(-12.0) - 0.5).abs() < 1e-9);
    assert!((pitch_multiplier(1.0) - 1.0594631).abs() < 1e-6);
    assert_eq!(tempo_af(1.0, 0.0), None);
    assert_eq!(tempo_af(1.25, 0.0), Some("scaletempo2".to_owned()));
    let shifted = tempo_af(1.0, 1.0).expect("pitch filter");
    assert!(shifted.starts_with("rubberband=pitch-scale=1.059463"));
    let both = tempo_af(1.25, -12.0).expect("both");
    assert!(both.contains("scaletempo2"));
    assert!(both.contains("rubberband=pitch-scale=0.5"));
}

#[test]
fn build_af_composes_enabled_sections() {
    let config = PlayerConfig::default();
    assert_eq!(build_af(&config), "");
    let config = PlayerConfig {
        equalizer_enabled: true,
        equalizer_gains: PRESET_BASS.to_vec(),
        normalization: Normalization::Dynamic,
        skip_silence: true,
        playback_speed: 1.25,
        pitch_semitones: 1.0,
        ..PlayerConfig::default()
    };
    let chain = build_af(&config);
    assert!(chain.contains("equalizer=f=63:"));
    assert!(chain.contains(LOUDNORM_AF));
    assert!(chain.contains(SKIP_SILENCE_AF));
    assert!(chain.contains("scaletempo2"));
    assert!(chain.contains("rubberband=pitch-scale="));
}

#[test]
fn sleep_minutes_fires_on_fixed_clock() {
    let start = Instant::now();
    let mut timer = SleepTimer::minutes(1);
    assert!(!timer.is_armed());
    timer.arm(start);
    assert!(timer.is_armed());
    assert!(!timer.poll(start + Duration::from_secs(59)));
    assert!(timer.is_armed());
    assert_eq!(timer.remaining(start), Some(Duration::from_secs(60)));
    assert!(timer.poll(start + Duration::from_secs(60)));
    assert!(!timer.is_armed());
    assert!(!timer.poll(start + Duration::from_secs(61)));
    assert_eq!(timer.remaining(start), None);
}

#[test]
fn sleep_end_of_song_fires_once_on_event() {
    let start = Instant::now();
    let mut timer = SleepTimer::end_of_song();
    timer.arm(start);
    assert_eq!(timer.mode(), SleepMode::EndOfSong);
    assert!(!timer.poll(start + Duration::from_secs(3600)));
    assert_eq!(timer.remaining(start), None);
    assert!(timer.on_track_end());
    assert!(!timer.is_armed());
    assert!(!timer.on_track_end());
    timer.cancel();
    assert!(!timer.is_armed());
}

#[test]
fn error_skips_bound_and_reset() {
    let mut skips = ErrorSkipper::new(MAX_AUTO_SKIPS);
    for _ in 0..MAX_AUTO_SKIPS {
        assert!(skips.on_error());
    }
    assert!(!skips.on_error());
    assert_eq!(skips.consecutive(), MAX_AUTO_SKIPS);
    skips.on_success();
    assert_eq!(skips.consecutive(), 0);
    assert!(skips.on_error());
}

#[test]
fn zero_volume_state_machine() {
    let mut zero = ZeroVolumePause::new();
    assert_eq!(zero.set_volume(70, 0), VolumeAction::Pause);
    assert!(zero.is_engaged());
    assert_eq!(zero.set_volume(0, 0), VolumeAction::Keep);
    assert_eq!(zero.set_volume(0, 30), VolumeAction::Resume);
    assert!(!zero.is_engaged());
    assert_eq!(zero.set_volume(30, 40), VolumeAction::Keep);
    let mut zero = ZeroVolumePause::new();
    assert_eq!(zero.set_volume(70, 0), VolumeAction::Pause);
    assert_eq!(zero.resume_target(0, 50), Some(70));
    assert!(!zero.is_engaged());
    let mut zero = ZeroVolumePause::new();
    assert_eq!(zero.resume_target(40, 50), None);
    let mut zero = ZeroVolumePause::new();
    assert_eq!(zero.set_volume(0, 0), VolumeAction::Pause);
    assert_eq!(zero.resume_target(0, 50), Some(50));
}
