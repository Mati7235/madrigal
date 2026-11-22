use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex, MutexGuard};

use madrigal_tidal::auth::{Client, Credentials, Endpoints, TokenSet};
use madrigal_tidal::{catalog, lyrics as tidal_lyrics, stream};

static PROFILE_LOCK: Mutex<()> = Mutex::new(());

fn lock_profile() -> MutexGuard<'static, ()> {
    PROFILE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct Logged {
    path: String,
    body: String,
}

#[derive(Clone)]
struct Behavior {
    pending_first: bool,
    refresh_ok: bool,
    sessions_ok: bool,
}

struct Mock {
    url: String,
    log: Arc<Mutex<Vec<Logged>>>,
    behavior: Arc<Mutex<Behavior>>,
}

fn link_json() -> String {
    serde_json::json!({
        "deviceCode": "dev-1",
        "userCode": "ABCD-EFGH",
        "verificationUri": "tidal.com/link",
        "verificationUriComplete": "tidal.com/link/ABCD-EFGH",
        "expiresIn": 30,
        "interval": 1,
    })
    .to_string()
}

fn token_json() -> String {
    serde_json::json!({
        "token_type": "Bearer",
        "access_token": "access-1",
        "refresh_token": "refresh-1",
        "expires_in": 3600,
    })
    .to_string()
}

fn refresh_json() -> String {
    serde_json::json!({
        "token_type": "Bearer",
        "access_token": "access-2",
        "expires_in": 3600,
    })
    .to_string()
}

fn sessions_json() -> String {
    serde_json::json!({"sessionId": "sess-1", "countryCode": "US", "userId": 7}).to_string()
}

fn search_json() -> String {
    serde_json::json!({
        "tracks": {"items": [
            {"id": 47, "title": "Tema", "duration": 180, "explicit": false,
             "popularity": 50, "allowStreaming": true, "streamReady": true,
             "stemReady": false, "djReady": false, "adSupportedStreamReady": false,
             "trackNumber": 1, "volumeNumber": 1,
             "artists": [{"id": 45, "name": "Banda"}],
             "artist": {"id": 45, "name": "Banda"},
             "album": {"id": 46, "title": "Disco", "cover": "ab12-cd34-ef56"}},
        ], "limit": 1, "offset": 0, "totalNumberOfItems": 1},
    })
    .to_string()
}

fn bts_manifest(urls: &[&str], encryption: &str, codec: &str) -> String {
    use base64::Engine as _;
    let raw = serde_json::json!({
        "urls": urls,
        "codecs": codec,
        "mimeType": "audio/flac",
        "encryptionType": encryption,
    })
    .to_string();
    base64::engine::general_purpose::STANDARD.encode(raw)
}

fn mpd_xml() -> String {
    [
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<MPD mediaPresentationDuration=\"PT3M5.5S\" type=\"static\">",
        "<Period><AdaptationSet contentType=\"audio\" mimeType=\"audio/mp4\">",
        "<Representation codecs=\"mp4a.40.2\" audioSamplingRate=\"44100\">",
        "<SegmentTemplate timescale=\"44100\" media=\"chunk-$Number$.m4s\" initialization=\"init.mp4\">",
        "<SegmentTimeline><S t=\"0\" d=\"441000\" r=\"5\"/><S d=\"220500\"/></SegmentTimeline>",
        "</SegmentTemplate></Representation></AdaptationSet></Period></MPD>",
    ]
    .join("")
}

fn playback_json(mime: &str, manifest_b64: &str, id: u64) -> String {
    serde_json::json!({
        "trackId": id,
        "audioQuality": "LOSSLESS",
        "manifestMimeType": mime,
        "manifest": manifest_b64,
        "bitDepth": 16,
        "sampleRate": 44100,
    })
    .to_string()
}

fn lyrics_json() -> String {
    serde_json::json!({
        "trackId": 47,
        "lyricsProvider": "Musixmatch",
        "providerCommontrackId": 111,
        "providerLyricsId": 222,
        "lyrics": "Primera\nSegunda",
        "subtitles": "[00:01.00]Línea una\n[00:05.00]Línea dos",
        "isRightToLeft": false,
    })
    .to_string()
}

fn respond(stream: &mut std::net::TcpStream, code: u16, body: &str) {
    let status = match code {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Error",
    };
    let reply = format!(
        "HTTP/1.1 {code} {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(reply.as_bytes());
}

fn mock(behavior: Behavior) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let log = Arc::new(Mutex::new(Vec::new()));
    let shared = Arc::new(Mutex::new(behavior));
    let thread_log = log.clone();
    let thread_behavior = shared.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(64).flatten() {
            let mut stream = stream;
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                if stream.read(&mut byte).unwrap_or(0) == 0 {
                    break;
                }
                head.push(byte[0]);
                if head.len() > 16384 {
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
            for line in lines {
                if line.is_empty() {
                    break;
                }
                if let Some(value) = line
                    .to_ascii_lowercase()
                    .strip_prefix("content-length:")
                {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; length];
            let _ = stream.read_exact(&mut body);
            let entry = Logged {
                path: path.clone(),
                body: String::from_utf8_lossy(&body).into_owned(),
            };
            thread_log.lock().unwrap().push(entry);
            let current = thread_behavior.lock().unwrap().clone();
            if method == "POST" && path == "/v1/oauth2/device_authorization" {
                respond(&mut stream, 200, &link_json());
            } else if method == "POST" && path == "/v1/oauth2/token" {
                let (is_refresh, polls) = {
                    let entries = thread_log.lock().unwrap();
                    let refresh = entries
                        .last()
                        .is_some_and(|entry| entry.body.contains("grant_type=refresh_token"));
                    let polls = entries
                        .iter()
                        .filter(|entry| {
                            entry.path == "/v1/oauth2/token"
                                && entry.body.contains("grant_type=urn")
                        })
                        .count();
                    (refresh, polls)
                };
                if is_refresh {
                    if current.refresh_ok {
                        respond(&mut stream, 200, &refresh_json());
                    } else {
                        respond(&mut stream, 400, r#"{"error":"invalid_grant"}"#);
                    }
                } else if current.pending_first && polls <= 1 {
                    respond(&mut stream, 400, r#"{"error":"authorization_pending"}"#);
                } else {
                    respond(&mut stream, 200, &token_json());
                }
            } else if method == "GET" && path == "/v1/sessions" {
                if current.sessions_ok {
                    respond(&mut stream, 200, &sessions_json());
                } else {
                    respond(&mut stream, 401, r#"{"error":"unauthorized"}"#);
                }
            } else if method == "GET" && path == "/v1/search" {
                respond(&mut stream, 200, &search_json());
            } else if path == "/v1/tracks/47/playbackinfopostpaywall" {
                respond(
                    &mut stream,
                    200,
                    &playback_json(
                        "application/vnd.tidal.bts",
                        &bts_manifest(&["https://audio.tidal.example/media-47.flac"], "", "FLAC"),
                        47,
                    ),
                );
            } else if path == "/v1/tracks/48/playbackinfopostpaywall" {
                use base64::Engine as _;
                let encoded = base64::engine::general_purpose::STANDARD.encode(mpd_xml());
                respond(&mut stream, 200, &playback_json("application/dash+xml", &encoded, 48));
            } else if path == "/v1/tracks/49/playbackinfopostpaywall" {
                respond(
                    &mut stream,
                    200,
                    &playback_json(
                        "application/vnd.tidal.bts",
                        &bts_manifest(&["https://audio.tidal.example/media-49.flac"], "AES-128", "FLAC"),
                        49,
                    ),
                );
            } else if path == "/v1/tracks/50/playbackinfopostpaywall" {
                respond(&mut stream, 404, "");
            } else if path == "/v1/tracks/47/lyrics" {
                respond(&mut stream, 200, &lyrics_json());
            } else {
                respond(&mut stream, 404, "");
            }
        }
    });
    Mock {
        url: format!("http://127.0.0.1:{port}"),
        log,
        behavior: shared,
    }
}

fn setup(behavior: Behavior) -> (tempfile::TempDir, Mock, Client) {
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    std::env::set_var("MADRIGAL_KEYRING", "0");
    let mock = mock(behavior);
    let endpoints = Endpoints {
        device_authorization_url: format!("{}/v1/oauth2/device_authorization", mock.url),
        token_url: format!("{}/v1/oauth2/token", mock.url),
        api_base: format!("{}/", mock.url),
        scope: madrigal_tidal::auth::SCOPE.to_owned(),
    };
    let client = Client::with_endpoints(
        Credentials::new("fixture-id", "fixture-secret"),
        endpoints,
    )
    .expect("client");
    (root, mock, client)
}

fn token() -> TokenSet {
    TokenSet {
        token_type: "Bearer".to_owned(),
        access_token: "access-1".to_owned(),
        refresh_token: "refresh-1".to_owned(),
        session_id: "sess-1".to_owned(),
        country_code: "US".to_owned(),
        expires_at: madrigal_tidal::auth::now_epoch() + 3600,
    }
}

#[test]
fn device_login_stores_token_in_keyring() {
    let _guard = lock_profile();
    let (_root, mock, client) = setup(Behavior {
        pending_first: true,
        refresh_ok: true,
        sessions_ok: true,
    });
    let emitted = Arc::new(Mutex::new(Vec::<(String, u64)>::new()));
    let captured = emitted.clone();
    let settled = client
        .authorize(move |url, expires| captured.lock().unwrap().push((url.to_owned(), expires)))
        .expect("authorize");
    assert_eq!(settled.access_token, "access-1");
    assert_eq!(settled.refresh_token, "refresh-1");
    assert_eq!(settled.session_id, "sess-1");
    let shown = emitted.lock().unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].0, "https://tidal.com/link/ABCD-EFGH");
    assert_eq!(shown[0].1, 30);
    let bodies: Vec<String> = mock
        .log
        .lock()
        .unwrap()
        .iter()
        .map(|entry| entry.body.clone())
        .collect();
    assert!(bodies.iter().any(|body| body.contains("grant_type=urn")));
    assert!(bodies.iter().all(|body| !body.contains("access-1")));
    let stored: serde_json::Value = serde_json::from_str(
        &madrigal_secrets::get_secret("tidal")
            .expect("read secret")
            .expect("stored secret"),
    )
    .expect("json");
    assert_eq!(stored["access_token"], "access-1");
    assert!(
        !madrigal_paths::account_profiles_dir()
            .join("secret_tidal.txt")
            .exists()
    );
    assert!(client.is_logged_in());
    client.logout();
    assert!(!client.is_logged_in());
    assert!(
        madrigal_secrets::get_secret("tidal")
            .expect("read secret")
            .is_none()
    );
}

#[test]
fn search_parses_tracks_with_tidal_source() {
    let _guard = lock_profile();
    let (_root, _mock, client) = setup(Behavior {
        pending_first: false,
        refresh_ok: true,
        sessions_ok: true,
    });
    client.save_session(&token()).expect("seed");
    let loaded = client.load_session().expect("session");
    assert_eq!(loaded.access_token, "access-1");
    let tracks = catalog::search_tracks(&client, &loaded, "Banda", 15).expect("search");
    assert_eq!(tracks.len(), 1);
    let track = &tracks[0];
    assert_eq!(track.identity.to_storage_string(), "tidal:47");
    assert!(track.is_remote());
    assert_eq!(track.title, "Tema");
    assert_eq!(track.artists, vec!["Banda"]);
    assert_eq!(track.album.as_deref(), Some("Disco"));
    assert_eq!(track.duration_secs, Some(180));
    assert_eq!(
        track.thumbnail.as_deref(),
        Some("https://resources.tidal.com/images/ab12/cd34/ef56/320x320.jpg")
    );
}

#[test]
fn stream_resolves_bts_url_and_mpd_file() {
    let _guard = lock_profile();
    let (_root, _mock, client) = setup(Behavior {
        pending_first: false,
        refresh_ok: true,
        sessions_ok: true,
    });
    let direct = stream::resolve_stream(&client, &token(), "tidal:47", None).expect("bts");
    assert_eq!(direct.kind, stream::ManifestKind::Bts);
    assert_eq!(direct.url, "https://audio.tidal.example/media-47.flac");
    assert_eq!(direct.track_id, "tidal:47");
    assert_eq!(direct.codec.as_deref(), Some("FLAC"));
    let built = stream::resolve_stream(&client, &token(), "48", None).expect("mpd");
    assert_eq!(built.kind, stream::ManifestKind::HlsFile);
    let playlist = std::fs::read_to_string(&built.url).expect("playlist");
    assert!(playlist.contains("#EXT-X-MAP:URI=\"chunk-0.m4s\""));
    assert!(playlist.contains("chunk-7.m4s"));
    assert!(playlist.contains("#EXT-X-ENDLIST"));
    assert!(stream::url_ok("https://audio.tidal.example/a.flac"));
    assert!(!stream::url_ok("http://audio.tidal.example/a.flac"));
    stream::cleanup_streams();
    assert!(!std::path::Path::new(&built.url).exists());
}

#[test]
fn stream_rejects_drm_missing_and_bad_id() {
    let _guard = lock_profile();
    let (_root, _mock, client) = setup(Behavior {
        pending_first: false,
        refresh_ok: true,
        sessions_ok: true,
    });
    assert!(matches!(
        stream::resolve_stream(&client, &token(), "tidal:49", None),
        Err(madrigal_tidal::Error::Drm)
    ));
    assert!(matches!(
        stream::resolve_stream(&client, &token(), "tidal:50", None),
        Err(madrigal_tidal::Error::Unavailable(_))
    ));
    assert!(matches!(
        stream::resolve_stream(&client, &token(), "tidal:song", None),
        Err(madrigal_tidal::Error::InvalidId(_))
    ));
    assert!(matches!(
        stream::upgrade_hls("#EXTM3U\n#EXT-X-MAP:URI=\"a\"\n").as_str(),
        "#EXTM3U\n#EXT-X-MAP:URI=\"a\"\n"
    ));
}

#[test]
fn lyrics_payload_converts_to_parseable_lyrics() {
    let _guard = lock_profile();
    let (_root, _mock, client) = setup(Behavior {
        pending_first: false,
        refresh_ok: true,
        sessions_ok: true,
    });
    let payload = tidal_lyrics::fetch_lyrics(&client, &token(), "tidal:47")
        .expect("lyrics")
        .expect("payload");
    assert_eq!(payload.provider.as_deref(), Some("Musixmatch"));
    let parsed = payload.to_lyrics().expect("parseable");
    assert!(parsed.synced);
    assert_eq!(parsed.lines.len(), 2);
    assert_eq!(parsed.lines[0].start_ms, 1000);
    assert!(tidal_lyrics::fetch_lyrics(&client, &token(), "tidal:99").expect("404").is_none());
    let empty = tidal_lyrics::LyricsPayload {
        subtitles: None,
        text: Some("   ".to_owned()),
        provider: None,
    };
    assert!(empty.to_lyrics().is_none());
    let plain = tidal_lyrics::LyricsPayload {
        subtitles: None,
        text: Some("Primera\nSegunda".to_owned()),
        provider: Some("Musixmatch".to_owned()),
    };
    let plain_parsed = plain.to_lyrics().expect("plain");
    assert!(!plain_parsed.synced);
    assert_eq!(plain_parsed.plain_text(), "Primera\nSegunda");
}

#[test]
fn expired_token_refreshes_then_requires_relogin() {
    let _guard = lock_profile();
    let (_root, mock, client) = setup(Behavior {
        pending_first: false,
        refresh_ok: true,
        sessions_ok: true,
    });
    let mut stale = token();
    stale.expires_at = 1;
    client.save_session(&stale).expect("seed expired");
    let fresh = client.load_session().expect("refresh");
    assert_eq!(fresh.access_token, "access-2");
    assert_eq!(fresh.refresh_token, "refresh-1");
    mock.behavior.lock().unwrap().refresh_ok = false;
    let mut gone = token();
    gone.expires_at = 1;
    gone.refresh_token = "stale".to_owned();
    client.save_session(&gone).expect("seed stale");
    assert!(matches!(
        client.load_session(),
        Err(madrigal_tidal::Error::NotLoggedIn)
    ));
}
