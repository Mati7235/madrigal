use std::env;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use madrigal_core::{Event, Track, TrackIdentity};
use madrigal_scrobble::outbox::due_events;
use madrigal_scrobble::{
    Eligibility, MalojaClient, Outbox, OutboxEntry, ScrobbleSink, SubmitOutcome,
};

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock")
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs() as i64
}

fn setup() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    madrigal_paths::ensure_dirs().expect("dirs");
    root
}

fn sample_track() -> Track {
    Track {
        identity: TrackIdentity::youtube("dQw4w9WgXcQ"),
        title: "Never Gonna Give You Up".to_owned(),
        artists: vec!["Rick Astley".to_owned()],
        album: Some("Whenever You Need Somebody".to_owned()),
        duration_secs: Some(212),
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

fn closed_port_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

fn serve_ok(times: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!(
        "http://127.0.0.1:{}",
        listener.local_addr().expect("addr").port()
    );
    std::thread::spawn(move || {
        for mut stream in listener.incoming().take(times).flatten() {
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = stream.read(&mut buf).expect("read");
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&buf[..n]);
                if let Some(end) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&raw[..end]).into_owned();
                    let length: usize = head
                        .lines()
                        .skip(1)
                        .filter_map(|line| line.split_once(':'))
                        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
                        .and_then(|(_, value)| value.trim().parse().ok())
                        .unwrap_or(0);
                    if raw.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let body = "{}";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    url
}

struct FnSink<F>(F);

impl<F: Fn(&OutboxEntry) -> SubmitOutcome> ScrobbleSink for FnSink<F> {
    fn submit(&self, entry: &OutboxEntry) -> SubmitOutcome {
        (self.0)(entry)
    }
}

#[test]
fn queue_while_down_persists_and_flushes_when_up() {
    let _guard = lock();
    let _root = setup();
    let timeout = Duration::from_secs(3);

    let mut outbox = Outbox::open_default().expect("open");
    assert!(outbox.is_empty());

    let down = MalojaClient::new(&closed_port_url(), "key-1").with_timeout(timeout);
    let failing = FnSink(|entry: &OutboxEntry| {
        SubmitOutcome::from_result(&down.scrobble(&entry.track(), entry.started_at))
    });
    assert!(outbox.push(&sample_track(), 1_700_000_000).expect("push"));
    assert!(!outbox
        .push(&sample_track(), 1_700_000_000)
        .expect("duplicate"));
    assert_eq!(outbox.len(), 1);

    let stats = outbox.flush(&failing, now_secs()).expect("flush");
    assert_eq!((stats.submitted, stats.retried, stats.dropped), (0, 1, 0));
    let pending = outbox.due(i64::MAX);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].attempts, 1);
    drop(outbox);

    let reopened = Outbox::open_default().expect("reopen");
    assert_eq!(reopened.len(), 1);
    assert_eq!(reopened.due(i64::MAX)[0].attempts, 1);
    drop(reopened);

    let mut outbox = Outbox::open_default().expect("reopen");
    let url = serve_ok(1);
    let up = MalojaClient::new(&url, "key-1").with_timeout(timeout);
    let working = FnSink(|entry: &OutboxEntry| {
        SubmitOutcome::from_result(&up.scrobble(&entry.track(), entry.started_at))
    });
    let stats = outbox.flush(&working, i64::MAX).expect("flush");
    assert_eq!((stats.submitted, stats.retried, stats.dropped), (1, 0, 0));
    assert!(outbox.is_empty());
    assert!(!outbox.push(&sample_track(), 1_700_000_000).expect("replay"));
}

#[test]
fn backoff_delays_grow_across_failures() {
    let _guard = lock();
    let _root = setup();
    let mut outbox = Outbox::open_default().expect("open").with_backoff(10, 100);
    outbox.push(&sample_track(), 1_700_000_000).expect("push");

    let failing = FnSink(|_: &OutboxEntry| SubmitOutcome::Retryable);
    let first = now_secs();
    outbox.flush(&failing, first).expect("flush");
    let after_first = outbox.due(i64::MAX);
    assert_eq!(after_first[0].attempts, 1);
    assert_eq!(after_first[0].next_retry_at, first + 10);

    outbox.flush(&failing, first + 10).expect("flush");
    let after_second = outbox.due(i64::MAX);
    assert_eq!(after_second[0].attempts, 2);
    assert_eq!(after_second[0].next_retry_at, first + 30);

    outbox.flush(&failing, first + 30).expect("flush");
    let after_third = outbox.due(i64::MAX);
    assert_eq!(after_third[0].attempts, 3);
    assert_eq!(after_third[0].next_retry_at, first + 70);

    let immediate = outbox.due(first + 31);
    assert!(immediate.is_empty(), "backoff must hold the entry back");
}

#[test]
fn due_events_reads_completed_db_events() {
    let _guard = lock();
    let _root = setup();
    let conn = madrigal_db::open(&madrigal_paths::db_path()).expect("open db");
    let rules = Eligibility::default();

    let track = sample_track();
    madrigal_db::upsert_song(&conn, &track).expect("song");
    madrigal_db::record_event(
        &conn,
        &Event {
            track: track.identity.clone(),
            played_at: 1_700_000_000,
            duration_secs: Some(200),
            completed: true,
        },
    )
    .expect("event");
    madrigal_db::record_event(
        &conn,
        &Event {
            track: track.identity.clone(),
            played_at: 1_700_000_001,
            duration_secs: Some(10),
            completed: true,
        },
    )
    .expect("short event");
    madrigal_db::record_event(
        &conn,
        &Event {
            track: track.identity.clone(),
            played_at: 1_700_000_002,
            duration_secs: Some(200),
            completed: false,
        },
    )
    .expect("partial event");

    let due = due_events(&conn, &rules, 10);
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].played_at, 1_700_000_000);
}
