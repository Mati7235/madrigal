use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use madrigal_core::{Track, TrackIdentity};
use madrigal_lyrics::chain::{self, ChainConfig, EndpointBases};
use madrigal_lyrics::providers::http_client;
use madrigal_settings::LyricsProvider;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("providers")
            .join(name),
    )
    .expect("read fixture")
}

type Handler = Arc<dyn Fn(&str) -> (u16, Vec<u8>) + Send + Sync>;

fn mock(handler: Handler) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let base = format!("http://127.0.0.1:{}/", listener.local_addr().expect("addr").port());
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().take(64).flatten() {
            counter.fetch_add(1, Ordering::SeqCst);
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        raw.extend_from_slice(&buf[..n]);
                        if raw.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                }
            }
            let text = String::from_utf8_lossy(&raw).into_owned();
            let target = text.lines().next().unwrap_or_default().to_owned();
            let (status, payload) = handler(&target);
            let reason = if status == 200 { "OK" } else { "Error" };
            let head = format!(
                "HTTP/1.0 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                payload.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&payload);
        }
    });
    (base, hits)
}

fn track() -> Track {
    Track {
        identity: TrackIdentity::youtube("CHAINTEST123"),
        title: "Yesterday".to_owned(),
        artists: vec!["The Beatles".to_owned()],
        album: None,
        duration_secs: Some(125),
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

fn lrclib_routes() -> Handler {
    let body = fixture("lrclib_search.json");
    Arc::new(move |_| (200, body.clone()))
}

fn config(lrclib: &str, kugou: &str) -> ChainConfig {
    ChainConfig {
        order: vec![LyricsProvider::Lrclib, LyricsProvider::Kugou],
        disabled: Vec::new(),
        paxsenix_api_key: String::new(),
        youtube_subtitle_enabled: false,
        bases: EndpointBases {
            lrclib: Some(lrclib.to_owned()),
            kugou_songs: Some(kugou.to_owned()),
            kugou_lyrics: Some(kugou.to_owned()),
            ..EndpointBases::default()
        },
    }
}

fn kugou_routes(songs: Vec<u8>, candidates: Vec<u8>, download: Vec<u8>) -> Handler {
    Arc::new(move |target: &str| {
        if target.contains("/api/v3/search/song") {
            (200, songs.clone())
        } else if target.contains("/download") {
            (200, download.clone())
        } else if target.contains("/search") {
            (200, candidates.clone())
        } else {
            (404, b"no".to_vec())
        }
    })
}

fn db() -> (tempfile::TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().expect("tempdir");
    let conn = madrigal_db::open(&dir.path().join("lyrics.db")).expect("open db");
    (dir, conn)
}

#[tokio::test]
async fn order_honored_first_hit_wins() {
    let (_dir, conn) = db();
    let (lrclib_base, _) = mock(lrclib_routes());
    let (kugou_base, kugou_hits) = mock(kugou_routes(
        fixture("kugou_songs.json"),
        fixture("kugou_candidates.json"),
        fixture("kugou_download.json"),
    ));
    let outcome = chain::fetch_track(&http_client(), &conn, &track(), &config(&lrclib_base, &kugou_base))
        .await
        .expect("fetch")
        .expect("hit");
    assert_eq!(outcome.provider, "lrclib");
    assert!(outcome.lyrics.synced);
    assert_eq!(kugou_hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn disabled_provider_skipped() {
    let (_dir, conn) = db();
    let (lrclib_base, lrclib_hits) = mock(lrclib_routes());
    let (kugou_base, _) = mock(kugou_routes(
        fixture("kugou_songs.json"),
        fixture("kugou_candidates.json"),
        fixture("kugou_download.json"),
    ));
    let mut cfg = config(&lrclib_base, &kugou_base);
    cfg.disabled = vec![LyricsProvider::Lrclib];
    let outcome = chain::fetch_track(&http_client(), &conn, &track(), &cfg)
        .await
        .expect("fetch")
        .expect("hit");
    assert_eq!(outcome.provider, "kugou");
    assert_eq!(lrclib_hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn failing_provider_does_not_abort_chain() {
    let (_dir, conn) = db();
    let (lrclib_base, _) = mock(Arc::new(|_| (500, b"boom".to_vec())));
    let (kugou_base, _) = mock(kugou_routes(
        fixture("kugou_songs.json"),
        fixture("kugou_candidates.json"),
        fixture("kugou_download.json"),
    ));
    let outcome = chain::fetch_track(&http_client(), &conn, &track(), &config(&lrclib_base, &kugou_base))
        .await
        .expect("fetch")
        .expect("hit");
    assert_eq!(outcome.provider, "kugou");
}

#[tokio::test]
async fn miss_is_cached_and_avoids_http() {
    let (_dir, conn) = db();
    let (lrclib_base, lrclib_hits) = mock(Arc::new(|_| (404, b"no".to_vec())));
    let (kugou_base, kugou_hits) = mock(Arc::new(|_| (404, b"no".to_vec())));
    let cfg = config(&lrclib_base, &kugou_base);
    assert!(
        chain::fetch_track(&http_client(), &conn, &track(), &cfg)
            .await
            .expect("fetch")
            .is_none()
    );
    let before = lrclib_hits.load(Ordering::SeqCst) + kugou_hits.load(Ordering::SeqCst);
    assert!(before > 0);
    assert!(
        chain::fetch_track(&http_client(), &conn, &track(), &cfg)
            .await
            .expect("fetch")
            .is_none()
    );
    assert_eq!(lrclib_hits.load(Ordering::SeqCst) + kugou_hits.load(Ordering::SeqCst), before);
}

#[tokio::test]
async fn hit_is_cached_and_avoids_http() {
    let (_dir, conn) = db();
    let (lrclib_base, lrclib_hits) = mock(lrclib_routes());
    let (kugou_base, _) = mock(kugou_routes(
        fixture("kugou_songs.json"),
        fixture("kugou_candidates.json"),
        fixture("kugou_download.json"),
    ));
    let cfg = config(&lrclib_base, &kugou_base);
    let first = chain::fetch_track(&http_client(), &conn, &track(), &cfg)
        .await
        .expect("fetch")
        .expect("hit");
    assert_eq!(first.provider, "lrclib");
    let before = lrclib_hits.load(Ordering::SeqCst);
    drop(cfg);
    let failing = ChainConfig {
        order: vec![LyricsProvider::Lrclib, LyricsProvider::Kugou],
        disabled: Vec::new(),
        paxsenix_api_key: String::new(),
        youtube_subtitle_enabled: false,
        bases: EndpointBases {
            lrclib: Some("http://127.0.0.1:1/".to_owned()),
            ..EndpointBases::default()
        },
    };
    let second = chain::fetch_track(&http_client(), &conn, &track(), &failing)
        .await
        .expect("fetch")
        .expect("cached hit");
    assert_eq!(second.provider, "cache");
    assert_eq!(madrigal_lyrics::to_lrc(&second.lyrics), madrigal_lyrics::to_lrc(&first.lyrics));
    assert_eq!(lrclib_hits.load(Ordering::SeqCst), before);
}

#[tokio::test]
async fn cache_roundtrip_preserves_sync_and_plain() {
    let synced = madrigal_lyrics::lrc::parse("[00:01.00]Hello\n[00:02.00]World\n");
    let encoded = chain::encode_cached(&synced);
    let decoded = chain::decode_cached(&encoded).expect("decode");
    assert!(decoded.synced);
    assert_eq!(decoded.plain_text(), "Hello\nWorld");
    let plain = madrigal_lyrics::Lyrics {
        lines: vec![madrigal_lyrics::LyricLine {
            start_ms: 0,
            end_ms: None,
            text: "Just text".to_owned(),
            words: None,
            markers: Default::default(),
        }],
        synced: false,
    };
    let decoded_plain = chain::decode_cached(&chain::encode_cached(&plain)).expect("decode");
    assert!(!decoded_plain.synced);
    assert_eq!(decoded_plain.plain_text(), "Just text");
    assert!(chain::decode_cached("v1:miss").is_none());
    let instrumental = chain::decode_cached("v1:txt\n").expect("decode");
    assert!(instrumental.lines.is_empty());
}

#[tokio::test]
async fn tidal_track_uses_seam_then_chain() {
    let (_dir, conn) = db();
    let (lrclib_base, _) = mock(lrclib_routes());
    let cfg = ChainConfig {
        order: vec![LyricsProvider::Lrclib],
        disabled: Vec::new(),
        paxsenix_api_key: String::new(),
        youtube_subtitle_enabled: false,
        bases: EndpointBases {
            lrclib: Some(lrclib_base),
            ..EndpointBases::default()
        },
    };
    let mut tidal_track = track();
    tidal_track.identity = TrackIdentity::tidal("TIDAL123");
    tidal_track.duration_secs = Some(125);
    let outcome = chain::fetch_track(&http_client(), &conn, &tidal_track, &cfg)
        .await
        .expect("fetch")
        .expect("hit");
    assert_eq!(outcome.provider, "lrclib");
    let plan = chain::plan(&cfg, madrigal_core::TrackSource::Tidal);
    assert_eq!(plan[0], chain::ChainProvider::Tidal);
    assert!(!plan.contains(&chain::ChainProvider::Youtube));
}
