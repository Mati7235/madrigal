use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use madrigal_app::desktop::{Desktop, ERROR_CODE};
use madrigal_app::playback::{track_from_file, Playback, PlaybackService};
use madrigal_app::state::AppState;
use madrigal_player::engine::EngineConfig;
use madrigal_settings::{Player as PlayerConfig, RepeatMode, Settings};
use madrigal_testkit as kit;
use madrigal_ui::MainShell;

fn have_ffmpeg() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|out| out.status.success())
}

fn have_libmpv() -> bool {
    if PathBuf::from("/usr/lib/libmpv.so").exists() {
        return true;
    }
    Command::new("ldconfig")
        .arg("-p")
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("libmpv.so"))
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .join("madrigal-lyrics/tests/fixtures/embedded_uslt.mp3")
}

fn make_wav(dir: &Path, name: &str) -> PathBuf {
    let out = dir.join(name);
    let status = Command::new("ffmpeg")
        .args(["-y", "-stream_loop", "7", "-i"])
        .arg(fixture())
        .args(["-t", "8", "-ar", "44100", "-ac", "2", "-c:a", "pcm_s16le"])
        .arg(&out)
        .output()
        .expect("run ffmpeg");
    assert!(status.status.success(), "ffmpeg: {status:?}");
    assert!(out.metadata().expect("stat").len() > 44100);
    out
}

fn wait_playing(playback: &mut Playback, floor_ms: i32) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        playback.poll(Instant::now());
        if playback.position_ms() > floor_ms {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("mpv no avanzó la posición (floor {floor_ms} ms)");
}

#[test]
fn app_playback() {
    if !have_ffmpeg() || !have_libmpv() {
        println!("SKIP app_playback native (missing ffmpeg or libmpv)");
        return;
    }
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, root.path());
    std::env::set_var(madrigal_app::desktop::BUS_ADDRESS_ENV, "");
    std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", root.path().join("cache"));

    let wav = make_wav(root.path(), "track.wav");
    let other = make_wav(root.path(), "other.wav");
    let config = EngineConfig {
        vo: "null".to_owned(),
        ao: "null".to_owned(),
        gapless_audio: false,
    };
    let settings = PlayerConfig::default();

    let (_, meta) = track_from_file(&wav).expect("track from wav");
    assert_eq!(meta.title, "track");
    assert!(
        meta.duration_secs
            .is_some_and(|secs| (7..=8).contains(&secs)),
        "duración {meta:?}"
    );

    let streams = madrigal_app::streams::orchestrator().expect("orchestrator");
    let mut playback = Playback::open(config.clone(), &settings, streams).expect("open playback");
    let (added, errors) = playback.enqueue_files(&[wav.clone(), other.clone()]);
    assert_eq!(added, 2);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(playback.has_track());

    wait_playing(&mut playback, 200);
    assert!(!playback.paused());
    let snapshot = playback.snapshot();
    assert_eq!(snapshot.title, "track");
    assert_eq!(snapshot.track_source, madrigal_core::TrackSource::Local);
    assert_eq!(snapshot.queue_index, 0);
    assert_eq!(snapshot.queue.all.len(), 2);
    assert!(
        snapshot.duration_ms >= 7000,
        "duration {} ms",
        snapshot.duration_ms
    );

    assert!(playback.next_track().expect("next"));
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3) {
        playback.poll(Instant::now());
        if playback.snapshot().title == "other" {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(playback.snapshot().title, "other");
    assert_eq!(playback.snapshot().queue_index, 1);
    assert!(playback.previous_track().expect("previous"));
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3) {
        playback.poll(Instant::now());
        if playback.snapshot().title == "track" {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(playback.snapshot().title, "track");
    wait_playing(&mut playback, 200);

    playback.seek_ms(4000).expect("seek");
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3)
        && !(3800..6000).contains(&playback.position_ms())
    {
        std::thread::sleep(Duration::from_millis(50));
    }
    let seeked = playback.position_ms();
    assert!(
        (3800..6000).contains(&seeked),
        "posición tras seek {seeked}"
    );

    playback.play_pause().expect("pause");
    assert!(playback.paused());
    let frozen = playback.position_ms();
    std::thread::sleep(Duration::from_millis(500));
    let still = playback.position_ms();
    assert!(
        (still - frozen).abs() < 200,
        "pausa no congela {frozen} -> {still}"
    );
    playback.play_pause().expect("resume");
    assert!(!playback.paused());

    playback.set_volume(0.25).expect("volume");
    assert!((playback.volume() - 0.25).abs() < 0.05);
    playback.set_repeat(RepeatMode::One);
    assert_eq!(playback.snapshot().repeat, RepeatMode::One);
    playback.set_shuffle(true);
    assert!(playback.snapshot().shuffle);
    drop(playback);

    kit::init_headless();
    let app = AppState::boot_with_engine(config).expect("boot app");
    let (added, errors) = app.open_files(&[
        wav.to_string_lossy().into_owned(),
        other.to_string_lossy().into_owned(),
    ]);
    assert_eq!(added, 2);
    assert!(errors.is_empty(), "{errors:?}");
    let shell = &app.ui.shell;

    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3) {
        kit::mock_advance(Duration::from_millis(100));
        std::thread::sleep(Duration::from_millis(20));
        if shell.get_has_track() && shell.get_position_ms() > 200 {
            break;
        }
    }
    assert!(shell.get_has_track(), "sin track en el shell");
    assert_eq!(shell.get_track_title().as_str(), "track");
    assert_eq!(shell.get_track_source(), 2);
    assert_eq!(shell.get_queue_count(), 2);
    assert!(shell.get_duration_ms() >= 7000);

    shell.invoke_next_requested();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        kit::mock_advance(Duration::from_millis(100));
        std::thread::sleep(Duration::from_millis(20));
        if shell.get_track_title().as_str() == "other" {
            break;
        }
    }
    assert_eq!(shell.get_track_title().as_str(), "other");
    shell.invoke_prev_requested();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        kit::mock_advance(Duration::from_millis(100));
        std::thread::sleep(Duration::from_millis(20));
        if shell.get_track_title().as_str() == "track" {
            break;
        }
    }
    assert_eq!(shell.get_track_title().as_str(), "track");

    shell.invoke_play_pause_toggled();
    assert!(shell.get_paused(), "pausa no reflejada");
    std::thread::sleep(Duration::from_millis(250));
    kit::mock_advance(Duration::from_millis(100));
    assert!(shell.get_paused());

    shell.invoke_play_pause_toggled();
    shell.set_full_player_open(true);
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        kit::mock_advance(Duration::from_millis(100));
        std::thread::sleep(Duration::from_millis(20));
        if !shell.get_paused() && shell.get_position_ms() > 200 {
            break;
        }
    }
    assert!(!shell.get_paused(), "resume no reflejado");
    let ticks = shell.get_lyrics_clock_ticks();
    for _ in 0..10 {
        kit::mock_advance(Duration::from_millis(20));
    }
    assert!(
        shell.get_lyrics_clock_ticks() > ticks,
        "lyrics clock no avanza"
    );
    shell.invoke_play_pause_toggled();
    assert!(shell.get_paused());
    let frozen_ticks = shell.get_lyrics_clock_ticks();
    for _ in 0..10 {
        kit::mock_advance(Duration::from_millis(20));
    }
    assert_eq!(
        shell.get_lyrics_clock_ticks(),
        frozen_ticks,
        "lyrics clock no se congela en pausa"
    );
    std::thread::sleep(Duration::from_millis(200));
    kit::mock_advance(Duration::from_millis(100));
    shell.invoke_play_pause_toggled();

    shell.invoke_seek_to_requested(4000);
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        kit::mock_advance(Duration::from_millis(100));
        std::thread::sleep(Duration::from_millis(20));
        if (3800..6000).contains(&shell.get_position_ms()) {
            break;
        }
    }
    let position = shell.get_position_ms();
    assert!(
        (3800..6000).contains(&position),
        "seek del shell {position}"
    );

    shell.invoke_volume_changed(0.25);
    assert!((shell.get_volume() - 0.25).abs() < 0.01);
    shell.invoke_repeat_toggled();
    assert_eq!(shell.get_repeat_mode(), 1);
    shell.invoke_shuffle_toggled();
    assert!(shell.get_shuffle());
    shell.invoke_next_requested();

    drop(app);

    let broken = EngineConfig {
        vo: "null".to_owned(),
        ao: "madrigal-missing-ao".to_owned(),
        gapless_audio: false,
    };
    let streams = madrigal_app::streams::orchestrator().expect("orchestrator");
    let playback =
        Playback::open(broken, &PlayerConfig::default(), streams).expect("open broken engine");
    let service = PlaybackService::start(playback);
    let shell = MainShell::new().expect("shell");
    let _desktop = Desktop::start(&shell, service.core(), &Settings::default());
    {
        let core = service.core();
        let mut playback = core.lock().expect("playback core");
        let (added, errors) = playback.enqueue_files(std::slice::from_ref(&wav));
        assert_eq!(added, 1);
        assert!(errors.is_empty(), "{errors:?}");
    }
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) && shell.get_error_code().is_empty() {
        kit::mock_advance(Duration::from_millis(250));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(shell.get_error_code().as_str(), ERROR_CODE);
    assert!(
        shell.get_error_message().contains("audio device"),
        "unexpected error: {}",
        shell.get_error_message()
    );
    assert_eq!(shell.get_player_dialog(), 4, "error dialog not open");

    println!("OK app_playback");
}
