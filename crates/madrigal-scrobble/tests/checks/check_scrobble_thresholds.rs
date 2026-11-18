use madrigal_core::{Event, Track, TrackIdentity};
use madrigal_scrobble::Eligibility;
use madrigal_settings::Integrations;

fn track_with_duration(duration: Option<u32>) -> Track {
    Track {
        identity: TrackIdentity::youtube("dQw4w9WgXcQ"),
        title: "Never Gonna Give You Up".to_owned(),
        artists: vec!["Rick Astley".to_owned()],
        album: Some("Whenever You Need Somebody".to_owned()),
        duration_secs: duration,
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

fn event_with_duration(duration: Option<u32>) -> Event {
    Event {
        track: TrackIdentity::youtube("dQw4w9WgXcQ"),
        played_at: 1_700_000_000,
        duration_secs: duration,
        completed: true,
    }
}

#[test]
fn oracle_threshold_table_with_boundaries() {
    let rules = Eligibility::default();
    assert_eq!(rules.delay_fraction, 0.5);
    assert_eq!(rules.delay_secs, 180);
    assert_eq!(rules.min_duration_secs, 30);

    assert_eq!(rules.threshold_secs(0), None);
    assert_eq!(rules.threshold_secs(29), None);
    assert_eq!(rules.threshold_secs(30), None);
    assert_eq!(rules.threshold_secs(31), Some(15));
    assert_eq!(rules.threshold_secs(200), Some(100));
    assert_eq!(rules.threshold_secs(360), Some(180));
    assert_eq!(rules.threshold_secs(400), Some(180));
    assert_eq!(rules.threshold_secs(720), Some(180));

    assert!(!rules.eligible(30, 30));
    assert!(!rules.eligible(200, 99));
    assert!(rules.eligible(200, 100));
    assert!(!rules.eligible(360, 179));
    assert!(rules.eligible(360, 180));
    assert!(rules.eligible(400, 180));
    assert!(!rules.eligible(31, 15));
    assert!(rules.eligible(31, 16));
    assert!(!rules.eligible(0, 0));

    let duration = |secs| track_with_duration(secs).duration_secs.unwrap_or(0) as u64;
    assert!(!rules.eligible(duration(Some(30)), 30));
    assert!(rules.eligible(duration(Some(212)), 106));
    assert!(!rules.eligible(duration(Some(212)), 105));
}

#[test]
fn event_threshold_mirrors_duration_rule() {
    let rules = Eligibility::default();
    assert_eq!(rules.event_threshold(&event_with_duration(None)), None);
    assert_eq!(rules.event_threshold(&event_with_duration(Some(30))), None);
    assert_eq!(
        rules.event_threshold(&event_with_duration(Some(200))),
        Some(100)
    );
    assert_eq!(
        rules.event_threshold(&event_with_duration(Some(500))),
        Some(180)
    );
}

#[test]
fn custom_config_and_settings_mapping() {
    let custom = Eligibility {
        delay_fraction: 1.0,
        delay_secs: 180,
        min_duration_secs: 30,
    };
    assert_eq!(custom.threshold_secs(100), Some(100));
    assert!(custom.eligible(100, 100));
    assert!(!custom.eligible(100, 99));

    let from_settings = Eligibility::from_integrations(&Integrations {
        scrobble_delay_percent: 25.0,
        scrobble_min_song_duration_secs: 60,
        scrobble_delay_secs: 120,
        ..Integrations::default()
    });
    assert_eq!(from_settings.delay_fraction, 0.25);
    assert_eq!(from_settings.min_duration_secs, 60);
    assert_eq!(from_settings.delay_secs, 120);
    assert_eq!(from_settings.threshold_secs(60), None);
    assert_eq!(from_settings.threshold_secs(400), Some(100));
    assert_eq!(from_settings.threshold_secs(800), Some(120));

    let defaults = Eligibility::from_integrations(&Integrations::default());
    assert_eq!(defaults.delay_fraction, 0.5);
    assert_eq!(defaults.min_duration_secs, 30);
}
