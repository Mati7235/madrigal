use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use madrigal_core::TrackIdentity;
use madrigal_streams::{Orchestrator, PoConfig, ResolveError, StreamConfig};
use madrigal_streams::cache::now_epoch;

fn setup() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
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
    let cache_dir = root.path().join("cache");
    fs::create_dir_all(&cache_dir).unwrap();
    (root, shim, log, cache_dir)
}

fn base_config(shim: &std::path::Path, po: PoConfig) -> StreamConfig {
    StreamConfig {
        ytdlp_bin: shim.to_path_buf(),
        timeout_secs: 10,
        po,
        ..StreamConfig::default()
    }
}

fn logged_args(log: &std::path::Path) -> Vec<String> {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn serve_tokens(body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        for mut stream in listener.incoming().take(8).flatten() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.0 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://127.0.0.1:{}", addr.port())
}

#[test]
fn orchestration_resolves_falls_back_and_backs_off() {
    let (_root, shim, log, cache_dir) = setup();
    let po = PoConfig {
        enabled: false,
        ..PoConfig::default()
    };
    let client = Orchestrator::with_cache_dir(base_config(&shim, po), &cache_dir);

    let identity = TrackIdentity::youtube("FAKEID123456");
    let (stream, path) = client.resolve(&identity, None).expect("resolve");
    assert_eq!(path, "default");
    assert!(stream.url.starts_with("https://"));
    assert_eq!(stream.resolution_path, "default");
    assert_eq!(stream.expires_at, Some(9_999_999_999.0));
    let lines = logged_args(&log);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("--format"));
    assert!(lines[0].contains("FAKEID123456"));

    env::set_var("FAKE_MODE", "fail_default");
    let identity2 = TrackIdentity::youtube("OTHERID9999");
    let (stream2, path2) = client.resolve(&identity2, None).expect("fallback");
    assert_eq!(path2, "web_safari");
    assert_eq!(stream2.client, "web_safari");
    let lines = logged_args(&log);
    assert!(
        lines.iter().any(|l| l.contains("player_client=web_safari")),
        "fallback attempt logged: {lines:?}"
    );

    env::set_var("FAKE_MODE", "rate_limited");
    let identity3 = TrackIdentity::youtube("FRESH429ID1");
    let first_delay = match client.resolve(&identity3, None) {
        Err(ResolveError::RateLimited { retry_after_secs }) => retry_after_secs,
        other => panic!("expected rate limited, got {other:?}"),
    };
    let second_delay = match client.resolve(&identity3, None) {
        Err(ResolveError::RateLimited { retry_after_secs }) => retry_after_secs,
        other => panic!("expected rate limited, got {other:?}"),
    };
    assert!(second_delay >= first_delay);
    assert_eq!(client.retry_delay_secs("FRESH429ID1"), Some(second_delay));

    let (cached, cached_path) = client.resolve(&identity, None).expect("cached");
    assert_eq!(cached_path, "cache");
    assert_eq!(cached.url, stream.url);
    assert!(client.cache().load("FAKEID123456", None, now_epoch()).is_some());

    let base_url = serve_tokens(r#"{"poToken": "po-abc-123", "visitorData": "vd-xyz-789"}"#);
    env::remove_var("FAKE_MODE");
    let po2 = PoConfig {
        base_url: base_url.clone(),
        enabled: true,
        timeout_ms: 2000,
    };
    let dir2 = cache_dir.join("po");
    fs::create_dir_all(&dir2).unwrap();
    let client2 = Orchestrator::with_cache_dir(base_config(&shim, po2), &dir2);
    let before = logged_args(&log).len();
    let identity4 = TrackIdentity::youtube("POTOKENID01");
    let (stream4, _) = client2.resolve(&identity4, None).expect("po resolve");
    assert!(stream4.url.starts_with("https://"));
    let after = logged_args(&log);
    assert!(after.len() > before);
    let tail = after[before..].join("\n");
    assert!(tail.contains("po-abc-123"), "po token in args: {tail}");
    assert!(tail.contains("vd-xyz-789"), "visitor data in args: {tail}");
    assert!(tail.contains(&base_url));
}
