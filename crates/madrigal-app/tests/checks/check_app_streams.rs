use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use madrigal_app::catalog::{
    apply_catalog_event, playable_track, wire_catalog_with, wire_play_requests, CatalogEvent,
    CatalogSource, Delivery, ExplorePages,
};
use madrigal_app::downloads::{register_with, StreamResolver};
use madrigal_app::playback::{Playback, PlaybackService};
use madrigal_app::state::apply_snapshot;
use madrigal_app::streams::{orchestrator, YTDLP_ENV};
use madrigal_core::{Track, TrackSource};
use madrigal_downloads::{DownloadState, JobOptions, ManagerConfig};
use madrigal_innertube::{CatalogClient, HomeFeed, SearchFilter, SearchOutcome};
use madrigal_player::engine::EngineConfig;
use madrigal_settings::{Content, Player as PlayerConfig, Storage};
use madrigal_streams::Orchestrator;
use madrigal_testkit as kit;
use madrigal_ui::MainShell;
use slint::Model;

const WAIT: Duration = Duration::from_secs(10);
const TITLE: &str = "Tomorrow Never Knows (Mono)";
const AGENT: &str = "madrigal-fake-agent/1.0";

const FAKE_OK: &str = r#"#!/bin/sh
log="$(dirname "$0")/ytdlp.log"
echo "$@" >> "$log"
sleep 1
for last in "$@"; do :; done
video_id=${last##*v=}
cat <<EOF
{"id": "$video_id", "title": "Fake Title", "duration": 8.0, "ext": "wav", "acodec": "pcm_s16le", "abr": 1411.0, "url": "@BASE@/media/$video_id.wav", "http_headers": {"User-Agent": "@AGENT@"}}
EOF
"#;

const FAKE_UNAVAILABLE: &str = r#"#!/bin/sh
log="$(dirname "$0")/ytdlp.log"
echo "$@" >> "$log"
echo "ERROR: [youtube] Video unavailable" >&2
exit 1
"#;

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

fn fixture_audio() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .join("madrigal-lyrics/tests/fixtures/embedded_uslt.mp3")
}

fn innertube_fixture(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../madrigal-innertube/tests/fixtures")
            .join(name),
    )
    .expect("read innertube fixture")
}

fn make_wav(dir: &Path) -> Vec<u8> {
    let out = dir.join("media.wav");
    let status = Command::new("ffmpeg")
        .args(["-y", "-stream_loop", "7", "-i"])
        .arg(fixture_audio())
        .args(["-t", "8", "-ar", "44100", "-ac", "2", "-c:a", "pcm_s16le"])
        .arg(&out)
        .output()
        .expect("run ffmpeg");
    assert!(status.status.success(), "ffmpeg: {status:?}");
    std::fs::read(&out).expect("read wav")
}

struct MediaServer {
    addr: SocketAddr,
    body: Arc<Vec<u8>>,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl MediaServer {
    fn start(body: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let addr = listener.local_addr().expect("local addr");
        let body = Arc::new(body);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let body = Arc::clone(&body);
            let requests = Arc::clone(&requests);
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let body = Arc::clone(&body);
                            let requests = Arc::clone(&requests);
                            thread::spawn(move || serve(stream, &body, &requests));
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => break,
                    }
                }
            })
        };
        Self {
            addr,
            body,
            requests,
            stop,
            worker: Some(worker),
        }
    }

    fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn body(&self) -> Vec<u8> {
        self.body.as_ref().clone()
    }

    fn clear_requests(&self) {
        self.requests.lock().expect("requests").clear();
    }

    fn saw_header(&self, needle: &str) -> bool {
        let needle = needle.to_ascii_lowercase();
        self.requests
            .lock()
            .expect("requests")
            .iter()
            .any(|head| head.to_ascii_lowercase().contains(&needle))
    }
}

impl Drop for MediaServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve(mut stream: TcpStream, body: &[u8], requests: &Mutex<Vec<String>>) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut head = Vec::new();
    let mut chunk = [0u8; 512];
    while !head.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => head.extend_from_slice(&chunk[..read]),
        }
    }
    requests
        .lock()
        .expect("requests")
        .push(String::from_utf8_lossy(&head).into_owned());
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    if stream.write_all(header.as_bytes()).is_ok() {
        let _ = stream.write_all(body);
        let _ = stream.flush();
    }
}

fn write_fake(dir: &Path, name: &str, template: &str, server: &MediaServer) -> PathBuf {
    let script = template
        .replace("@BASE@", &server.base())
        .replace("@AGENT@", AGENT);
    let path = dir.join(name);
    std::fs::write(&path, script).expect("write fake yt-dlp");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake yt-dlp");
    }
    path
}

fn invocations(log: &Path, video_id: &str) -> usize {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains(video_id))
        .count()
}

fn wait_until(label: &str, mut ready: impl FnMut() -> bool) {
    let started = Instant::now();
    while started.elapsed() < WAIT {
        if ready() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("timeout: {label}");
}

struct FixtureSource;

impl CatalogSource for FixtureSource {
    fn home(&mut self) -> Result<HomeFeed, madrigal_innertube::Error> {
        CatalogClient::parse_home(&innertube_fixture("home.json"))
    }

    fn suggestions(&mut self, _query: &str) -> Result<Vec<String>, madrigal_innertube::Error> {
        CatalogClient::parse_suggestions(&innertube_fixture("suggestions_beat.json"))
    }

    fn search(
        &mut self,
        _query: &str,
        filter: SearchFilter,
    ) -> Result<SearchOutcome, madrigal_innertube::Error> {
        match filter {
            SearchFilter::Songs => Ok(SearchOutcome::Songs(CatalogClient::parse_search_songs(
                &innertube_fixture("search_songs_beatles.json"),
            )?)),
            SearchFilter::Artists => {
                Ok(SearchOutcome::Artists(CatalogClient::parse_search_artists(
                    &innertube_fixture("search_artists_beatles.json"),
                )?))
            }
            SearchFilter::Albums => Ok(SearchOutcome::Albums(CatalogClient::parse_search_albums(
                &innertube_fixture("search_albums_beatles.json"),
            )?)),
        }
    }

    fn explore(&mut self, country: &str) -> Result<ExplorePages, madrigal_innertube::Error> {
        let mut charts = CatalogClient::parse_charts(&innertube_fixture("charts_ZZ.json"))?;
        charts.country = country.to_owned();
        Ok(ExplorePages {
            moods: CatalogClient::parse_mood_listing(&innertube_fixture("moods.json"))?,
            releases: CatalogClient::parse_new_releases(&innertube_fixture("explore.json"))?,
            browse: CatalogClient::parse_browse(&innertube_fixture("explore.json"))?,
            charts,
        })
    }
}

fn engine_config() -> EngineConfig {
    EngineConfig {
        vo: "null".to_owned(),
        ao: "null".to_owned(),
        gapless_audio: false,
    }
}

fn part_bypass(streams: &Orchestrator, track: &Track, log: &Path) {
    let (_, label) = streams
        .resolve(&track.identity, None)
        .expect("yt-dlp fallback resolves");
    assert_eq!(label, "default", "innertube was not disabled");
    assert!(
        invocations(log, &track.identity.id) >= 1,
        "yt-dlp not invoked"
    );
    assert!(
        madrigal_paths::cache_dir().join("streams").is_dir(),
        "stream cache escaped the profile root"
    );
}

fn part_playback(streams: Orchestrator, track: &Track, log: &Path, server: &MediaServer) {
    let settings = PlayerConfig::default();
    let mut playback =
        Playback::open(engine_config(), &settings, streams.clone()).expect("open playback");

    let started = Instant::now();
    playback
        .enqueue_track(track)
        .expect("enqueue youtube track");
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "enqueue blocked for {:?}",
        started.elapsed()
    );

    wait_until("youtube position advances", || {
        playback.poll(Instant::now());
        playback.position_ms() > 200
    });
    let snapshot = playback.snapshot();
    assert_eq!(snapshot.track_source, TrackSource::Youtube);
    assert_eq!(snapshot.title, track.title);
    assert_eq!(snapshot.artist, track.artist_text());
    assert_eq!(snapshot.album, track.album.clone().unwrap_or_default());
    assert!(!playback.paused());
    assert_eq!(
        playback.current_track().expect("current track").thumbnail,
        track.thumbnail
    );
    assert!(
        invocations(log, &track.identity.id) >= 1,
        "yt-dlp not invoked"
    );
    assert!(
        server.saw_header(&format!("user-agent: {AGENT}")),
        "user agent not sent to mpv"
    );

    let session = playback.session_state().expect("session state");
    assert_eq!(session.tracks, vec![track.identity.clone()]);
    assert!(
        session.tracks.iter().all(
            |identity| !identity.id.contains("googlevideo") && !identity.id.starts_with("http")
        ),
        "session persisted a stream url"
    );
    drop(playback);

    let _ = std::fs::remove_dir_all(madrigal_paths::cache_dir().join("streams"));
    let before = invocations(log, &track.identity.id);
    let mut restored = Playback::open(engine_config(), &settings, streams).expect("open restored");
    assert!(restored.restore_session(&session), "restore failed");
    restored.play_pause().expect("play restored track");
    wait_until("restored youtube track plays", || {
        restored.poll(Instant::now());
        restored.position_ms() > 200
    });
    assert!(
        invocations(log, &track.identity.id) > before,
        "restored track was not re-resolved"
    );
}

fn part_catalog_play(shell: &MainShell, streams: Orchestrator, tracks: &[Track], log: &Path) {
    let (sink, rx): (mpsc::Sender<CatalogEvent>, Receiver<CatalogEvent>) = mpsc::channel();
    let delivery: Delivery = {
        let sink = Arc::new(Mutex::new(sink));
        Arc::new(move |event| {
            if let Ok(sink) = sink.lock() {
                let _ = sink.send(event);
            }
        })
    };
    let wiring = wire_catalog_with(
        shell,
        &Content::default(),
        Box::new(FixtureSource),
        delivery,
    )
    .expect("wire catalog");
    let apply = |event: CatalogEvent| {
        apply_catalog_event(shell, event, &wiring.generations(), &wiring.home(), true);
    };

    shell.set_search_query("beatles".into());
    shell.invoke_query_changed("beatles".into());
    while shell.get_search_results().row_count() == 0 {
        apply(rx.recv_timeout(WAIT).expect("catalog event"));
    }
    let item = shell.get_search_results().row_data(0).expect("first song");
    assert_eq!(item.title.as_str(), TITLE);
    let target = playable_track(&item).expect("song is playable");
    assert_eq!(target.identity, tracks[0].identity);
    assert_eq!(target.title, TITLE);

    let playback =
        Playback::open(engine_config(), &PlayerConfig::default(), streams).expect("open playback");
    let service = PlaybackService::start(playback);
    wire_play_requests(shell, &service.core());
    shell.invoke_play_requested(item);

    wait_until("catalog item plays in the shell", || {
        if let Some(snapshot) = service.latest() {
            apply_snapshot(shell, &snapshot, true);
        }
        shell.get_track_title().as_str() == TITLE && shell.get_position_ms() > 200
    });
    assert!(!shell.get_paused(), "shell shows paused");
    assert_eq!(shell.get_queue_count(), 1);
    assert!(invocations(log, &tracks[0].identity.id) >= 1);
}

fn part_download(
    shell: &MainShell,
    streams: Orchestrator,
    track: &Track,
    root: &Path,
    server: &MediaServer,
) {
    let storage = Storage {
        download_concurrency: 1,
        ..Storage::default()
    };
    let config = ManagerConfig::with_storage(
        root.join("downloads"),
        root.join("library.sqlite3"),
        &storage,
    );
    let downloads = register_with(shell, Arc::new(StreamResolver::new(streams)), config);
    server.clear_requests();
    assert!(downloads.enqueue(track.clone(), JobOptions::default()));
    wait_until("youtube download completes", || {
        downloads.refresh(shell);
        downloads.state(&track.identity) == Some(DownloadState::Completed)
    });
    assert!(downloads.error(&track.identity).is_none());
    let output = downloads.output_of(&track.identity).expect("output path");
    assert_eq!(
        std::fs::read(&output).expect("read download"),
        server.body()
    );
    assert!(
        server.saw_header(&format!("user-agent: {AGENT}")),
        "download sent no user agent"
    );
}

fn part_failure(streams: Orchestrator, track: &Track, log: &Path) {
    let mut playback =
        Playback::open(engine_config(), &PlayerConfig::default(), streams).expect("open playback");
    playback.enqueue_track(track).expect("enqueue");
    let mut message = None;
    wait_until("resolution failure surfaces", || {
        playback.poll(Instant::now());
        message = playback.error();
        message.is_some()
    });
    let message = message.expect("error message");
    assert!(message.contains("stream unavailable"), "{message}");
    assert!(invocations(log, &track.identity.id) >= 1);
}

#[test]
fn app_streams() {
    if !have_ffmpeg() || !have_libmpv() {
        println!("SKIP app_streams (missing ffmpeg or libmpv)");
        return;
    }
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, root.path());
    std::env::set_var(madrigal_app::desktop::BUS_ADDRESS_ENV, "");
    std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", root.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", root.path().join("state"));
    std::env::set_var(madrigal_app::streams::INNERTUBE_ENV, "0");

    let server = MediaServer::start(make_wav(root.path()));
    let log = root.path().join("ytdlp.log");
    let ok_bin = write_fake(root.path(), "yt-dlp-ok", FAKE_OK, &server);
    let failing_bin = write_fake(root.path(), "yt-dlp-fail", FAKE_UNAVAILABLE, &server);

    let tracks = CatalogClient::parse_search_songs(&innertube_fixture("search_songs_beatles.json"))
        .expect("search songs fixture");
    assert!(tracks.len() >= 4, "fixture songs");
    assert!(tracks
        .iter()
        .all(|track| track.identity.source == TrackSource::Youtube));

    std::env::set_var(YTDLP_ENV, &ok_bin);
    let streams = orchestrator().expect("orchestrator");
    part_bypass(&streams, &tracks[3], &log);

    part_playback(streams.clone(), &tracks[0], &log, &server);

    kit::init_headless();
    let shell = MainShell::new().expect("shell");
    part_catalog_play(&shell, streams.clone(), &tracks, &log);
    part_download(&shell, streams, &tracks[1], root.path(), &server);

    std::env::set_var(YTDLP_ENV, &failing_bin);
    let failing = orchestrator().expect("failing orchestrator");
    part_failure(failing, &tracks[2], &log);

    println!("OK app_streams");
}
