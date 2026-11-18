use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use madrigal_net::{Config, RetryPolicy, SectionBudget, build_client, delay_for_attempt};

fn isolate() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (key, sub) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let path = dir.path().join(sub);
        std::fs::create_dir_all(&path).expect("xdg subdir");
        std::env::set_var(key, &path);
    }
    for key in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
    ] {
        std::env::remove_var(key);
    }
    std::mem::forget(dir);
}

fn fast_policy(max_attempts: u32) -> RetryPolicy {
    RetryPolicy::new(max_attempts, Duration::from_millis(1), Duration::from_millis(10), 0.5)
}

fn flaky_server(fail_first: usize, connections: usize) -> (String, Arc<AtomicUsize>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind flaky");
    let url = format!("http://{}/resource", listener.local_addr().expect("flaky addr"));
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&hits);
    let handle = std::thread::spawn(move || {
        for _ in 0..connections {
            let (mut stream, _) = listener.accept().expect("accept flaky");
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let hit = counter.fetch_add(1, Ordering::SeqCst) + 1;
            let (status, body) = if hit <= fail_first { (500, "boom") } else { (200, "ok") };
            let reason = if status == 200 { "OK" } else { "Internal Server Error" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).expect("write flaky");
            stream.flush().expect("flush flaky");
        }
    });
    (url, hits, handle)
}

fn fetch(url: &str) -> Result<String, String> {
    let client = build_client(
        &Config::new().with_timeout(Duration::from_secs(5)),
    )
    .map_err(|source| source.to_string())?;
    let response = client.get(url).send().map_err(|source| source.to_string())?;
    if !response.status().is_success() {
        return Err(format!("unexpected status {}", response.status()));
    }
    response.text().map_err(|source| source.to_string())
}

#[test]
fn failing_requests_retry_then_succeed() {
    isolate();
    let (url, hits, server) = flaky_server(2, 3);
    let mut budget = SectionBudget::new();
    let result = budget.run(&fast_policy(5), "metadata", || fetch(&url));
    assert_eq!(result.expect("eventual success"), "ok");
    assert_eq!(hits.load(Ordering::SeqCst), 3, "two failures plus one success");
    assert_eq!(budget.attempts("metadata"), 0, "success resets the section counter");
    server.join().expect("flaky thread");
}

#[test]
fn exhausted_budget_returns_the_underlying_error() {
    isolate();
    let (url, hits, server) = flaky_server(usize::MAX, 3);
    let mut budget = SectionBudget::new();
    let result = budget.run(&fast_policy(3), "metadata", || fetch(&url));
    let failure = result.expect_err("budget must exhaust");
    assert_eq!(failure.attempts, 3, "exactly max_attempts tries");
    assert!(failure.source.contains("500"), "underlying error preserved");
    assert_eq!(hits.load(Ordering::SeqCst), 3, "no tries beyond the budget");
    server.join().expect("failing thread");
}

#[test]
fn section_counters_are_independent() {
    isolate();
    let (url, hits, server) = flaky_server(usize::MAX, 6);
    let mut budget = SectionBudget::new();
    let policy = fast_policy(2);
    assert!(budget.run(&policy, "scrobble", || fetch(&url)).is_err());
    assert_eq!(budget.attempts("scrobble"), 2);
    assert_eq!(budget.attempts("lyrics"), 0, "untouched section keeps zero");
    assert!(budget.run(&policy, "lyrics", || fetch(&url)).is_err());
    assert_eq!(budget.attempts("lyrics"), 2, "second section gets a full budget");
    assert_eq!(hits.load(Ordering::SeqCst), 4, "two tries per section");
    assert!(budget.run(&policy, "scrobble", || fetch(&url)).is_err());
    assert_eq!(hits.load(Ordering::SeqCst), 6, "a new run gets a fresh budget");
    server.join().expect("section thread");
}

#[test]
fn backoff_grows_within_jitter_bounds() {
    isolate();
    let policy = RetryPolicy::new(10, Duration::from_millis(100), Duration::from_secs(10), 0.25);
    for attempt in 0..6u32 {
        let expected = 100.0 * 2f64.powi(attempt as i32);
        let delay = delay_for_attempt(&policy, attempt).as_secs_f64() * 1000.0;
        assert!(
            delay >= expected * 0.75 && delay <= expected * 1.25,
            "attempt {attempt}: {delay}ms outside ±25% of {expected}ms"
        );
    }
    let capped = delay_for_attempt(&policy, 100);
    assert!(capped <= Duration::from_secs(10), "delay capped at max_delay");
    assert!(capped >= Duration::from_secs(10).mul_f32(0.75), "capped delay keeps jitter floor");
}
