use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex, MutexGuard};

use madrigal_canvas::cache::CanvasCache;
use madrigal_canvas::spotify::Credentials as SpotifyCredentials;
use madrigal_canvas::{Canvas, CanvasEntry, CanvasSource, Context, Error, NoSpotifyAuth, SpotifyCanvasAuth};
use madrigal_core::{Track, TrackIdentity};

static PROFILE_LOCK: Mutex<()> = Mutex::new(());

fn lock_profile() -> MutexGuard<'static, ()> {
    PROFILE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct Logged {
    path: String,
    body: String,
    headers: HashMap<String, String>,
}

#[derive(Clone)]
struct Behavior {
    betterlyrics_alt: bool,
}

struct Mock {
    url: String,
    log: Arc<Mutex<Vec<Logged>>>,
    behavior: Arc<Mutex<Behavior>>,
}

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR"),
    );
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("fixture {name}"))
}

fn respond(stream: &mut std::net::TcpStream, code: u16, content_type: &str, body: &[u8]) {
    let status = match code {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Error",
    };
    let head = format!(
        "HTTP/1.1 {code} {status}\r\nContent-Length: {}\r\nContent-Type: {content_type}\r\nConnection: close\r\nSet-Cookie: sp_t=mock-device-1; Path=/\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn spotify_search_json() -> String {
    serde_json::json!({
        "data": {"searchV2": {"tracksV2": {"items": [
            {"item": {"_uri": "spotify:track:0VjIjW4GlUZAMYd2vXMi3b",
             "data": {"uri": "spotify:track:0VjIjW4GlUZAMYd2vXMi3b",
              "name": "Neon Skyline",
              "artists": {"items": [{"profile": {"name": "Aurora Field"}}]},
              "albumOfTrack": {"name": "Neon Skyline",
               "coverArt": {"sources": [
                {"url": "https://i.scdn.co/image/small", "width": 64},
                {"url": "https://i.scdn.co/image/big", "width": 640}]}}}}},
            {"item": {"uri": "spotify:track:AAAAAAAAAAAAAAAAAAAAAA",
             "data": {"uri": "spotify:track:AAAAAAAAAAAAAAAAAAAAAA",
              "name": "Neon Skyline",
              "artists": {"items": [{"profile": {"name": "Someone Else"}}]},
              "albumOfTrack": {"name": "Other", "coverArt": {"sources": []}}}}},
        ]}}},
    })
    .to_string()
}

fn push_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8 & 0x7F) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn push_field(out: &mut Vec<u8>, field: u32, bytes: &[u8]) {
    push_varint(out, (field as u64) << 3 | 2);
    push_varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

fn push_string(out: &mut Vec<u8>, field: u32, value: &str) {
    push_field(out, field, value.as_bytes());
}

fn canvaz_bytes(pairs: &[(&str, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (uri, url) in pairs {
        let mut inner = Vec::new();
        push_string(&mut inner, 2, url);
        push_string(&mut inner, 5, uri);
        push_field(&mut out, 1, &inner);
    }
    out
}

fn spotify_page() -> String {
    use std::fmt::Write as _;
    let raw = serde_json::json!({"clientVersion": "9.0.34.593"}).to_string();
    let mut encoded = String::new();
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i] as u32;
        let b1 = if i + 1 < bytes.len() { bytes[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < bytes.len() { bytes[i + 2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        let mut chars = [0u8; 4];
        chars[0] = ALPHABET[((n >> 18) & 63) as usize];
        chars[1] = ALPHABET[((n >> 12) & 63) as usize];
        chars[2] = ALPHABET[((n >> 6) & 63) as usize];
        chars[3] = ALPHABET[(n & 63) as usize];
        let remaining = bytes.len() - i;
        if remaining == 1 {
            let _ = write!(encoded, "{}{}==", chars[0] as char, chars[1] as char);
        } else if remaining == 2 {
            let _ = write!(
                encoded,
                "{}{}{}=",
                chars[0] as char, chars[1] as char, chars[2] as char
            );
        } else {
            for byte in chars {
                encoded.push(byte as char);
            }
        }
        i += 3;
    }
    format!("<html><script id=\"appServerConfig\" type=\"application/json\">{encoded}</script></html>")
}

fn mock() -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let log = Arc::new(Mutex::new(Vec::new()));
    let behavior = Arc::new(Mutex::new(Behavior {
        betterlyrics_alt: false,
    }));
    let thread_log = log.clone();
    let thread_behavior = behavior.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(128).flatten() {
            let mut stream = stream;
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                if stream.read(&mut byte).unwrap_or(0) == 0 {
                    break;
                }
                head.push(byte[0]);
                if head.len() > 32768 {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&head).into_owned();
            let mut lines = text.lines();
            let first = lines.next().unwrap_or_default().to_owned();
            let mut parts = first.split_whitespace();
            let method = parts.next().unwrap_or("").to_owned();
            let target = parts.next().unwrap_or("/").to_owned();
            let path = target.split('?').next().unwrap_or("/").to_owned();
            let mut length = 0;
            let mut headers = HashMap::new();
            for line in lines {
                if line.is_empty() {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    headers.insert(
                        name.trim().to_ascii_lowercase(),
                        value.trim().to_owned(),
                    );
                    if name.trim().eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
            }
            let mut body = vec![0u8; length];
            let _ = stream.read_exact(&mut body);
            thread_log.lock().unwrap().push(Logged {
                path: path.clone(),
                body: String::from_utf8_lossy(&body).into_owned(),
                headers: headers.clone(),
            });
            let alt = thread_behavior.lock().unwrap().betterlyrics_alt;
            if method == "GET" && path == "/v1/search" {
                respond(&mut stream, 200, "application/json", fixture("tidal_search_canvas.json").as_bytes());
            } else if method == "GET" && path == "/us/new" {
                respond(&mut stream, 200, "text/html", fixture("apple_token_page.html").as_bytes());
            } else if method == "GET" && path == "/v1/catalog/us/search" {
                respond(&mut stream, 200, "application/json", fixture("apple_search.json").as_bytes());
            } else if method == "GET" && path == "/v1/catalog/us/albums/1468058165" {
                respond(&mut stream, 200, "application/json", fixture("apple_album.json").as_bytes());
            } else if method == "GET" && path == "/artwork" {
                let mut body = fixture("betterlyrics.json");
                if alt {
                    body = body.replace("canvas-neon.mp4", "canvas-neon-v2.mp4");
                }
                respond(&mut stream, 200, "application/json", body.as_bytes());
            } else if method == "POST" && path == "/pathfinder/v2/query" {
                let asked = String::from_utf8_lossy(&body).into_owned();
                if asked.contains("hash-bad") {
                    respond(&mut stream, 200, "application/json", br#"{"errors":[{"message":"PersistedQueryNotFound"}]}"#);
                } else {
                    respond(&mut stream, 200, "application/json", spotify_search_json().as_bytes());
                }
            } else if method == "GET" && path == "/spotify-page" {
                respond(&mut stream, 200, "text/html", spotify_page().as_bytes());
            } else if method == "POST" && path == "/v1/clienttoken" {
                respond(&mut stream, 200, "application/json", br#"{"granted_token":{"token":"ctok-1","expires_after_seconds":3600}}"#);
            } else if method == "POST" && path == "/canvaz-cache/v0/canvases" {
                let payload = canvaz_bytes(&[
                    ("spotify:track:0VjIjW4GlUZAMYd2vXMi3b", "https://canvaz.scdn.co/upload/neon/test.mp4"),
                    ("spotify:track:AAAAAAAAAAAAAAAAAAAAAA", "https://example.com/not-canvas.mp4"),
                ]);
                respond(&mut stream, 200, "application/protobuf", &payload);
            } else {
                respond(&mut stream, 404, "text/plain", b"");
            }
        }
    });
    Mock {
        url: format!("http://127.0.0.1:{port}"),
        log,
        behavior,
    }
}

fn track(title: &str, artist: &str) -> Track {
    Track {
        identity: TrackIdentity::youtube("video-1"),
        title: title.to_owned(),
        artists: vec![artist.to_owned()],
        album: Some("Neon Skyline".to_owned()),
        duration_secs: Some(214),
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

fn tidal_token() -> madrigal_tidal::auth::TokenSet {
    madrigal_tidal::auth::TokenSet {
        token_type: "Bearer".to_owned(),
        access_token: "access-1".to_owned(),
        refresh_token: "refresh-1".to_owned(),
        session_id: "sess-1".to_owned(),
        country_code: "US".to_owned(),
        expires_at: madrigal_tidal::auth::now_epoch() + 3600,
    }
}

fn tidal_client(mock: &Mock) -> madrigal_tidal::auth::Client {
    let endpoints = madrigal_tidal::auth::Endpoints {
        device_authorization_url: format!("{}/v1/oauth2/device_authorization", mock.url),
        token_url: format!("{}/v1/oauth2/token", mock.url),
        api_base: format!("{}/", mock.url),
        scope: madrigal_tidal::auth::SCOPE.to_owned(),
    };
    madrigal_tidal::auth::Client::with_endpoints(
        madrigal_tidal::auth::Credentials::new("fixture-id", "fixture-secret"),
        endpoints,
    )
    .expect("client")
}

fn canvas_with(mock: &Mock) -> (tempfile::TempDir, Canvas<'_>) {
    let profile = tempfile::tempdir().expect("tempdir");
    std::env::set_var("MADRIGAL_PROFILE_ROOT", profile.path());
    let cache = CanvasCache::with_dir(profile.path().join("canvas")).expect("cache");
    let http = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .no_proxy()
        .build()
        .expect("http");
    let apple = madrigal_canvas::apple::Fetcher::with_bases(mock.url.clone(), mock.url.clone());
    let spotify = madrigal_canvas::spotify::Fetcher::offline();
    let ctx = Context {
        tidal: None,
        apple,
        better_lyrics_base: format!("{}/artwork", mock.url),
        spotify,
        http,
    };
    (profile, Canvas::new(ctx, cache))
}

fn count_path(mock: &Mock, path: &str) -> usize {
    mock.log
        .lock()
        .unwrap()
        .iter()
        .filter(|entry| entry.path == path)
        .count()
}

struct StubAuth(SpotifyCredentials);

impl SpotifyCanvasAuth for StubAuth {
    fn canvas_credentials(&self) -> Result<SpotifyCredentials, Error> {
        Ok(self.0.clone())
    }
}

#[test]
fn tidal_canvas_fetch_via_session() {
    let _guard = lock_profile();
    let profile = tempfile::tempdir().expect("tempdir");
    std::env::set_var("MADRIGAL_PROFILE_ROOT", profile.path());
    std::env::set_var("MADRIGAL_KEYRING", "0");
    let mock = mock();
    let client = tidal_client(&mock);
    let found = madrigal_canvas::tidal::fetch(&client, &tidal_token(), &track("Neon Skyline", "Aurora Field"))
        .expect("fetch")
        .expect("entry");
    assert_eq!(found.source, CanvasSource::Tidal);
    assert_eq!(
        found.video_url.as_deref(),
        Some("https://resources.tidal.com/videos/aaaaaaaa/bbbb/cccc/dddd/eeeeeeeeeeee/1280x1280.mp4")
    );
    assert_eq!(
        found.static_url.as_deref(),
        Some("https://resources.tidal.com/images/9f8e7d6c/5b4a/4321/fedc/ba9876543210/640x640.jpg")
    );
    assert_eq!(found.album_name.as_deref(), Some("Neon Skyline"));
    let missing = madrigal_canvas::tidal::fetch(&client, &tidal_token(), &track("Static Only", "Aurora Field"))
        .expect("fetch");
    assert!(missing.is_none());
    let unknown = madrigal_canvas::tidal::fetch(&client, &tidal_token(), &track("Nada", "Nadie"))
        .expect("fetch");
    assert!(unknown.is_none());
    let auth = mock
        .log
        .lock()
        .unwrap()
        .iter()
        .find(|entry| entry.path == "/v1/search")
        .and_then(|entry| entry.headers.get("authorization"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(auth, "Bearer access-1");
}

#[test]
fn apple_canvas_fetch_via_public_endpoint() {
    let _guard = lock_profile();
    let profile = tempfile::tempdir().expect("tempdir");
    std::env::set_var("MADRIGAL_PROFILE_ROOT", profile.path());
    let mock = mock();
    let http = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .no_proxy()
        .build()
        .expect("http");
    let fetcher = madrigal_canvas::apple::Fetcher::with_bases(mock.url.clone(), mock.url.clone());
    let found = fetcher
        .fetch(&http, &track("Cruel Summer", "Taylor Swift"))
        .expect("fetch")
        .expect("entry");
    assert_eq!(found.source, CanvasSource::AppleMusic);
    assert_eq!(found.album_id.as_deref(), Some("1468058165"));
    assert_eq!(
        found.animated.as_deref(),
        Some("https://mvod.itunes.apple.com/itunes-assets/HLSMusic116/v4/f9/b5/12/f9b512d4-4781-0214-81c8-13f12d9cee95/P643898319_default.m3u8")
    );
    assert_eq!(
        found.animated_vertical.as_deref(),
        Some("https://mvod.itunes.apple.com/itunes-assets/HLSMusic211/v4/f3/84/96/f384962a-8a99-ca3a-76ab-c28b28e63777/P1169970878_default.m3u8")
    );
    assert!(count_path(&mock, "/us/new") >= 1);
    let missing = fetcher
        .fetch(&http, &track("Nada Que Ver", "Nadie"))
        .expect("fetch");
    assert!(missing.is_none());
}

#[test]
fn betterlyrics_fetch_cache_hit_and_playback() {
    let _guard = lock_profile();
    let mock = mock();
    let (_profile, canvas) = canvas_with(&mock);
    let first = canvas
        .fetch(&track("Neon Skyline", "Aurora Field"), CanvasSource::BetterLyrics)
        .expect("fetch")
        .expect("entry");
    assert_eq!(
        first.preferred_animation_url(),
        Some("https://artwork.example/canvas-neon.mp4")
    );
    let second = canvas
        .fetch(&track("Neon Skyline", "Aurora Field"), CanvasSource::BetterLyrics)
        .expect("fetch")
        .expect("entry");
    assert_eq!(first, second);
    assert_eq!(count_path(&mock, "/artwork"), 1);
    let playback = canvas
        .resolve_playback(&track("Neon Skyline", "Aurora Field"), CanvasSource::BetterLyrics)
        .expect("playback");
    assert_eq!(playback.as_deref(), Some("https://artwork.example/canvas-neon.mp4"));
}

#[test]
fn refetch_invalidates_and_returns_fresh_entry() {
    let _guard = lock_profile();
    let mock = mock();
    let (_profile, canvas) = canvas_with(&mock);
    let first = canvas
        .fetch(&track("Neon Skyline", "Aurora Field"), CanvasSource::BetterLyrics)
        .expect("fetch")
        .expect("entry");
    assert!(first.preferred_animation_url().is_some_and(|url| !url.contains("v2")));
    mock.behavior.lock().unwrap().betterlyrics_alt = true;
    let fresh = canvas
        .refetch(&track("Neon Skyline", "Aurora Field"), CanvasSource::BetterLyrics)
        .expect("refetch")
        .expect("entry");
    assert_eq!(
        fresh.preferred_animation_url(),
        Some("https://artwork.example/canvas-neon-v2.mp4")
    );
    assert_eq!(count_path(&mock, "/artwork"), 2);
}

#[test]
fn spotify_seam_returns_unsupported() {
    let _guard = lock_profile();
    let auth = NoSpotifyAuth;
    let err = auth.canvas_credentials().expect_err("unsupported");
    assert!(matches!(err, Error::Unsupported(_)));
    let mock = mock();
    let (_profile, canvas) = canvas_with(&mock);
    let err = canvas
        .fetch(&track("Neon Skyline", "Aurora Field"), CanvasSource::Spotify)
        .expect_err("unsupported");
    assert!(matches!(err, Error::Unsupported(_)));
}

#[test]
fn spotify_flow_falls_back_across_hashes() {
    let _guard = lock_profile();
    let profile = tempfile::tempdir().expect("tempdir");
    std::env::set_var("MADRIGAL_PROFILE_ROOT", profile.path());
    let mock = mock();
    let http = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .no_proxy()
        .build()
        .expect("http");
    let creds = SpotifyCredentials {
        access_token: "sp-access".to_owned(),
        client_id: "sp-client".to_owned(),
        search_hashes: vec!["hash-bad".to_owned(), "hash-good".to_owned()],
    };
    let bases = madrigal_canvas::spotify::Bases {
        search_url: format!("{}/pathfinder/v2/query", mock.url),
        canvas_url: format!("{}/canvaz-cache/v0/canvases", mock.url),
        client_token_url: format!("{}/v1/clienttoken", mock.url),
        page_url: format!("{}/spotify-page", mock.url),
    };
    let found = madrigal_canvas::spotify::fetch(&http, &bases, &creds, &track("Neon Skyline", "Aurora Field"))
        .expect("fetch")
        .expect("entry");
    assert_eq!(found.source, CanvasSource::Spotify);
    assert_eq!(
        found.video_url.as_deref(),
        Some("https://canvaz.scdn.co/upload/neon/test.mp4")
    );
    assert_eq!(found.static_url.as_deref(), Some("https://i.scdn.co/image/big"));
    assert_eq!(count_path(&mock, "/pathfinder/v2/query"), 2);
    assert_eq!(count_path(&mock, "/canvaz-cache/v0/canvases"), 1);
    assert!(
        mock.log.lock().unwrap().iter().any(|entry| entry
            .path
            .contains("/pathfinder")
            && entry.body.contains("searchDesktop"))
    );
    let token_header = mock
        .log
        .lock()
        .unwrap()
        .iter()
        .find(|entry| entry.path == "/canvaz-cache/v0/canvases")
        .and_then(|entry| entry.headers.get("client-token"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(token_header, "ctok-1");
}

#[test]
fn spotify_stub_auth_wires_through_canvas() {
    let _guard = lock_profile();
    let profile = tempfile::tempdir().expect("tempdir");
    std::env::set_var("MADRIGAL_PROFILE_ROOT", profile.path());
    let mock = mock();
    let stub = StubAuth(SpotifyCredentials {
        access_token: "sp-access".to_owned(),
        client_id: "sp-client".to_owned(),
        search_hashes: vec!["hash-good".to_owned()],
    });
    let http = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .no_proxy()
        .build()
        .expect("http");
    let ctx = Context {
        tidal: None,
        apple: madrigal_canvas::apple::Fetcher::with_bases(mock.url.clone(), mock.url.clone()),
        better_lyrics_base: format!("{}/artwork", mock.url),
        spotify: madrigal_canvas::spotify::Fetcher {
            bases: madrigal_canvas::spotify::Bases {
                search_url: format!("{}/pathfinder/v2/query", mock.url),
                canvas_url: format!("{}/canvaz-cache/v0/canvases", mock.url),
                client_token_url: format!("{}/v1/clienttoken", mock.url),
                page_url: format!("{}/spotify-page", mock.url),
            },
            auth: Some(&stub),
        },
        http,
    };
    let cache = CanvasCache::with_dir(profile.path().join("canvas")).expect("cache");
    let canvas = Canvas::new(ctx, cache);
    let found = canvas
        .fetch(&track("Neon Skyline", "Aurora Field"), CanvasSource::Spotify)
        .expect("fetch")
        .expect("entry");
    assert_eq!(
        found.video_url.as_deref(),
        Some("https://canvaz.scdn.co/upload/neon/test.mp4")
    );
}

#[test]
fn spotify_codec_roundtrip_and_url_gate() {
    let _guard = lock_profile();
    assert!(madrigal_canvas::spotify::valid_track_uri("spotify:track:0VjIjW4GlUZAMYd2vXMi3b"));
    assert!(!madrigal_canvas::spotify::valid_track_uri("spotify:track:short"));
    assert!(madrigal_canvas::spotify::is_canvas_url("https://canvaz.scdn.co/upload/a/b.mp4"));
    assert!(!madrigal_canvas::spotify::is_canvas_url("https://example.com/not-canvas.mp4"));
    assert!(!madrigal_canvas::spotify::is_canvas_url("http://canvaz.scdn.co/upload/a/b.mp4"));
    let uri = "spotify:track:0VjIjW4GlUZAMYd2vXMi3b".to_owned();
    let url = "https://canvaz.scdn.co/upload/neon/test.mp4".to_owned();
    let encoded = madrigal_canvas::spotify::encode_request(std::slice::from_ref(&uri)).expect("encode");
    let decoded = madrigal_canvas::spotify::decode_response(&encoded).expect("decode");
    assert!(decoded.is_empty());
    let wrapped = canvaz_bytes(&[(uri.as_str(), url.as_str())]);
    let decoded = madrigal_canvas::spotify::decode_response(&wrapped).expect("decode");
    assert_eq!(decoded, vec![(uri, url)]);
    let oversized = vec![0u8; 1_048_577];
    assert!(madrigal_canvas::spotify::decode_response(&oversized).is_err());
}

#[test]
fn cache_expiry_prune_and_source_model() {
    let _guard = lock_profile();
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = CanvasCache::with_dir(dir.path().join("canvas")).expect("cache");
    let key = "tidal__tidal_4711__neon_skyline__aurora_field";
    let entry = CanvasEntry {
        source: CanvasSource::Tidal,
        name: Some("Neon Skyline".to_owned()),
        artist: Some("Aurora Field".to_owned()),
        album_id: None,
        album_name: Some("Neon Skyline".to_owned()),
        static_url: None,
        animated: None,
        animated_vertical: None,
        video_url: Some("https://resources.tidal.com/videos/a/1280x1280.mp4".to_owned()),
        video_url_vertical: None,
    };
    assert!(cache.store(key, &entry, 3600).expect("store"));
    assert!(cache.load(key).is_some());
    assert!(!cache.store("zero", &entry, 0).expect("store"));
    assert!(cache.load("zero").is_none());
    let stale = r#"{"entry":{"source":"TIDAL","video_url":"https://x/y.mp4"},"expires_at":1}"#;
    std::fs::write(cache.entry_path("stale"), stale).expect("stale");
    let limited = madrigal_settings::Storage {
        max_canvas_cache_mb: 0,
        ..Default::default()
    };
    std::fs::write(cache.dir().join("notes.txt"), b"keep me").expect("notes");
    let removed = cache.prune(&limited);
    assert!(removed >= 1);
    assert!(!cache.entry_path("stale").exists());
    assert!(cache.load(key).is_none());
    assert!(cache.dir().join("notes.txt").exists());
    let fresh = CanvasCache::with_dir(dir.path().join("canvas2")).expect("cache");
    assert!(fresh.store(key, &entry, 3600).expect("store"));
    let roomy = madrigal_settings::Storage::default();
    assert_eq!(fresh.prune(&roomy), 0);
    assert!(fresh.load(key).is_some());
    assert_eq!(CanvasSource::from_preference("BOTH"), CanvasSource::All);
    assert_eq!(CanvasSource::from_preference("TIDAL"), CanvasSource::Tidal);
    assert_eq!(CanvasSource::from_preference("nope"), CanvasSource::All);
    assert!(CanvasSource::All.accepts(CanvasSource::Tidal));
    assert!(!CanvasSource::Tidal.accepts(CanvasSource::Spotify));
    assert!(!CanvasSource::Tidal.accepts(CanvasSource::All));
    let with_feat = CanvasEntry {
        artist: Some("Aurora Field feat. Someone".to_owned()),
        name: Some("Neon Skyline (Official Video)".to_owned()),
        ..entry.clone()
    };
    assert!(with_feat.matches_identity("neon skyline", "aurora field"));
    assert!(!with_feat.matches_identity("other song", "aurora field"));
}
