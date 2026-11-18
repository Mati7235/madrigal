use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use madrigal_shazam::{
    RecognitionOutcome, ShazamClient, Signature, SignatureGenerator, TIMEZONES, USER_AGENTS,
    parse_response_body, youtube_video_id,
};

const TIMEOUT: Duration = Duration::from_secs(10);
const MATCH_JSON: &str = include_str!("../fixtures/match.json");
const NO_MATCH_JSON: &str = include_str!("../fixtures/no_match.json");

struct Captured {
    request_line: String,
    headers: HashMap<String, String>,
    body: String,
}

fn find_headers_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4).position(|window| window == b"\r\n\r\n")
}

fn read_request(stream: &mut TcpStream) -> Captured {
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    let header_end = loop {
        let n = stream.read(&mut buf).expect("read request");
        if n == 0 {
            break None;
        }
        raw.extend_from_slice(&buf[..n]);
        if let Some(position) = find_headers_end(&raw) {
            break Some(position);
        }
        assert!(raw.len() < 1_000_000, "request too large");
    }
    .expect("complete headers");
    let head = String::from_utf8_lossy(&raw[..header_end]).into_owned();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default().to_owned();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_lowercase(), value.trim().to_owned());
        }
    }
    let content_length: usize = headers
        .get("content-length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    while raw.len() < header_end + 4 + content_length {
        let n = stream.read(&mut buf).expect("read body");
        assert!(n > 0, "body cut off");
        raw.extend_from_slice(&buf[..n]);
    }
    let body =
        String::from_utf8_lossy(&raw[header_end + 4..header_end + 4 + content_length]).into_owned();
    Captured {
        request_line,
        headers,
        body,
    }
}

fn serve(responses: Vec<(u16, &'static str)>) -> (String, Receiver<Captured>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock");
    let url = format!(
        "http://127.0.0.1:{}",
        listener.local_addr().expect("addr").port()
    );
    let (tx, rx): (Sender<Captured>, Receiver<Captured>) = channel();
    std::thread::spawn(move || {
        for (stream, (status, body)) in listener
            .incoming()
            .take(responses.len())
            .flatten()
            .zip(responses)
        {
            let mut stream = stream;
            let captured = read_request(&mut stream);
            let _ = tx.send(captured);
            let reason = match status {
                200 => "OK",
                404 => "Not Found",
                429 => "Too Many Requests",
                _ => "Error",
            };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (url, rx)
}

fn test_signature() -> Signature {
    let mut pcm = Vec::with_capacity(64_000);
    for i in 0..64_000 {
        let t = i as f64 / 16_000.0;
        let v = (2.0 * std::f64::consts::PI * 440.0 * t).sin() * 0.5
            + (2.0 * std::f64::consts::PI * 660.0 * t).sin() * 0.3
            + (2.0 * std::f64::consts::PI * 550.0 * (t + 2.0)).sin() * 0.2;
        pcm.push((v * 20000.0) as i16);
    }
    let mut gen = SignatureGenerator::new();
    gen.feed_pcm16_mono(&pcm);
    gen.next_signature().expect("synthetic mix yields")
}

fn fast_client(base_url: &str) -> ShazamClient {
    ShazamClient::with_limits(
        base_url,
        TIMEOUT,
        Duration::from_millis(0),
        3,
        Duration::from_millis(1),
    )
}

fn recv_with_timeout(rx: &Receiver<Captured>) -> Captured {
    rx.recv_timeout(TIMEOUT).expect("mock got request")
}

#[test]
fn match_shape_parses_to_recognition_with_links() {
    let (url, rx) = serve(vec![(200, MATCH_JSON)]);
    let client = fast_client(&url);
    let signature = test_signature();
    match client.recognize(&signature).expect("recognize ok") {
        RecognitionOutcome::Matched(found) => {
            assert_eq!(found.recognition.title, "Test Song");
            assert_eq!(found.recognition.artists, vec!["Test Artist"]);
            assert_eq!(found.recognition.album.as_deref(), Some("Test Album"));
            assert_eq!(found.recognition.score, 1.0);
            assert!(found.recognition.is_reliable());
            assert_eq!(
                found.recognition.track.as_ref().map(|t| t.to_storage_string()),
                Some("dQw4w9WgXcQ".to_owned())
            );
            assert_eq!(found.links.genre.as_deref(), Some("Pop"));
            assert_eq!(found.links.label.as_deref(), Some("Test Label"));
            assert_eq!(found.links.release_date.as_deref(), Some("2020"));
            assert_eq!(
                found.links.cover.as_deref(),
                Some("https://example.invalid/img/cover.jpg")
            );
            assert_eq!(
                found.links.cover_hq.as_deref(),
                Some("https://example.invalid/img/coverhq.jpg")
            );
            assert_eq!(
                found.links.shazam_url.as_deref(),
                Some("https://www.shazam.com/song/12345678/test-song")
            );
            assert_eq!(
                found.links.apple_music_url.as_deref(),
                Some("https://music.apple.com/song/123")
            );
            assert_eq!(
                found.links.spotify_url.as_deref(),
                Some("https://open.spotify.com/track/abc")
            );
            assert_eq!(found.links.isrc.as_deref(), Some("USRC10000001"));
            assert_eq!(
                found.links.youtube_video_id.as_deref(),
                Some("dQw4w9WgXcQ")
            );
            assert_eq!(
                found.links.lyrics,
                Some(vec!["first line".to_owned(), "second line".to_owned()])
            );

            let dir = tempfile::tempdir().expect("tempdir");
            let db = madrigal_db::open(&dir.path().join("lib.sqlite3")).expect("open db");
            madrigal_shazam::record_match(&db, &found.recognition).expect("record");
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM recognitions", [], |row| row.get(0))
                .expect("count");
            assert_eq!(count, 1);
        }
        RecognitionOutcome::NoMatch => panic!("fixture must match"),
    }

    let captured = recv_with_timeout(&rx);
    let (path, query) = captured
        .request_line
        .split_once(' ')
        .and_then(|(_, rest)| rest.split_once(' '))
        .map(|(target, _)| target)
        .and_then(|target| target.split_once('?'))
        .map(|(p, q)| (p.to_owned(), q.to_owned()))
        .expect("request target");
    assert!(path.starts_with("/discovery/v5/en/US/android/-/tag/"));
    assert!(path.len() > "/discovery/v5/en/US/android/-/tag/".len() + 70);
    for param in [
        "sync=true",
        "webv3=true",
        "sampling=true",
        "connected=",
        "shazamapiversion=v3",
        "sharehub=true",
        "video=v3",
    ] {
        assert!(query.contains(param), "missing {param} in {query}");
    }
    let agent = captured
        .headers
        .get("user-agent")
        .expect("user agent header");
    assert!(USER_AGENTS.contains(&agent.as_str()), "unknown agent {agent}");
    assert_eq!(
        captured.headers.get("content-language").map(String::as_str),
        Some("en_US")
    );
    assert!(
        captured
            .headers
            .get("content-type")
            .is_some_and(|v| v.contains("application/json")),
        "json content type"
    );
    let body: serde_json::Value = serde_json::from_str(&captured.body).expect("json body");
    assert_eq!(
        body.pointer("/signature/uri").and_then(|v| v.as_str()),
        Some(signature.uri.as_str())
    );
    assert!(
        body.pointer("/signature/samplems")
            .and_then(|v| v.as_u64())
            .is_some_and(|ms| ms > 0)
    );
    assert!(
        body.pointer("/timestamp")
            .and_then(|v| v.as_u64())
            .is_some_and(|ts| ts > 0)
    );
    assert!(
        body.pointer("/timezone")
            .and_then(|v| v.as_str())
            .is_some_and(|tz| TIMEZONES.contains(&tz))
    );
    assert!(body.pointer("/geolocation/latitude").is_some());
}

#[test]
fn empty_track_and_not_found_yield_no_match() {
    let (url, _) = serve(vec![(200, NO_MATCH_JSON)]);
    let client = fast_client(&url);
    assert!(matches!(
        client.recognize(&test_signature()).expect("ok"),
        RecognitionOutcome::NoMatch
    ));

    let (url, _) = serve(vec![(404, "")]);
    let client = fast_client(&url);
    assert!(matches!(
        client.recognize(&test_signature()).expect("ok"),
        RecognitionOutcome::NoMatch
    ));

    assert!(matches!(
        parse_response_body("{\"track\":null}").expect("ok"),
        RecognitionOutcome::NoMatch
    ));
}

#[test]
fn server_errors_retry_then_succeed() {
    let (url, rx) = serve(vec![(500, ""), (503, ""), (200, MATCH_JSON)]);
    let client = fast_client(&url);
    match client.recognize(&test_signature()).expect("ok") {
        RecognitionOutcome::Matched(found) => assert_eq!(found.recognition.title, "Test Song"),
        RecognitionOutcome::NoMatch => panic!("must match after retries"),
    }
    assert!(recv_with_timeout(&rx).request_line.starts_with("POST "));
    assert!(recv_with_timeout(&rx).request_line.starts_with("POST "));
    assert!(recv_with_timeout(&rx).request_line.starts_with("POST "));
}

#[test]
fn malformed_and_client_errors_fail() {
    let (url, _) = serve(vec![(200, "not json{{{"), (400, "bad")]);
    let client = fast_client(&url);
    assert!(matches!(
        client.recognize(&test_signature()),
        Err(madrigal_shazam::client::Error::Malformed(_))
    ));
    assert!(matches!(
        client.recognize(&test_signature()),
        Err(madrigal_shazam::client::Error::Status(400))
    ));
}

#[test]
fn minimum_interval_spaces_requests() {
    let (url, _) = serve(vec![(404, ""), (404, "")]);
    let client = ShazamClient::with_limits(
        &url,
        TIMEOUT,
        Duration::from_millis(150),
        1,
        Duration::from_millis(1),
    );
    let start = Instant::now();
    assert!(matches!(
        client.recognize(&test_signature()).expect("ok"),
        RecognitionOutcome::NoMatch
    ));
    assert!(matches!(
        client.recognize(&test_signature()).expect("ok"),
        RecognitionOutcome::NoMatch
    ));
    assert!(start.elapsed() >= Duration::from_millis(150));
}

#[test]
fn youtube_id_extraction_rules() {
    assert_eq!(
        youtube_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
        Some("dQw4w9WgXcQ".to_owned())
    );
    assert_eq!(
        youtube_video_id("https://youtu.be/dQw4w9WgXcQ"),
        Some("dQw4w9WgXcQ".to_owned())
    );
    assert_eq!(youtube_video_id("https://example.invalid/x"), None);
    assert_eq!(youtube_video_id("https://example.invalid/watch?v="), None);
}

#[test]
fn recognition_settings_defaults_and_clamps() {
    let defaults = madrigal_settings::Settings::default();
    assert!(!defaults.recognition.enabled);
    assert_eq!(defaults.recognition.capture_seconds, 8);
    assert_eq!(defaults.recognition.min_interval_ms, 1000);
    assert_eq!(defaults.recognition.max_attempts, 3);

    let mut edited = madrigal_settings::Settings::default();
    edited.recognition.capture_seconds = 99;
    edited.recognition.min_interval_ms = 1;
    edited.recognition.max_attempts = 99;
    let value = serde_json::to_value(&edited).expect("serialize");
    let mut back: madrigal_settings::Settings =
        serde_json::from_value(value).expect("deserialize");
    back.recognition.sanitize();
    assert_eq!(back.recognition.capture_seconds, 30);
    assert_eq!(back.recognition.min_interval_ms, 250);
    assert_eq!(back.recognition.max_attempts, 5);
}
