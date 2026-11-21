use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use madrigal_app::downloads::{register_with, Downloads, UrlResolver};
use madrigal_core::{Track, TrackIdentity};
use madrigal_downloads::{DownloadState, JobOptions, ManagerConfig};
use madrigal_settings::Storage;
use madrigal_testkit as kit;
use madrigal_ui::MainShell;
use slint::{ComponentHandle, Model};

const IDLE_TIMEOUT: Duration = Duration::from_secs(10);

struct Resolver {
    base: String,
}

impl UrlResolver for Resolver {
    fn resolve(&self, identity: &TrackIdentity) -> Result<String, String> {
        Ok(format!("{}/{}.mp3", self.base, identity.id))
    }
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .join("madrigal-lyrics/tests/fixtures/embedded_uslt.mp3")
}

fn fixture_bytes() -> Vec<u8> {
    std::fs::read(fixture()).expect("fixture audio")
}

#[derive(Clone, Copy)]
enum Mode {
    Stream { chunk: usize, delay_ms: u64 },
    FailFirst(usize),
    AlwaysFail,
}

struct TestServer {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl TestServer {
    fn start(mode: Mode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener.set_nonblocking(true).expect("nonblocking listener");
        let addr = listener.local_addr().expect("local addr");
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                let bytes = fixture_bytes();
                let attempts = AtomicUsize::new(0);
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => serve(&mut stream, &bytes, mode, &attempts),
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
            stop,
            worker: Some(worker),
        }
    }

    fn base(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 512];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buffer).into_owned()
}

fn serve(stream: &mut TcpStream, bytes: &[u8], mode: Mode, attempts: &AtomicUsize) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let request = read_request(stream);
    if !request.starts_with("GET ") {
        return;
    }
    let fail = match mode {
        Mode::AlwaysFail => true,
        Mode::FailFirst(times) => attempts.fetch_add(1, Ordering::SeqCst) < times,
        Mode::Stream { .. } => false,
    };
    let header = if fail {
        "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_string()
    } else {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bytes.len()
        )
    };
    if stream.write_all(header.as_bytes()).is_err() {
        return;
    }
    if fail {
        let _ = stream.flush();
        return;
    }
    match mode {
        Mode::Stream { chunk, delay_ms } => {
            for part in bytes.chunks(chunk.max(1)) {
                if stream.write_all(part).is_err() {
                    return;
                }
                let _ = stream.flush();
                if delay_ms > 0 {
                    thread::sleep(Duration::from_millis(delay_ms));
                }
            }
        }
        _ => {
            let _ = stream.write_all(bytes);
        }
    }
    let _ = stream.flush();
}

struct Harness {
    root: tempfile::TempDir,
    shell: MainShell,
    downloads: Downloads,
    _server: TestServer,
}

impl Harness {
    fn new(mode: Mode, max_retries: u32) -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        let server = TestServer::start(mode);
        let shell = MainShell::new().expect("shell");
        let storage = Storage {
            download_concurrency: 1,
            ..Storage::default()
        };
        let mut config = ManagerConfig::with_storage(
            root.path().join("downloads"),
            root.path().join("library.sqlite3"),
            &storage,
        );
        config.max_retries = max_retries;
        let resolver = Resolver {
            base: server.base(),
        };
        let downloads = register_with(&shell, Arc::new(resolver), config);
        Self {
            root,
            shell,
            downloads,
            _server: server,
        }
    }

    fn track(&self, id: &str, title: &str) -> Track {
        Track {
            identity: TrackIdentity::youtube(id),
            title: title.to_owned(),
            artists: vec!["The Artist".to_owned()],
            album: Some("The Album".to_owned()),
            duration_secs: Some(200),
            thumbnail: None,
            track_number: Some(1),
            local_path: None,
            set_video_id: None,
        }
    }

    fn pump(&self) {
        self.downloads.refresh(&self.shell);
    }

    fn state(&self, identity: &TrackIdentity) -> Option<DownloadState> {
        self.downloads.state(identity)
    }

    fn wait_for<F>(&self, identity: &TrackIdentity, label: &str, ready: F) -> DownloadState
    where
        F: Fn(Option<DownloadState>) -> bool,
    {
        let deadline = Instant::now() + IDLE_TIMEOUT;
        loop {
            let state = self.state(identity);
            if ready(state) {
                self.pump();
                return state.expect("state after wait");
            }
            if Instant::now() > deadline {
                panic!("{label}: estado {state:?}");
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
}

fn row_state(shell: &MainShell, id: &str) -> Option<i32> {
    let model = shell.get_dl_rows();
    for index in 0..model.row_count() {
        if let Some(row) = model.row_data(index) {
            if row.id.as_str() == id {
                return Some(row.state);
            }
        }
    }
    let failed = shell.get_dl_failed();
    for index in 0..failed.row_count() {
        if let Some(row) = failed.row_data(index) {
            if row.id.as_str() == id {
                return Some(row.state);
            }
        }
    }
    None
}

fn row_progress(shell: &MainShell, id: &str) -> Option<f32> {
    let model = shell.get_dl_rows();
    for index in 0..model.row_count() {
        if let Some(row) = model.row_data(index) {
            if row.id.as_str() == id {
                return Some(row.progress);
            }
        }
    }
    None
}

fn row_speed(shell: &MainShell, id: &str) -> Option<String> {
    let model = shell.get_dl_rows();
    for index in 0..model.row_count() {
        if let Some(row) = model.row_data(index) {
            if row.id.as_str() == id {
                return Some(row.speed.to_string());
            }
        }
    }
    None
}

#[test]
fn app_downloads() {
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, root.path());
    std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", root.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", root.path().join("state"));
    kit::init_headless();

    progress_queue_and_page();
    retry_after_failure();
    cancel_mid_run();
    pause_and_resume();
    quarantine_after_failure();

    println!("OK app_downloads");
}

fn progress_queue_and_page() {
    let h = Harness::new(
        Mode::Stream {
            chunk: 256,
            delay_ms: 4,
        },
        0,
    );
    let first = h.track("PROGRESS1", "Progress Song");
    let second = h.track("PROGRESS2", "Queued Song");
    assert!(h.downloads.enqueue(first.clone(), JobOptions::default()));
    assert!(h.downloads.enqueue(second.clone(), JobOptions::default()));

    let mut saw_downloading = false;
    let mut saw_queued = false;
    let mut saw_mid_progress = false;
    let mut saw_speed = false;
    let deadline = Instant::now() + IDLE_TIMEOUT;
    loop {
        h.pump();
        if row_state(&h.shell, "PROGRESS1") == Some(1) {
            saw_downloading = true;
        }
        if row_state(&h.shell, "PROGRESS2") == Some(0) {
            saw_queued = true;
        }
        if let Some(progress) = row_progress(&h.shell, "PROGRESS1") {
            if progress > 0.0 && progress < 1.0 {
                saw_mid_progress = true;
            }
        }
        if row_speed(&h.shell, "PROGRESS1").is_some_and(|speed| !speed.is_empty()) {
            saw_speed = true;
        }
        if h.state(&first.identity) == Some(DownloadState::Completed)
            && h.state(&second.identity) == Some(DownloadState::Completed)
        {
            break;
        }
        assert!(Instant::now() <= deadline, "downloads did not complete");
        thread::sleep(Duration::from_millis(2));
    }
    h.pump();
    assert!(saw_downloading, "no downloading row");
    assert!(saw_queued, "no queued row (concurrency 1)");
    assert!(saw_mid_progress, "no intermediate progress observed");
    assert!(saw_speed, "no speed observed");
    assert_eq!(row_state(&h.shell, "PROGRESS1"), Some(3));
    assert_eq!(row_state(&h.shell, "PROGRESS2"), Some(3));
    assert_eq!(h.shell.get_dl_failed().row_count(), 0);
    assert_eq!(h.shell.get_dl_rows().row_count(), 2);
    let rows = h.shell.get_dl_rows();
    assert_eq!(
        rows.row_data(0).expect("row 0").title.as_str(),
        "Progress Song"
    );
    assert_eq!(
        rows.row_data(1).expect("row 1").title.as_str(),
        "Queued Song"
    );
    assert_eq!(
        rows.row_data(0).expect("row 0").subtitle.as_str(),
        "The Artist"
    );

    let expected = fixture_bytes();
    for track in [&first, &second] {
        let output = h.downloads.output_of(&track.identity).expect("output");
        assert_eq!(std::fs::read(&output).expect("downloaded file"), expected);
    }

    h.shell.set_page(4);
    h.shell.show().expect("show");
    kit::mock_advance(Duration::from_millis(200));
    assert_eq!(
        h.shell.get_dl_rows().row_count(),
        2,
        "rows after timer pump"
    );
}

fn retry_after_failure() {
    let h = Harness::new(Mode::FailFirst(1), 1);
    let track = h.track("RETRY1", "Retry Song");
    assert!(h.downloads.enqueue(track.clone(), JobOptions::default()));
    h.wait_for(&track.identity, "retry fail", |state| {
        state == Some(DownloadState::Failed)
    });
    assert_eq!(h.downloads.attempts(&track.identity), Some(1));
    assert!(h.downloads.error(&track.identity).is_some());
    assert_eq!(row_state(&h.shell, "RETRY1"), Some(4));
    assert_eq!(h.shell.get_dl_failed().row_count(), 1);

    h.shell.invoke_dl_retry("RETRY1".into());
    assert_eq!(h.shell.get_dl_last_action().as_str(), "dl-retry");
    h.wait_for(&track.identity, "retry done", |state| {
        state == Some(DownloadState::Completed)
    });
    assert_eq!(row_state(&h.shell, "RETRY1"), Some(3));
    assert_eq!(h.shell.get_dl_failed().row_count(), 0);
    let output = h.downloads.output_of(&track.identity).expect("output");
    assert_eq!(std::fs::read(&output).expect("downloaded file"), fixture_bytes());
}

fn cancel_mid_run() {
    let h = Harness::new(
        Mode::Stream {
            chunk: 256,
            delay_ms: 15,
        },
        0,
    );
    let track = h.track("CANCEL1", "Cancel Song");
    assert!(h.downloads.enqueue(track.clone(), JobOptions::default()));
    h.wait_for(&track.identity, "cancel downloading", |state| {
        state == Some(DownloadState::Downloading)
    });
    let output = h.downloads.output_of(&track.identity).expect("output");
    h.shell.invoke_dl_cancel("CANCEL1".into());
    assert_eq!(h.shell.get_dl_last_action().as_str(), "dl-cancel");

    let deadline = Instant::now() + IDLE_TIMEOUT;
    loop {
        h.pump();
        if h.state(&track.identity).is_none()
            && h.shell.get_dl_rows().row_count() == 0
            && h.shell.get_dl_failed().row_count() == 0
        {
            break;
        }
        assert!(Instant::now() <= deadline, "cancel had no effect");
        thread::sleep(Duration::from_millis(2));
    }
    assert!(!output.exists(), "partial file left after cancel");
}

fn pause_and_resume() {
    let h = Harness::new(
        Mode::Stream {
            chunk: 256,
            delay_ms: 4,
        },
        0,
    );
    let first = h.track("PAUSE1", "Pause One");
    let second = h.track("PAUSE2", "Pause Two");
    assert!(h.downloads.enqueue(first.clone(), JobOptions::default()));
    assert!(h.downloads.enqueue(second.clone(), JobOptions::default()));
    h.wait_for(&first.identity, "pause first downloading", |state| {
        state == Some(DownloadState::Downloading)
    });

    h.pump();
    h.shell.invoke_dl_pause("PAUSE2".into());
    assert_eq!(h.shell.get_dl_last_action().as_str(), "dl-pause");
    h.wait_for(&second.identity, "second paused", |state| {
        state == Some(DownloadState::Paused)
    });
    assert_eq!(row_state(&h.shell, "PAUSE2"), Some(2));

    h.shell.invoke_dl_resume("PAUSE2".into());
    assert_eq!(h.shell.get_dl_last_action().as_str(), "dl-resume");
    h.wait_for(&second.identity, "second done", |state| {
        state == Some(DownloadState::Completed)
    });
    let output = h.downloads.output_of(&second.identity).expect("output");
    assert_eq!(std::fs::read(&output).expect("downloaded file"), fixture_bytes());
}

fn quarantine_after_failure() {
    let h = Harness::new(Mode::AlwaysFail, 0);
    let track = h.track("QUAR1", "Broken Song");
    assert!(h.downloads.enqueue(track.clone(), JobOptions::default()));
    h.wait_for(&track.identity, "quarantine", |state| {
        state == Some(DownloadState::Quarantined)
    });
    assert_eq!(h.downloads.attempts(&track.identity), Some(1));
    assert!(h.downloads.error(&track.identity).is_some());
    assert_eq!(row_state(&h.shell, "QUAR1"), Some(5));
    assert_eq!(h.shell.get_dl_failed().row_count(), 1);
    let output = h.downloads.output_of(&track.identity).expect("output");
    let quarantine = h.root.path().join("downloads/quarantine");
    let deadline = Instant::now() + IDLE_TIMEOUT;
    while !(quarantine.is_dir() && !output.exists()) {
        assert!(Instant::now() < deadline, "partial file never quarantined");
        thread::sleep(Duration::from_millis(2));
    }
}
