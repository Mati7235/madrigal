use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

use madrigal_core::{Track, TrackIdentity};
use madrigal_downloads::{
    DownloadManager, DownloadRecord, DownloadRunner, JobOptions, ManagerConfig, ProgressEvent,
    RestoreOptions, RunContext, RunnerError, YtDlpRunner, output_path, parse_progress_line,
};
use madrigal_settings::Storage;

static SERIAL: Mutex<()> = Mutex::new(());

#[derive(Clone)]
enum Mode {
    Instant { delay_ms: u64 },
    Gate { release: std::sync::Arc<AtomicBool> },
    FailTimes { remaining: std::sync::Arc<AtomicUsize> },
    AlwaysFail,
}

#[derive(Clone)]
struct FakeRunner {
    mode: Mode,
    active: std::sync::Arc<AtomicUsize>,
    max_seen: std::sync::Arc<AtomicUsize>,
}

impl FakeRunner {
    fn instant() -> Self {
        Self {
            mode: Mode::Instant { delay_ms: 150 },
            active: std::sync::Arc::new(AtomicUsize::new(0)),
            max_seen: std::sync::Arc::new(AtomicUsize::new(0)),
        }
    }

    fn gate(release: std::sync::Arc<AtomicBool>) -> Self {
        Self {
            mode: Mode::Gate { release },
            active: std::sync::Arc::new(AtomicUsize::new(0)),
            max_seen: std::sync::Arc::new(AtomicUsize::new(0)),
        }
    }

    fn fail_times(n: usize) -> Self {
        Self {
            mode: Mode::FailTimes {
                remaining: std::sync::Arc::new(AtomicUsize::new(n)),
            },
            active: std::sync::Arc::new(AtomicUsize::new(0)),
            max_seen: std::sync::Arc::new(AtomicUsize::new(0)),
        }
    }

    fn always_fail() -> Self {
        Self {
            mode: Mode::AlwaysFail,
            active: std::sync::Arc::new(AtomicUsize::new(0)),
            max_seen: std::sync::Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl DownloadRunner for FakeRunner {
    fn run(
        &self,
        ctx: &RunContext,
        report: &dyn Fn(ProgressEvent),
    ) -> Result<PathBuf, RunnerError> {
        let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        loop {
            let seen = self.max_seen.load(Ordering::SeqCst);
            if now <= seen || self.max_seen.compare_exchange(seen, now, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                break;
            }
        }
        struct Guard(std::sync::Arc<AtomicUsize>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        let _guard = Guard(std::sync::Arc::clone(&self.active));
        match &self.mode {
            Mode::AlwaysFail => Err(RunnerError::Failed("boom".to_string())),
            Mode::FailTimes { remaining } => {
                if remaining.fetch_sub(1, Ordering::SeqCst) > 0 {
                    return Err(RunnerError::Failed("boom".to_string()));
                }
                write_output(ctx, report)
            }
            Mode::Gate { release } => {
                while !release.load(Ordering::SeqCst) {
                    if ctx.is_cancelled() {
                        return Err(RunnerError::Cancelled);
                    }
                    if ctx.is_paused() {
                        return Err(RunnerError::Paused);
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                if ctx.is_cancelled() {
                    return Err(RunnerError::Cancelled);
                }
                write_output(ctx, report)
            }
            Mode::Instant { delay_ms } => {
                std::thread::sleep(Duration::from_millis(*delay_ms));
                if ctx.is_cancelled() {
                    return Err(RunnerError::Cancelled);
                }
                write_output(ctx, report)
            }
        }
    }
}

fn write_output(
    ctx: &RunContext,
    report: &dyn Fn(ProgressEvent),
) -> Result<PathBuf, RunnerError> {
    for (index, percent) in [33.3, 66.6, 100.0].iter().enumerate() {
        report(ProgressEvent {
            key: ctx.identity.to_storage_string(),
            downloaded_bytes: ((index + 1) * 100) as u64,
            total_bytes: Some(300),
            speed_bps: Some(50.0),
            percent: *percent,
        });
    }
    if let Some(parent) = ctx.output.parent() {
        fs::create_dir_all(parent).map_err(|err| RunnerError::Failed(err.to_string()))?;
    }
    fs::write(&ctx.output, b"fake-audio-bytes").map_err(|err| RunnerError::Failed(err.to_string()))?;
    Ok(ctx.output.clone())
}

fn track(id: &str, title: &str) -> Track {
    Track {
        identity: TrackIdentity::youtube(id),
        title: title.to_string(),
        artists: vec!["The Artist".to_string()],
        album: Some("The Album".to_string()),
        duration_secs: Some(200),
        thumbnail: None,
        track_number: Some(2),
        local_path: None,
        set_video_id: None,
    }
}

fn config(root: &Path, concurrency: i32, max_retries: u32, ytdlp: PathBuf) -> ManagerConfig {
    let storage = Storage {
        download_concurrency: concurrency,
        ..Storage::default()
    };
    let mut cfg = ManagerConfig::with_storage(
        root.join("downloads"),
        root.join("library.sqlite3"),
        &storage,
    );
    cfg.max_retries = max_retries;
    cfg.ytdlp_bin = ytdlp;
    cfg
}

fn setup(concurrency: i32, max_retries: u32, runner: FakeRunner) -> (tempfile::TempDir, DownloadManager<FakeRunner>) {
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    let cfg = config(root.path(), concurrency, max_retries, PathBuf::from("yt-dlp"));
    (root, DownloadManager::new(cfg, runner))
}

fn wait_state<R: DownloadRunner>(
    manager: &DownloadManager<R>,
    identity: &TrackIdentity,
    want: madrigal_downloads::DownloadState,
) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if manager.state(identity) == Some(want) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn wait_gone<R: DownloadRunner>(manager: &DownloadManager<R>, identity: &TrackIdentity) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if manager.state(identity).is_none() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn concurrency_pause_resume_and_progress() {
    let _guard = SERIAL.lock().unwrap();
    let runner = FakeRunner::instant();
    let max_seen = std::sync::Arc::clone(&runner.max_seen);
    let (root, manager) = setup(2, 2, runner);
    let _ = &root;
    let rx = manager.subscribe();
    let identities: Vec<TrackIdentity> = (0..6)
        .map(|n| TrackIdentity::youtube(format!("CONC{n:04}")))
        .collect();
    for identity in &identities {
        let mut item = track(&identity.id, "Song");
        item.identity = identity.clone();
        assert!(manager.enqueue(item, JobOptions::default()));
    }
    assert!(manager.wait_idle(Duration::from_secs(20)));
    for identity in &identities {
        assert_eq!(
            manager.state(identity),
            Some(madrigal_downloads::DownloadState::Completed),
            "{identity}"
        );
    }
    let peak = max_seen.load(Ordering::SeqCst);
    assert!(peak <= 2, "concurrency exceeded: {peak}");
    let mut events = 0;
    while let Ok(event) = rx.try_recv() {
        assert!(event.downloaded_bytes > 0);
        assert!(event.speed_bps.unwrap_or(0.0) > 0.0);
        events += 1;
    }
    assert!(events >= 6, "expected progress events, got {events}");

    let release = std::sync::Arc::new(AtomicBool::new(false));
    let (root2, manager2) = setup(2, 2, FakeRunner::gate(std::sync::Arc::clone(&release)));
    let _ = &root2;
    let identity = TrackIdentity::youtube("PAUSE0001");
    assert!(manager2.enqueue(track(&identity.id, "Gated"), JobOptions::default()));
    assert!(wait_state(
        &manager2,
        &identity,
        madrigal_downloads::DownloadState::Downloading
    ));
    manager2.pause(&identity);
    assert!(wait_state(
        &manager2,
        &identity,
        madrigal_downloads::DownloadState::Paused
    ));
    manager2.resume(&identity);
    release.store(true, Ordering::SeqCst);
    assert!(wait_state(
        &manager2,
        &identity,
        madrigal_downloads::DownloadState::Completed
    ));
    assert!(manager2.wait_idle(Duration::from_secs(10)));
    manager2.shutdown();
    manager.shutdown();
}

#[test]
fn cancel_mid_run_removes_job() {
    let _guard = SERIAL.lock().unwrap();
    let release = std::sync::Arc::new(AtomicBool::new(false));
    let (root, manager) = setup(1, 2, FakeRunner::gate(release));
    let _ = &root;
    let identity = TrackIdentity::youtube("CANCEL001");
    assert!(manager.enqueue(track(&identity.id, "Gated"), JobOptions::default()));
    assert!(wait_state(
        &manager,
        &identity,
        madrigal_downloads::DownloadState::Downloading
    ));
    let output = manager.output_of(&identity).expect("output");
    manager.cancel(&identity);
    assert!(wait_gone(&manager, &identity));
    assert!(!output.exists());
    manager.shutdown();
}

#[test]
fn retry_then_quarantine_after_max_retries() {
    let _guard = SERIAL.lock().unwrap();
    let runner = FakeRunner::fail_times(1);
    let (root, manager) = setup(1, 1, runner);
    let _ = &root;
    let identity = TrackIdentity::youtube("RETRY0001");
    assert!(manager.enqueue(track(&identity.id, "Flaky"), JobOptions::default()));
    assert!(wait_state(
        &manager,
        &identity,
        madrigal_downloads::DownloadState::Failed
    ));
    assert_eq!(manager.attempts(&identity), Some(1));
    manager.retry(&identity);
    assert!(wait_state(
        &manager,
        &identity,
        madrigal_downloads::DownloadState::Completed
    ));

    let (root2, manager2) = setup(1, 1, FakeRunner::always_fail());
    let _ = &root2;
    let identity2 = TrackIdentity::youtube("QUAR00001");
    assert!(manager2.enqueue(track(&identity2.id, "Broken"), JobOptions::default()));
    assert!(wait_state(
        &manager2,
        &identity2,
        madrigal_downloads::DownloadState::Failed
    ));
    manager2.retry(&identity2);
    assert!(wait_state(
        &manager2,
        &identity2,
        madrigal_downloads::DownloadState::Quarantined
    ));
    assert!(manager2.error(&identity2).is_some());
    manager.shutdown();
    manager2.shutdown();
}

struct PartialRunner;

impl DownloadRunner for PartialRunner {
    fn run(
        &self,
        ctx: &RunContext,
        _report: &dyn Fn(ProgressEvent),
    ) -> Result<PathBuf, RunnerError> {
        if let Some(parent) = ctx.output.parent() {
            fs::create_dir_all(parent).expect("output dir");
        }
        fs::write(&ctx.output, b"partial-bytes").expect("partial write");
        Err(RunnerError::Failed("connection cut".to_string()))
    }
}

#[test]
fn quarantine_keeps_partial_file() {
    let _guard = SERIAL.lock().unwrap();
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    let cfg = config(root.path(), 1, 0, PathBuf::from("yt-dlp"));
    let downloads_dir = cfg.downloads_dir.clone();
    let manager = DownloadManager::new(cfg, PartialRunner);
    let identity = TrackIdentity::youtube("PARTIAL001");
    let song = track(&identity.id, "Partial");
    let output = output_path(&downloads_dir, &song);
    assert!(manager.enqueue(song, JobOptions::default()));
    assert!(wait_state(
        &manager,
        &identity,
        madrigal_downloads::DownloadState::Quarantined
    ));

    let quarantine_dir = downloads_dir.join("quarantine");
    let deadline = Instant::now() + Duration::from_secs(10);
    let quarantined = loop {
        let entries: Vec<PathBuf> = fs::read_dir(&quarantine_dir)
            .map(|dir| dir.map(|entry| entry.expect("entry").path()).collect())
            .unwrap_or_default();
        if !entries.is_empty() || Instant::now() > deadline {
            break entries;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(quarantined.len(), 1, "partial file should be quarantined");
    assert_eq!(
        fs::read(&quarantined[0]).expect("quarantined bytes"),
        b"partial-bytes"
    );
    assert!(!output.exists());
    manager.shutdown();
}

#[test]
fn delete_removes_file_and_registry_row() {
    let _guard = SERIAL.lock().unwrap();
    let (root, manager) = setup(1, 2, FakeRunner::instant());
    let _ = &root;
    let identity = TrackIdentity::youtube("DELETE001");
    assert!(manager.enqueue(track(&identity.id, "Keep"), JobOptions::default()));
    assert!(wait_state(
        &manager,
        &identity,
        madrigal_downloads::DownloadState::Completed
    ));
    let output = manager.output_of(&identity).expect("output");
    assert!(output.exists());
    assert!(manager.list_registry().iter().any(|r| r.id == identity.id));
    assert!(manager.delete(&identity));
    assert!(!output.exists());
    assert!(manager.list_registry().iter().all(|r| r.id != identity.id));
    assert!(manager.state(&identity).is_none());
    manager.shutdown();
}

#[test]
fn layout_export_import_restore_and_like() {
    let _guard = SERIAL.lock().unwrap();
    let base = PathBuf::from("/tmp/dl");
    let mut item = track("abc123", "My/Title: Mix");
    item.track_number = Some(2);
    let path = output_path(&base, &item);
    assert_eq!(
        path,
        base.join("The Artist")
            .join("The Album")
            .join("02 - My_Title_ Mix [abc123].opus")
    );

    let (root, manager) = setup(2, 2, FakeRunner::instant());
    let _ = &root;
    let first = TrackIdentity::youtube("EXPORT001");
    let second = TrackIdentity::youtube("EXPORT002");
    assert!(manager.enqueue(track(&first.id, "One"), JobOptions::default()));
    assert!(manager.enqueue(track(&second.id, "Two"), JobOptions::default()));
    assert!(manager.wait_idle(Duration::from_secs(20)));

    let export_path = root.path().join("export.json");
    manager.export_list(&export_path).expect("export");
    let records: Vec<DownloadRecord> = manager.import_list(&export_path).expect("import");
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|r| r.source == "youtube"));

    let liked = TrackIdentity::youtube("LIKE00001");
    assert!(manager.enqueue_on_like(track(&liked.id, "Liked"), true, true));
    assert!(!manager.enqueue_on_like(track(&liked.id, "Liked"), true, true));
    let skipped = TrackIdentity::youtube("LIKE00002");
    assert!(!manager.enqueue_on_like(track(&skipped.id, "Nope"), true, false));
    assert!(!manager.enqueue_on_like(track(&skipped.id, "Nope"), false, true));

    let missing: Vec<DownloadRecord> = records
        .iter()
        .map(|r| DownloadRecord {
            path: root.path().join("gone.opus").to_string_lossy().into_owned(),
            ..r.clone()
        })
        .collect();
    let requeued = manager.restore(
        &missing,
        RestoreOptions {
            redownload_missing: true,
        },
    );
    assert_eq!(requeued, 2);
    assert!(manager.wait_idle(Duration::from_secs(20)));
    manager.shutdown();
}

#[test]
fn ytdlp_runner_uses_shim_and_parses_newline_progress() {
    let _guard = SERIAL.lock().unwrap();
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    let bin = root.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_ytdlp.sh");
    let shim = bin.join("yt-dlp");
    fs::copy(&source, &shim).unwrap();
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
    let log = root.path().join("args.log");
    env::set_var("FAKE_YTDLP_LOG", &log);
    env::remove_var("FAKE_MODE");

    let output = root
        .path()
        .join("The Artist")
        .join("The Album")
        .join("03 - Song [SHIM001122].opus");
    let ctx = RunContext::new(
        TrackIdentity::youtube("SHIM001122"),
        output.clone(),
        None,
        shim.clone(),
    );
    let runner = YtDlpRunner::new();
    let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
    let probe = std::sync::Arc::clone(&seen);
    let result = runner.run(&ctx, &|event: ProgressEvent| {
        probe.lock().unwrap().push(event);
    });
    let produced = result.expect("shim run");
    assert_eq!(produced, output);
    assert!(output.exists());
    let logged = fs::read_to_string(&log).unwrap_or_default();
    assert!(logged.contains("--newline"), "args: {logged}");
    assert!(logged.contains("--extract-audio"), "args: {logged}");
    assert!(logged.contains("The Artist"), "args: {logged}");
    assert!(logged.contains("music.youtube.com/watch?v=SHIM001122"), "args: {logged}");

    let identity = TrackIdentity::youtube("X");
    let event = parse_progress_line(
        "[download]  12.3% of ~ 5.13MiB at  1.02MiB/s ETA 00:05",
        &identity,
    )
    .expect("parse");
    assert!((event.percent - 12.3).abs() < 0.01);
    assert!(event.speed_bps.unwrap_or(0.0) > 1_000_000.0);
    assert!(event.total_bytes.unwrap_or(0) > 5_000_000);
    assert!(parse_progress_line("[download] Destination: song.webm", &identity).is_none());

    env::set_var("FAKE_MODE", "fail_once");
    let ctx2 = RunContext::new(
        TrackIdentity::youtube("SHIMFAIL01"),
        root.path().join("out.opus"),
        None,
        shim,
    );
    assert!(matches!(
        runner.run(&ctx2, &|_| {}),
        Err(RunnerError::Failed(_))
    ));
    env::remove_var("FAKE_MODE");

    let ctx3 = RunContext::new(
        TrackIdentity::youtube("SHIMCANCEL"),
        root.path().join("cancelled.opus"),
        None,
        PathBuf::from("yt-dlp"),
    );
    ctx3.cancel();
    assert!(matches!(
        runner.run(&ctx3, &|_| {}),
        Err(RunnerError::Cancelled)
    ));
}
