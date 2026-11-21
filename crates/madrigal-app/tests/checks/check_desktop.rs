use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use madrigal_app::desktop::{
    self, Action, Handoff, PlayerState, SessionKeeper, TrackWatch, SESSION_INTERVAL,
};
use madrigal_app::playback::track_from_file;
use madrigal_core::Track;
use madrigal_mpris::{MockProvider, MockState};
use madrigal_player::session;
use madrigal_scrobble::outbox::Outbox;
use madrigal_scrobble::{Eligibility, SubmitOutcome};
use madrigal_settings::{RepeatMode, Settings};

static ENV: Mutex<()> = Mutex::new(());

struct SilentBus {
    _dir: tempfile::TempDir,
    address: String,
}

impl SilentBus {
    fn start() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bus.sock");
        let listener = UnixListener::bind(&path).expect("bind silent bus");
        thread::spawn(move || {
            let mut held = Vec::new();
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                held.push(stream);
            }
        });
        Self {
            address: format!("unix:path={}", path.display()),
            _dir: dir,
        }
    }
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .join("madrigal-lyrics/tests/fixtures/embedded_uslt.mp3")
}

fn track() -> Track {
    let (identity, meta) = track_from_file(&fixture()).expect("track from fixture");
    let artists = meta
        .artist
        .split(", ")
        .filter(|artist| !artist.is_empty())
        .map(str::to_owned)
        .collect();
    Track {
        identity,
        title: meta.title,
        artists,
        album: None,
        duration_secs: meta.duration_secs,
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

#[test]
fn player_state_persists() {
    let _guard = ENV.lock().expect("env lock");
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, root.path());
    std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", root.path().join("cache"));
    let path = madrigal_paths::settings_path();

    let mut settings = Settings::default();
    settings
        .extra
        .insert("future-key".to_owned(), serde_json::json!({"kept": true}));
    settings.save_to(&path).expect("seed settings");

    let state = PlayerState {
        volume: 42,
        repeat: RepeatMode::One,
    };
    assert!(desktop::persist_player_state(&path, state).expect("persist"));
    let disk = Settings::load_from(&path);
    assert_eq!(disk.player.volume, 42);
    assert_eq!(disk.player.repeat_mode, RepeatMode::One);
    assert!(!disk.player.permanent_shuffle);
    assert_eq!(disk.extra["future-key"]["kept"], serde_json::json!(true));

    assert!(
        !desktop::persist_player_state(&path, state).expect("persist again"),
        "unchanged state rewritten"
    );
    let other = PlayerState {
        volume: 7,
        repeat: RepeatMode::Off,
    };
    assert!(desktop::persist_player_state(&path, other).expect("persist other"));
    let disk = Settings::load_from(&path);
    assert_eq!(disk.player.volume, 7);
    assert_eq!(disk.player.repeat_mode, RepeatMode::Off);

    let keeper = SessionKeeper::new(root.path().join("player-session.json"));
    assert!(keeper.due(Instant::now()), "first save is due");
    println!("OK settings persist");
}

#[test]
fn session_keeper_saves_changed_state() {
    let path = tempfile::tempdir().expect("tempdir").path().join("session.json");
    let mut keeper = SessionKeeper::new(path.clone());
    let fixture_path = fixture();
    let state = session::SessionState {
        tracks: vec![madrigal_core::TrackIdentity::local(
            fixture_path.to_string_lossy().into_owned(),
        )],
        position: Some(0),
        paused: true,
    };

    let start = Instant::now();
    assert!(keeper.tick(state.clone(), start), "first tick saves");
    assert_eq!(session::load_from(&path).expect("load"), Some(state.clone()));
    assert_eq!(keeper.last(), Some(&state));

    assert!(
        !keeper.tick(state.clone(), start + Duration::from_secs(1)),
        "unchanged state saved again"
    );
    let mut changed = state.clone();
    changed.paused = false;
    assert!(
        !keeper.tick(changed.clone(), start + Duration::from_secs(1)),
        "interval not elapsed"
    );
    assert!(
        keeper.tick(changed.clone(), start + SESSION_INTERVAL),
        "changed state skipped"
    );
    assert_eq!(session::load_from(&path).expect("load"), Some(changed));

    keeper.save(&state).expect("save");
    assert_eq!(session::load_from(&path).expect("load"), Some(state));
    println!("OK session keeper");
}

#[test]
fn handoff_forwards_files() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = root.path().join(desktop::HANDOFF_SOCKET);
    let (tx, rx) = mpsc::channel();
    let handoff = Handoff::start(path.clone(), move |files| {
        let _ = tx.send(files.to_vec());
    })
    .expect("start handoff");
    assert_eq!(handoff.path(), path);

    let files = vec![
        fixture().to_string_lossy().into_owned(),
        "second.flac".to_owned(),
    ];
    assert_eq!(desktop::forward_files(&path, &files).expect("forward"), 2);
    let received = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("forwarded files");
    assert_eq!(received, files);
    assert_eq!(desktop::forward_files(&path, &[]).expect("empty"), 0);

    drop(handoff);
    assert!(!path.exists(), "socket left behind");
    assert!(desktop::forward_files(&path, &files).is_err());
    println!("OK handoff");
}

#[test]
fn notification_actions_map() {
    assert_eq!(
        desktop::action_from_notification("previous"),
        Some(Action::Previous)
    );
    assert_eq!(
        desktop::action_from_notification("toggle"),
        Some(Action::PlayPause)
    );
    assert_eq!(
        desktop::action_from_notification("next"),
        Some(Action::Next)
    );
    assert_eq!(desktop::action_from_notification("default"), None);
    assert_eq!(desktop::action_from_notification("bogus"), None);
    println!("OK notification actions");
}

#[test]
fn scrobble_observe_and_merge() {
    let root = tempfile::tempdir().expect("tempdir");
    let mut outbox = Outbox::open(&root.path().join("outbox.json")).expect("outbox");
    let mut watch = TrackWatch::default();
    let lenient = Eligibility {
        delay_fraction: 0.5,
        delay_secs: 180,
        min_duration_secs: 0,
    };
    let track = track();
    let duration = u64::from(track.duration_secs.expect("duration"));
    let threshold = lenient.threshold_secs(duration).expect("threshold");
    let mut now = Instant::now();

    assert!(
        !desktop::observe_track(
            &mut outbox,
            &mut watch,
            &lenient,
            &track,
            false,
            now,
            1_700_000_000
        ),
        "paused track scrobbled"
    );
    now += Duration::from_secs(600);
    assert!(!desktop::observe_track(
        &mut outbox,
        &mut watch,
        &lenient,
        &track,
        false,
        now,
        1_700_000_600
    ));
    assert_eq!(watch.played(), Duration::ZERO, "paused time counted");
    assert!(outbox.is_empty());

    let mut scrobbled_at = None;
    for tick in 1..=threshold + 4 {
        now += Duration::from_secs(1);
        if desktop::observe_track(
            &mut outbox,
            &mut watch,
            &lenient,
            &track,
            true,
            now,
            1_700_000_600 + tick as i64,
        ) {
            scrobbled_at = Some(tick);
            break;
        }
    }
    let tick = scrobbled_at.expect("scrobbled after threshold");
    assert!(
        (threshold..=threshold + 1).contains(&tick),
        "scrobbled at {tick}s, threshold {threshold}s"
    );
    assert_eq!(outbox.len(), 1);
    let entry = outbox.entries().pop().expect("entry");
    assert_eq!(entry.track_id, track.identity.id);
    assert_eq!(entry.title, track.title);
    assert_eq!(entry.started_at, 1_700_000_000);

    now += Duration::from_secs(1);
    assert!(
        !desktop::observe_track(
            &mut outbox,
            &mut watch,
            &lenient,
            &track,
            true,
            now,
            1_700_000_700
        ),
        "same track scrobbled twice"
    );
    assert_eq!(outbox.len(), 1);

    assert_eq!(
        desktop::combine_outcomes(&[SubmitOutcome::Submitted, SubmitOutcome::Permanent]),
        SubmitOutcome::Submitted
    );
    assert_eq!(
        desktop::combine_outcomes(&[SubmitOutcome::Permanent, SubmitOutcome::Retryable]),
        SubmitOutcome::Retryable
    );
    assert_eq!(
        desktop::combine_outcomes(&[SubmitOutcome::Permanent]),
        SubmitOutcome::Permanent
    );
    assert_eq!(
        desktop::combine_outcomes(&[]),
        SubmitOutcome::Retryable
    );
    println!("OK scrobble");
}

#[test]
fn dbus_services_skip_without_session_bus() {
    let _guard = ENV.lock().expect("env lock");
    std::env::remove_var(desktop::BUS_ADDRESS_ENV);
    std::env::remove_var("DBUS_SESSION_BUS_ADDRESS");
    assert_eq!(desktop::bus_address(), None);

    let labels = desktop::Labels::from_settings(&Settings::default());

    std::env::set_var(desktop::BUS_ADDRESS_ENV, "");
    assert_eq!(desktop::bus_address(), None, "empty override disables dbus");

    let bogus = "unix:path=/nonexistent/madrigal-session.sock";
    std::env::set_var(desktop::BUS_ADDRESS_ENV, bogus);
    assert_eq!(desktop::bus_address().as_deref(), Some(bogus));
    let (tx, _rx) = mpsc::channel();
    let started = Instant::now();
    let bridge = desktop::start_notifications(Some(bogus), labels, tx);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "boot waited on an unreachable bus: {:?}",
        started.elapsed()
    );
    let started = Instant::now();
    drop(bridge);
    assert!(
        started.elapsed() < desktop::DBUS_TIMEOUT + Duration::from_secs(2),
        "unreachable bus delayed shutdown: {:?}",
        started.elapsed()
    );
    std::env::remove_var(desktop::BUS_ADDRESS_ENV);
    println!("OK dbus skip");
}

#[test]
fn dbus_setup_never_blocks_boot() {
    let _guard = ENV.lock().expect("env lock");
    let bus = SilentBus::start();
    std::env::set_var(desktop::BUS_ADDRESS_ENV, &bus.address);
    let address = desktop::bus_address().expect("bus address");
    let labels = desktop::Labels::from_settings(&Settings::default());

    let started = Instant::now();
    let (tx, _rx) = mpsc::channel();
    let notify = desktop::start_notifications(Some(&address), labels, tx);
    let mpris = desktop::start_mpris(Arc::new(MockProvider::new(MockState::default())), &address);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "boot waited on dbus: {:?}",
        started.elapsed()
    );

    let started = Instant::now();
    drop(notify);
    assert!(
        started.elapsed() < desktop::DBUS_TIMEOUT + Duration::from_secs(2),
        "notification shutdown waited on a hung bus: {:?}",
        started.elapsed()
    );
    let started = Instant::now();
    drop(mpris);
    assert!(
        started.elapsed() < desktop::DBUS_TIMEOUT + Duration::from_secs(2),
        "mpris shutdown waited on a hung bus: {:?}",
        started.elapsed()
    );

    std::env::remove_var(desktop::BUS_ADDRESS_ENV);
    println!("OK dbus setup bounded");
}
