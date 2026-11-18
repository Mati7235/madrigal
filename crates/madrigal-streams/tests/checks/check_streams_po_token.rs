use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use madrigal_core::TrackIdentity;
use madrigal_streams::{Orchestrator, PoConfig, StreamConfig, po_token};

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

fn serve(body: &'static str) -> String {
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

fn free_port_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

#[test]
fn po_token_injected_and_degrades_when_down() {
    let (_root, shim, log, cache_dir) = setup();

    let base_url = serve(r#"{"poToken": "po-live-1", "visitorData": "vd-live-2"}"#);
    let fetched = po_token::fetch(
        &PoConfig {
            base_url: base_url.clone(),
            enabled: true,
            timeout_ms: 2000,
        },
        "POIDAAAAAA",
        Some("web_safari"),
    )
    .expect("token");
    assert_eq!(fetched.po_token, "po-live-1");
    assert_eq!(fetched.visitor_data, "vd-live-2");

    let down = PoConfig {
        base_url: free_port_url(),
        enabled: true,
        timeout_ms: 500,
    };
    assert!(po_token::fetch(&down, "POIDAAAAAA", None).is_none());
    let disabled = PoConfig {
        enabled: false,
        ..PoConfig::default()
    };
    assert!(po_token::fetch(&disabled, "POIDAAAAAA", None).is_none());

    let live = PoConfig {
        base_url: base_url.clone(),
        enabled: true,
        timeout_ms: 2000,
    };
    let config = StreamConfig {
        ytdlp_bin: shim.clone(),
        timeout_secs: 10,
        po: live,
        ..StreamConfig::default()
    };
    let client = Orchestrator::with_cache_dir(config, &cache_dir);
    let (stream, _) = client
        .resolve(&TrackIdentity::youtube("POIDAAAAAA"), None)
        .expect("resolve with token");
    assert!(stream.url.starts_with("https://"));
    let logged = fs::read_to_string(&log).unwrap();
    assert!(logged.contains("po-live-1"), "token in args: {logged}");
    assert!(logged.contains("vd-live-2"), "visitor data in args: {logged}");

    fs::write(&log, "").unwrap();
    let config2 = StreamConfig {
        ytdlp_bin: shim.clone(),
        timeout_secs: 10,
        po: down,
        ..StreamConfig::default()
    };
    let dir2 = cache_dir.join("down");
    fs::create_dir_all(&dir2).unwrap();
    let client2 = Orchestrator::with_cache_dir(config2, &dir2);
    let (stream2, _) = client2
        .resolve(&TrackIdentity::youtube("POIDBBBBBB"), None)
        .expect("resolve without provider");
    assert!(stream2.url.starts_with("https://"));
    let logged2 = fs::read_to_string(&log).unwrap();
    assert!(!logged2.contains("po-live-1"), "no token in args: {logged2}");
    assert!(!logged2.contains("bgutilhttp"), "no po flags: {logged2}");
}
