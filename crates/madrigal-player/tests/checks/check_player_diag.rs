use std::env;
use std::fs;
use std::os::unix::net::UnixListener;
use std::sync::{Mutex, MutexGuard};

use madrigal_player::diag_log::{DiagLog, DiagLogConfig, Level};
use madrigal_player::engine::EngineEvent;
use madrigal_player::latency::LatencyTracker;
use madrigal_player::single_instance::{InstanceRole, acquire_at};
use std::time::{Duration, Instant};

static PROFILE_LOCK: Mutex<()> = Mutex::new(());

fn lock_profile() -> MutexGuard<'static, ()> {
    PROFILE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn isolate() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    env::remove_var("XDG_CONFIG_HOME");
    env::remove_var("XDG_DATA_HOME");
    env::remove_var("XDG_CACHE_HOME");
    root
}

fn approx(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "expected {expected}, got {actual}"
    );
}

fn log_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out: Vec<std::path::PathBuf> = fs::read_dir(dir)
        .expect("read log dir")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "log")
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("diag"))
        })
        .collect();
    out.sort();
    out
}

#[test]
fn diag_log_rotates_filters_and_exports() {
    let root = tempfile::tempdir().expect("tempdir");
    let log = DiagLog::open_in(
        &root.path().join("logs"),
        DiagLogConfig {
            max_files: 3,
            max_bytes: 160,
        },
    )
    .expect("open diag log");
    for i in 0..40 {
        log.push(Level::Info, "t", &format!("n={i:03} padding-xxxxxxxxxx"))
            .expect("push");
    }
    let files = log_files(log.dir());
    assert!(files.len() <= 3, "ring capped: {}", files.len());
    let active = fs::metadata(log.dir().join("diag.log")).expect("active stat");
    assert!(active.len() <= 160, "active capped: {}", active.len());
    let all = log.read(Level::Info).expect("read all");
    assert!(all.len() < 40, "oldest dropped: {}", all.len());
    assert!(
        !all.iter().any(|line| line.text.contains("n=000")),
        "first line evicted"
    );
    assert!(
        all.iter().any(|line| line.text.contains("n=039")),
        "latest line kept"
    );
    for line in &all {
        assert!(line.epoch_secs > 0);
        assert_eq!(line.prefix, "t");
    }

    let root = tempfile::tempdir().expect("tempdir");
    let log = DiagLog::open_in(&root.path().join("logs"), DiagLogConfig::default())
        .expect("open diag log");
    log.push(Level::Info, "f", "info-msg").expect("push");
    log.push(Level::Warn, "f", "warn-msg").expect("push");
    log.push(Level::Error, "f", "error-msg").expect("push");
    assert_eq!(log.read(Level::Info).expect("read").len(), 3);
    let warns = log.read(Level::Warn).expect("read warns");
    assert_eq!(warns.len(), 2);
    assert!(warns.iter().all(|line| line.level >= Level::Warn));
    let errors = log.read(Level::Error).expect("read errors");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].text, "error-msg");

    assert!(
        log.push_engine_event(&EngineEvent::WarnLog {
            prefix: "cplayer".to_owned(),
            text: "something failed".to_owned(),
        })
        .expect("feed warn")
    );
    assert!(
        !log
            .push_engine_event(&EngineEvent::StartFile)
            .expect("feed start")
    );
    let warns = log.read(Level::Warn).expect("read warns");
    assert_eq!(warns.len(), 3);
    assert_eq!(warns[2].prefix, "cplayer");

    let dest = root.path().join("export.txt");
    let exported = log.export(&dest).expect("export");
    assert_eq!(exported, dest);
    let text = fs::read_to_string(&dest).expect("read export");
    assert!(text.contains("info-msg"));
    assert!(text.contains("warn-msg"));
    assert!(text.contains("something failed"));
    assert_eq!(text.lines().count(), log.read(Level::Info).expect("read").len());
    let leftovers: Vec<_> = fs::read_dir(root.path())
        .expect("read root")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "tmp"))
        .collect();
    assert!(leftovers.is_empty(), "no temp leftovers: {leftovers:?}");
}

#[test]
fn diag_log_default_path_under_data_dir() {
    let _guard = lock_profile();
    let root = isolate();
    let log = DiagLog::open().expect("open default");
    assert!(log.dir().starts_with(root.path()));
    log.push(Level::Warn, "probe", "hello").expect("push");
    let lines = log.read(Level::Info).expect("read");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].prefix, "probe");
    assert_eq!(lines[0].text, "hello");
}

#[test]
fn single_instance_socket_semantics() {
    let root = tempfile::tempdir().expect("tempdir");
    let sock = root.path().join("run/madrigal.sock");

    let primary = match acquire_at(&sock).expect("first bind") {
        InstanceRole::Primary(guard) => guard,
        InstanceRole::Secondary => panic!("first bind must be primary"),
    };
    assert!(sock.exists());
    match acquire_at(&sock).expect("second acquire") {
        InstanceRole::Secondary => {}
        InstanceRole::Primary(_) => panic!("live second instance must be refused"),
    }
    drop(primary);

    let rebound = match acquire_at(&sock).expect("rebind") {
        InstanceRole::Primary(guard) => guard,
        InstanceRole::Secondary => panic!("released socket must rebind"),
    };
    match acquire_at(&sock).expect("second acquire") {
        InstanceRole::Secondary => {}
        InstanceRole::Primary(_) => panic!("live second instance must be refused"),
    }
    drop(rebound);

    let stale = UnixListener::bind(&sock).expect("stale bind");
    drop(stale);
    assert!(sock.exists());
    let cleaned = match acquire_at(&sock).expect("stale rebind") {
        InstanceRole::Primary(guard) => guard,
        InstanceRole::Secondary => panic!("stale socket must be cleaned and rebound"),
    };
    match acquire_at(&sock).expect("second acquire") {
        InstanceRole::Secondary => {}
        InstanceRole::Primary(_) => panic!("rebound socket must be live"),
    }
    drop(cleaned);
}

#[test]
fn latency_p50_p95_over_sliding_window() {
    assert!(LatencyTracker::new(8).stats().is_none());

    let mut tracker = LatencyTracker::new(100);
    let base = Instant::now();
    for i in 1..=100u64 {
        tracker.enqueue_at(i, base);
        tracker.resolved_at(i, base + Duration::from_millis(5 * i));
        let total = tracker
            .started_at(i, base + Duration::from_millis(10 * i))
            .expect("started");
        approx(total, (10 * i) as f64);
    }
    assert!(tracker.started_at(7, base + Duration::from_millis(1)).is_none());
    assert!(tracker.started_at(999, base).is_none());
    let report = tracker.stats().expect("stats");
    assert_eq!(report.total.samples, 100);
    approx(report.total.p50_ms, 500.0);
    approx(report.total.p95_ms, 950.0);
    assert_eq!(report.resolve.samples, 100);
    approx(report.resolve.p50_ms, 250.0);
    approx(report.resolve.p95_ms, 475.0);

    let mut window = LatencyTracker::new(10);
    let base = Instant::now();
    for i in 1..=15u64 {
        window.enqueue_at(i, base);
        window
            .started_at(i, base + Duration::from_millis(10 * i))
            .expect("started");
    }
    let report = window.stats().expect("stats");
    assert_eq!(report.total.samples, 10);
    approx(report.total.p50_ms, 100.0);
    approx(report.total.p95_ms, 150.0);
    assert_eq!(report.resolve.samples, 0);
}
