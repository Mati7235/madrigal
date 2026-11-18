use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use madrigal_settings::{Integrations, SponsorCategory};
use madrigal_sponsorblock::client::{Client, hash_prefix};
use madrigal_sponsorblock::{Category, SkipTracker, active_segment, parse};

const VIDEO: &str = "abc";
const OTHER: &str = "otro";

fn fixture() -> serde_json::Value {
    serde_json::json!([
        {"videoID": "ajeno", "segments": [
            {"segment": [0, 5], "category": "intro", "actionType": "skip", "UUID": "z"},
        ]},
        {"videoID": "abc", "segments": [
            {"segment": [30, 45], "category": "sponsor", "actionType": "skip", "UUID": "b", "votes": 4},
            {"segment": [0, 6.5], "category": "intro", "actionType": "skip", "UUID": "a"},
            {"segment": [50, 55], "category": "music_offtopic", "actionType": "mute", "UUID": "c"},
            {"segment": [60, 70], "category": "chapter", "actionType": "chapter", "UUID": "d"},
            {"segment": [80, 80], "category": "sponsor", "actionType": "skip", "UUID": "e"},
        ]},
    ])
}

#[test]
fn parse_keeps_matching_video_sorted() {
    let segments = parse(&fixture(), VIDEO);
    assert_eq!(
        segments.iter().map(|item| item.uuid.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
    assert_eq!(
        segments.iter().map(|item| item.start).collect::<Vec<_>>(),
        vec![0.0, 30.0, 50.0]
    );
    assert_eq!(segments[1].votes, 4);
    assert_eq!(segments[1].duration(), 15.0);
    assert_eq!(segments[2].label(), "Non-music section");
    assert!(segments[2].mutes());
    assert!(!segments[0].mutes());
    assert!(parse(&fixture(), OTHER).is_empty());
    assert!(parse(&serde_json::Value::Null, VIDEO).is_empty());
}

#[test]
fn active_segment_respects_boundaries() {
    let segments = parse(&fixture(), VIDEO);
    assert_eq!(active_segment(&segments, 3000).unwrap().uuid, "a");
    assert!(active_segment(&segments, 6500).is_none());
    assert!(active_segment(&segments, 20_000).is_none());
    assert_eq!(active_segment(&segments, 44_900).unwrap().uuid, "b");
    assert_eq!(active_segment(&segments, 51_000).unwrap().uuid, "c");
    assert!(active_segment(&segments, 90_000).is_none());
    assert!(active_segment(&Vec::new(), 3000).is_none());
}

#[test]
fn skip_tracker_reports_each_segment_once() {
    let segments = parse(&fixture(), VIDEO);
    let mut tracker = SkipTracker::new();
    assert_eq!(tracker.check(&segments, 3000).unwrap().uuid, "a");
    assert!(tracker.check(&segments, 3100).is_none());
    assert_eq!(tracker.check(&segments, 44_900).unwrap().uuid, "b");
    tracker.reset();
    assert_eq!(tracker.check(&segments, 3000).unwrap().uuid, "a");
}

#[test]
fn hash_prefix_matches_sha256() {
    assert_eq!(hash_prefix("abc"), "ba78");
    assert_eq!(hash_prefix("abc").len(), 4);
}

struct Logged {
    path: String,
    query: String,
}

fn serve(responses: Vec<(u16, String)>, log: Arc<Mutex<Vec<Logged>>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let count = responses.len();
    let queue = Arc::new(Mutex::new(responses));
    std::thread::spawn(move || {
        for stream in listener.incoming().take(count).flatten() {
            let mut stream = stream;
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                if stream.read(&mut byte).unwrap_or(0) == 0 {
                    break;
                }
                head.push(byte[0]);
                if head.len() > 8192 {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&head).into_owned();
            let target = text
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_owned();
            let (path, query) = match target.find('?') {
                Some(index) => (target[..index].to_owned(), target[index + 1..].to_owned()),
                None => (target, String::new()),
            };
            log.lock().unwrap().push(Logged { path, query });
            let (code, body) = queue.lock().unwrap().remove(0);
            let status = if code == 200 { "OK" } else { "Not Found" };
            let reply = format!(
                "HTTP/1.1 {code} {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn decode(encoded: &str) -> String {
    let bytes = encoded.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &encoded[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_value(query: &str, key: &str) -> String {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == key)
        .map(|(_, value)| decode(value))
        .unwrap_or_default()
}

#[test]
fn fetch_filters_categories_and_uses_custom_url() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let base = serve(
        vec![(
            200,
            serde_json::json!([
                {"videoID": "abc", "segments": [
                    {"segment": [0, 5], "category": "sponsor", "actionType": "skip", "UUID": "s1"},
                ]},
            ])
            .to_string(),
        )],
        log.clone(),
    );
    let integrations = Integrations {
        sponsorblock_api_url: base.clone(),
        sponsorblock_categories: vec![SponsorCategory::Sponsor],
        ..Integrations::default()
    };
    let client = Client::from_integrations(&integrations);
    let segments = client.fetch("abc").expect("fetch");
    assert_eq!(
        segments.iter().map(|item| item.uuid.as_str()).collect::<Vec<_>>(),
        vec!["s1"]
    );
    let logged = log.lock().unwrap();
    assert_eq!(logged.len(), 1);
    assert!(logged[0].path.ends_with(&hash_prefix("abc")));
    assert_eq!(query_value(&logged[0].query, "prefix"), "4");
    assert_eq!(query_value(&logged[0].query, "categories"), r#"["sponsor"]"#);
}

#[test]
fn fetch_empty_selection_skips_network() {
    let closed = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = closed.local_addr().expect("addr").port();
    drop(closed);
    let integrations = Integrations {
        sponsorblock_api_url: format!("http://127.0.0.1:{port}"),
        sponsorblock_categories: Vec::new(),
        ..Integrations::default()
    };
    let client = Client::from_integrations(&integrations);
    assert!(client.fetch("abc").expect("empty").is_empty());
    assert!(client.fetch("").expect("blank").is_empty());
    let off = Integrations {
        sponsorblock_enabled: false,
        ..Integrations::default()
    };
    assert!(Client::from_integrations(&off).fetch("abc").expect("off").is_empty());
}

#[test]
fn fetch_404_yields_empty() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let base = serve(vec![(404, String::new())], log);
    let client = Client::new(&base);
    assert!(client.fetch("zzzzzzzzzzz").expect("404").is_empty());
}

#[test]
fn category_roundtrip_covers_oracle_set() {
    let names = [
        "sponsor",
        "selfpromo",
        "interaction",
        "intro",
        "outro",
        "preview",
        "music_offtopic",
        "filler",
        "hook",
    ];
    for name in names {
        let parsed = Category::parse(name).expect("known");
        assert_eq!(parsed.as_str(), name);
    }
    assert!(Category::parse("chapter").is_none());
    assert_eq!(Category::DEFAULT.len(), 5);
    let stored = vec![
        SponsorCategory::Sponsor,
        SponsorCategory::Sponsor,
        SponsorCategory::Intro,
    ];
    let integrations = Integrations {
        sponsorblock_categories: stored,
        ..Integrations::default()
    };
    let client = Client::from_integrations(&integrations);
    assert_eq!(client.categories(), &[Category::Sponsor, Category::Intro]);
}
