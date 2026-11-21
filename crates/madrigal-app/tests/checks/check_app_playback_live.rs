use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use madrigal_app::catalog::playable_track;
use madrigal_app::state::AppState;
use madrigal_innertube::CatalogClient;
use madrigal_player::engine::EngineConfig;
use madrigal_testkit as kit;
use madrigal_ui::song_results;

const QUERY: &str = "yomi yori kikoyu ICDD";
const WAIT: Duration = Duration::from_secs(20);

fn have_libmpv() -> bool {
    if PathBuf::from("/usr/lib/libmpv.so").exists() {
        return true;
    }
    Command::new("ldconfig")
        .arg("-p")
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("libmpv.so"))
}

#[test]
fn app_playback_live() {
    if !have_libmpv() {
        println!("SKIP app_playback_live (missing libmpv)");
        return;
    }
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, root.path());
    std::env::set_var(madrigal_app::desktop::BUS_ADDRESS_ENV, "");
    std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", root.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", root.path().join("state"));

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let tracks = runtime
        .block_on(async {
            let client = CatalogClient::new().await?;
            client.search_songs(QUERY).await
        })
        .expect("live search");
    assert!(!tracks.is_empty(), "no songs for {QUERY}");
    let track = tracks.first().cloned().expect("first song");
    println!(
        "live search: {} songs, first: {} - {} ({}s) id={}",
        tracks.len(),
        track.title,
        track.artist_text(),
        track.duration_secs.unwrap_or(0),
        track.identity.id
    );
    let item = song_results(std::slice::from_ref(&track))
        .into_iter()
        .next()
        .expect("catalog item");
    assert!(item.playable, "first search result is not playable");
    assert_eq!(
        playable_track(&item).map(|track| track.identity),
        Some(track.identity.clone())
    );

    kit::init_headless();
    let app = AppState::boot_with_engine(EngineConfig {
        vo: "null".to_owned(),
        ao: "auto".to_owned(),
        gapless_audio: false,
    })
    .expect("boot app");
    let shell = &app.ui.shell;
    shell.invoke_play_requested(item);

    let started = Instant::now();
    let mut positions = Vec::new();
    while started.elapsed() < WAIT {
        kit::mock_advance(Duration::from_millis(100));
        std::thread::sleep(Duration::from_millis(20));
        let position = shell.get_position_ms();
        if positions.last().copied() != Some(position) {
            positions.push(position);
        }
        if position > 1000 {
            break;
        }
    }
    if shell.get_position_ms() <= 1000 {
        println!(
            "positions: {positions:?} title={:?} paused={} error_code={:?} error={:?} dialog={}",
            shell.get_track_title().as_str(),
            shell.get_paused(),
            shell.get_error_code().as_str(),
            shell.get_error_message().as_str(),
            shell.get_player_dialog()
        );
        for line in app.engine_logs() {
            println!("  mpv {line}");
        }
        panic!("position did not advance in {WAIT:?}");
    }
    let first = shell.get_position_ms();
    std::thread::sleep(Duration::from_secs(1));
    kit::mock_advance(Duration::from_millis(100));
    let second = shell.get_position_ms();
    assert!(second > first, "position stalled: {first} -> {second}");
    assert_eq!(shell.get_track_title().as_str(), track.title.as_str());
    assert!(!shell.get_paused(), "shell shows paused");
    assert!(
        shell.get_error_code().is_empty(),
        "unexpected error: {}",
        shell.get_error_message()
    );
    assert!(
        !app
            .engine_logs()
            .iter()
            .any(|line| line.contains("Audio output auto not found")),
        "ao=auto was passed to mpv as a driver name"
    );
    println!("OK app_playback_live position {first} -> {second} ms");
}
