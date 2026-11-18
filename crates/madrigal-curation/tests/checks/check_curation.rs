use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use madrigal_core::{Track, TrackIdentity};
use madrigal_curation::caches::{CacheKind, CacheLimits};
use madrigal_curation::content::{ContentFlags, apply_content_filters, flags_of};
use madrigal_curation::offline::NetworkGate;
use madrigal_curation::slop::{
    FilterMode, SlopFilter, fallback_lists, load_cached, parse_list, refresh_from_url,
    should_refresh, store_cached,
};
use madrigal_curation::{enforce_limits, prune_cache};

static ENV_LOCK: Mutex<()> = Mutex::new(());

const FIXTURE: &str = include_str!("../../fixtures/ai_filter_sample.txt");

fn track(id: &str, artists: &[&str]) -> Track {
    Track {
        identity: TrackIdentity::youtube(id),
        title: "Titulo".to_owned(),
        artists: artists.iter().map(|a| a.to_string()).collect(),
        album: None,
        duration_secs: None,
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

fn isolate_profile() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    env::remove_var("XDG_CONFIG_HOME");
    env::remove_var("XDG_DATA_HOME");
    env::remove_var("XDG_CACHE_HOME");
    root
}

#[test]
fn slop_parse_modes_and_counters() {
    let lists = parse_list(FIXTURE);
    assert!(lists.high.contains("UCxHighFakeBlock01"));
    assert!(lists.high.contains("dQw4w9WgXcQ"));
    assert!(lists.high.contains("AISlopMusicFactory"));
    assert!(lists.moderate.contains("UCxModerateFake002"));
    assert!(lists.moderate.contains("UCxWarnlistFake03"));
    assert!(!lists.high.contains("@@@"));
    assert!(!lists.moderate.contains("@@@"));

    let tracks = vec![
        track("dQw4w9WgXcQ", &["Rick Astley"]),
        track("AAAABBBBCCC", &["@aislopmusicfactory"]),
        track("DDDDEEEEFFF", &["UCxModerateFake002"]),
        track("GGGGHHHHIII", &["Artista Limpio"]),
    ];

    let off = SlopFilter::new(lists.clone(), FilterMode::Off);
    let (kept, hidden) = off.filter_tracks(tracks.clone());
    assert_eq!(kept.len(), 4);
    assert_eq!(hidden, 0);

    let high = SlopFilter::new(lists.clone(), FilterMode::HighConfidence);
    let (kept, hidden) = high.filter_tracks(tracks.clone());
    assert_eq!(hidden, 2);
    assert_eq!(kept.len(), 2);
    assert!(kept.iter().all(|t| t.identity.id != "dQw4w9WgXcQ"));

    let both = SlopFilter::new(lists.clone(), FilterMode::IncludeModerate);
    let (kept, hidden) = both.filter_tracks(tracks.clone());
    assert_eq!(hidden, 3);
    assert_eq!(kept.len(), 1);

    let status = both.status(1_700_000_000);
    assert_eq!(status.blocklist_count, lists.high.len());
    assert_eq!(status.warnlist_count, lists.moderate.len());
    assert_eq!(status.last_updated, 1_700_000_000);

    let mut settings = madrigal_settings::Settings::default();
    assert_eq!(FilterMode::from_settings(&settings), FilterMode::Off);
    settings.privacy.ai_content_filter_enabled = true;
    assert_eq!(
        FilterMode::from_settings(&settings),
        FilterMode::HighConfidence
    );
    settings.privacy.ai_content_filter_include_moderate = true;
    assert_eq!(
        FilterMode::from_settings(&settings),
        FilterMode::IncludeModerate
    );
}

fn serve(responses: Vec<(u16, Vec<u8>)>) -> (TcpListener, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let incoming = listener.try_clone().expect("clone");
    let handle = thread::spawn(move || {
        for (status, body) in responses {
            let (mut stream, _) = incoming.accept().expect("accept");
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let reason = if status == 200 { "OK" } else { "Not Found" };
            let head = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).expect("head");
            stream.write_all(&body).expect("body");
            stream.flush().ok();
        }
    });
    (listener, handle)
}

#[test]
fn slop_refresh_and_disk_cache() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _root = isolate_profile();

    let (listener, server) = serve(vec![
        (200, FIXTURE.as_bytes().to_vec()),
        (404, b"missing".to_vec()),
    ]);
    let port = listener.local_addr().expect("addr").port();
    let base = format!("http://127.0.0.1:{port}");

    let fetched = refresh_from_url(&format!("{base}/list.txt")).expect("fetch");
    assert_eq!(fetched, FIXTURE);
    let lists = parse_list(&fetched);
    assert!(!lists.is_empty());

    store_cached(&fetched).expect("store");
    assert_eq!(load_cached().expect("load"), FIXTURE);

    let missing = refresh_from_url(&format!("{base}/absent.txt"));
    assert!(matches!(
        missing,
        Err(madrigal_curation::slop::FetchError::Status(404))
    ));
    server.join().expect("server");

    assert!(!fallback_lists().is_empty());

    let now = madrigal_curation::slop::now_epoch_secs();
    assert!(should_refresh(0, now, false));
    assert!(!should_refresh(now, now, false));
    assert!(should_refresh(now, now, true));
}

#[test]
fn offline_gate_blocks_network_but_serves_local() {
    let gate = NetworkGate::new(true);
    assert!(gate.is_offline());
    assert!(gate.check().is_err());

    let mut called = false;
    let blocked = gate.try_online(|| {
        called = true;
        42
    });
    assert!(blocked.is_err());
    assert!(!called);

    let mut local: HashMap<String, String> = HashMap::new();
    local.insert("track:1".to_owned(), "cache hit".to_owned());
    assert_eq!(local.get("track:1").map(String::as_str), Some("cache hit"));

    let open = NetworkGate::from_settings(&madrigal_settings::Settings::default());
    assert!(!open.is_offline());
    assert_eq!(open.try_online(|| 7), Ok(7));

    let mut offline_settings = madrigal_settings::Settings::default();
    offline_settings.internet.offline_mode = true;
    assert!(NetworkGate::from_settings(&offline_settings).is_offline());

    let start = std::time::Instant::now();
    let _ = gate.check();
    assert!(start.elapsed() < Duration::from_secs(5));
}

fn write_sized(path: &std::path::Path, bytes: usize) {
    fs::write(path, vec![b'x'; bytes]).expect("write");
    thread::sleep(Duration::from_millis(30));
}

#[test]
fn caches_sweep_evicts_oldest_to_cap() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _root = isolate_profile();

    let songs = CacheKind::Song.dir();
    fs::create_dir_all(&songs).expect("mkdir");
    write_sized(&songs.join("oldest.bin"), 600_000);
    write_sized(&songs.join("middle.bin"), 600_000);
    write_sized(&songs.join("newest.bin"), 600_000);

    let images = CacheKind::Image.dir();
    fs::create_dir_all(&images).expect("mkdir");
    write_sized(&images.join("stale.bin"), 10);

    let limits = CacheLimits {
        image_mb: 0,
        song_mb: 1,
        canvas_mb: 0,
        lyrics_mb: 0,
    };
    let from_settings = CacheLimits::from_settings(&madrigal_settings::Settings::default());
    assert!(from_settings.image_mb > 0);

    let sweep = enforce_limits(&limits);
    assert_eq!(sweep.reports.len(), 4);

    let songs_report = sweep
        .reports
        .iter()
        .find(|r| r.kind == CacheKind::Song)
        .expect("songs");
    assert_eq!(songs_report.cap_bytes, 1024 * 1024);
    assert!(songs_report.remaining_bytes <= songs_report.cap_bytes);
    assert_eq!(songs_report.removed_files, 2);
    assert!(!songs.join("oldest.bin").exists());
    assert!(!songs.join("middle.bin").exists());
    assert!(songs.join("newest.bin").exists());

    let images_report = sweep
        .reports
        .iter()
        .find(|r| r.kind == CacheKind::Image)
        .expect("images");
    assert_eq!(images_report.remaining_bytes, 0);
    assert!(!images.join("stale.bin").exists());

    assert_eq!(sweep.total_removed(), 3);
    assert_eq!(
        sweep.total_freed(),
        songs_report.freed_bytes + images_report.freed_bytes
    );
    let summary = sweep.summary();
    assert!(summary.contains("songs:"));
    assert!(summary.contains("images:"));

    let direct = prune_cache(CacheKind::Lyrics, 0);
    assert_eq!(direct.remaining_bytes, 0);
}

#[test]
fn content_filters_hide_explicit_and_video() {
    use madrigal_core::TrackSource;

    let mut flagged = track("ExplicitTrack001", &["Limpio"]);
    flagged.title = "Tema (explicit)".to_owned();
    let mut video = track("dQw4w9WgXcQ", &["Limpio"]);
    video.set_video_id = Some("set123".to_owned());
    let playlist = track("VLPLFakePlaylist01", &["Limpio"]);
    let channel_track = track("UCxChannelFake001", &["Limpio"]);
    let mut tidal = track("AAAABBBBCCC", &["Limpio"]);
    tidal.identity = TrackIdentity::tidal("12345");
    let tracks = vec![
        flagged.clone(),
        video.clone(),
        playlist.clone(),
        channel_track.clone(),
        tidal.clone(),
    ];

    let meta = |t: &Track| ContentFlags {
        explicit: t.title.contains("(explicit)"),
        is_video: flags_of(t).is_video,
    };

    let (kept, report) = apply_content_filters(tracks.clone(), meta, false, false);
    assert_eq!(kept.len(), 5);
    assert_eq!(report.total(), 0);

    let (kept, report) = apply_content_filters(tracks.clone(), meta, true, false);
    assert_eq!(report.explicit_hidden, 1);
    assert_eq!(report.video_hidden, 0);
    assert!(kept.iter().all(|t| !t.title.contains("(explicit)")));

    let (kept, report) = apply_content_filters(tracks.clone(), meta, false, true);
    assert_eq!(report.video_hidden, 1);
    assert_eq!(report.explicit_hidden, 0);
    assert!(kept.iter().all(|t| t.identity.id != "dQw4w9WgXcQ"));

    assert!(!flags_of(&playlist).is_video);
    assert!(!flags_of(&channel_track).is_video);
    assert!(!flags_of(&tidal).is_video);
    assert_eq!(tidal.source(), TrackSource::Tidal);

    let mut settings = madrigal_settings::Settings::default();
    settings.content.hide_explicit = true;
    settings.content.hide_video = true;
    let (kept, report) =
        madrigal_curation::content::apply_from_settings(tracks, meta, &settings);
    assert_eq!(report.total(), 2);
    assert_eq!(kept.len(), 3);
}
