use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use madrigal_core::Track;

static PROFILE_LOCK: Mutex<()> = Mutex::new(());

fn lock_profile() -> MutexGuard<'static, ()> {
    PROFILE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&path).expect("read fixture")
}

fn innertube_fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("madrigal-innertube")
        .join("tests")
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&path).expect("read innertube fixture")
}

struct Logged {
    path: String,
    query: String,
    cookie_ok: bool,
    body: String,
}

struct Mock {
    url: String,
    log: Arc<Mutex<Vec<Logged>>>,
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
        "HTTP/1.1 {code} {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(reply.as_bytes());
}

fn mock() -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let log = Arc::new(Mutex::new(Vec::new()));
    let thread_log = log.clone();
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
                if head.len() > 65536 {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&head).into_owned();
            let mut lines = text.lines();
            let first = lines.next().unwrap_or_default().to_owned();
            let mut parts = first.split_whitespace();
            let method = parts.next().unwrap_or("").to_owned();
            let target = parts.next().unwrap_or("/").to_owned();
            let mut query = String::new();
            let mut path = target.clone();
            if let Some(index) = target.find('?') {
                path = target[..index].to_owned();
                query = target[index + 1..].to_owned();
            }
            let mut length = 0;
            let mut cookie_ok = false;
            for line in lines {
                if line.is_empty() {
                    break;
                }
                let lower = line.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
                if let Some(value) = lower.strip_prefix("cookie:") {
                    cookie_ok = value.contains("sp_dc=good-dc") && value.contains("sp_key=good-key");
                    if value.contains("sp_dc=bad-dc") {
                        cookie_ok = false;
                    }
                }
            }
            let mut body = vec![0u8; length];
            let _ = stream.read_exact(&mut body);
            let body_text = String::from_utf8_lossy(&body).into_owned();
            if method == "GET" && path == "/raw/nuances.json" {
                thread_log.lock().unwrap().push(Logged {
                    path,
                    query,
                    cookie_ok,
                    body: String::new(),
                });
                let raw = std::fs::read_to_string(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("tests")
                        .join("fixtures")
                        .join("nuances.json"),
                )
                .unwrap();
                respond(&mut stream, 200, &raw);
            } else if method == "GET" && path == "/api/server-time" {
                thread_log.lock().unwrap().push(Logged {
                    path,
                    query,
                    cookie_ok,
                    body: String::new(),
                });
                respond(&mut stream, 200, r#"{"serverTime":59}"#);
            } else if method == "GET" && path == "/api/token" {
                thread_log.lock().unwrap().push(Logged {
                    path,
                    query: query.clone(),
                    cookie_ok,
                    body: String::new(),
                });
                if !cookie_ok {
                    respond(
                        &mut stream,
                        200,
                        r#"{"accessToken":"","accessTokenExpirationTimestampMs":0,"isAnonymous":true,"clientId":""}"#,
                    );
                } else {
                    respond(
                        &mut stream,
                        200,
                        r#"{"accessToken":"fresh-access","accessTokenExpirationTimestampMs":1893456000000,"isAnonymous":false,"clientId":"fixture-client"}"#,
                    );
                }
            } else if method == "POST" && path == "/pathfinder/v2/query" {
                thread_log.lock().unwrap().push(Logged {
                    path,
                    query: String::new(),
                    cookie_ok,
                    body: body_text.clone(),
                });
                let payload: serde_json::Value =
                    serde_json::from_str(&body_text).unwrap_or(serde_json::Value::Null);
                let operation = payload
                    .get("operationName")
                    .and_then(|entry| entry.as_str())
                    .unwrap_or_default();
                let offset = payload
                    .get("variables")
                    .and_then(|entry| entry.get("offset"))
                    .and_then(|entry| entry.as_i64())
                    .unwrap_or(0);
                let limit = payload
                    .get("variables")
                    .and_then(|entry| entry.get("limit"))
                    .and_then(|entry| entry.as_i64())
                    .unwrap_or(25);
                let watch = payload
                    .get("variables")
                    .and_then(|entry| entry.get("enableWatchFeedEntrypoint"))
                    .and_then(|entry| entry.as_bool())
                    .unwrap_or(false);
                let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
                let file = if operation == "libraryV3" {
                    if offset == 0 {
                        "library_v3_page1.json"
                    } else {
                        "library_v3_page2.json"
                    }
                } else if operation == "fetchPlaylist" {
                    if limit == 1 {
                        ""
                    } else if watch {
                        "fetch_playlist_detail.json"
                    } else {
                        "fetch_playlist_tracks.json"
                    }
                } else if operation == "profileAttributes" {
                    "profile.json"
                } else {
                    ""
                };
                if file.is_empty() {
                    if operation == "fetchPlaylist" {
                        respond(
                            &mut stream,
                            200,
                            r#"{"data":{"playlistV2":{"content":{"totalCount":0,"items":[]}}}}"#,
                        );
                    } else {
                        respond(&mut stream, 404, r#"{"error":"unknown operation"}"#);
                    }
                } else {
                    let raw = std::fs::read_to_string(dir.join(file)).unwrap();
                    respond(&mut stream, 200, &raw);
                }
            } else if method == "GET" && path == "/v1/me" {
                thread_log.lock().unwrap().push(Logged {
                    path,
                    query,
                    cookie_ok,
                    body: String::new(),
                });
                respond(
                    &mut stream,
                    200,
                    r#"{"display_name":"Rest Fallback","images":[{"url":"https://i.scdn.example/rest.jpg"}]}"#,
                );
            } else {
                respond(&mut stream, 404, "");
            }
        }
    });
    Mock {
        url: format!("http://127.0.0.1:{port}"),
        log,
    }
}

fn auth_client(mock: &Mock) -> madrigal_spotify::AuthClient {
    madrigal_spotify::AuthClient::with_endpoints(madrigal_spotify::AuthEndpoints {
        nuance_url: format!("{}/raw/nuances.json", mock.url),
        server_time_url: format!("{}/api/server-time", mock.url),
        token_url: format!("{}/api/token", mock.url),
    })
    .expect("auth client")
}

fn gql_client(mock: &Mock) -> madrigal_spotify::GqlClient {
    madrigal_spotify::GqlClient::with_endpoints(madrigal_spotify::GqlEndpoints {
        query_url: format!("{}/pathfinder/v2/query", mock.url),
        rest_base: format!("{}/v1/", mock.url),
    })
    .expect("gql client")
}

fn setup_profile() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    std::env::set_var("MADRIGAL_KEYRING", "0");
    root
}

#[test]
fn totp_matches_rfc6238_vectors() {
    let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
    assert_eq!(madrigal_spotify::totp::generate(secret, 59), "287082");
    assert_eq!(madrigal_spotify::totp::generate(secret, 1111111109), "081804");
    assert_eq!(madrigal_spotify::totp::generate(secret, 1234567890), "005924");
    assert_eq!(madrigal_spotify::totp::generate(secret, 2000000000), "279037");
    assert_eq!(madrigal_spotify::totp::generate(secret, 20000000000), "353130");
}

#[test]
fn missing_cookies_rejected_without_network() {
    let client = madrigal_spotify::AuthClient::with_endpoints(madrigal_spotify::AuthEndpoints {
        nuance_url: "http://127.0.0.1:1/nuances".to_owned(),
        server_time_url: "http://127.0.0.1:1/time".to_owned(),
        token_url: "http://127.0.0.1:1/token".to_owned(),
    })
    .expect("client");
    let err = client
        .exchange(&madrigal_spotify::Cookies::new("", ""))
        .expect_err("empty sp_dc rejected");
    assert!(matches!(err, madrigal_spotify::Error::Auth(_)));
}

#[test]
fn token_exchange_posts_totp_and_stores_in_keyring() {
    let _guard = lock_profile();
    let _root = setup_profile();
    let mock = mock();
    let client = auth_client(&mock);
    let token = client
        .connect(&madrigal_spotify::Cookies::new("good-dc", "good-key"))
        .expect("exchange");
    assert_eq!(token.access_token, "fresh-access");
    let entries = mock.log.lock().unwrap();
    let token_call = entries
        .iter()
        .find(|entry| entry.path == "/api/token")
        .expect("token called");
    let expected = madrigal_spotify::totp::generate("JBSWY3DPEHPK3PXP", 59);
    assert!(token_call.query.contains(&format!("totp={expected}")));
    assert!(token_call.query.contains(&format!("totpServer={expected}")));
    assert!(token_call.query.contains("totpVer=7"));
    assert!(token_call.cookie_ok);
    assert!(entries.iter().all(|entry| !entry.body.contains("good-dc")));
    drop(entries);
    let stored = madrigal_secrets::get_secret("spotify")
        .expect("read secret")
        .expect("stored secret");
    assert!(stored.contains("fresh-access"));
    assert!(stored.contains("good-dc"));
    assert!(
        !madrigal_paths::account_profiles_dir()
            .join("secret_spotify.txt")
            .exists()
    );
    assert!(client.is_logged_in());
    client.logout();
    assert!(!client.is_logged_in());
    assert!(
        madrigal_secrets::get_secret("spotify")
            .expect("read secret")
            .is_none()
    );
}

#[test]
fn anonymous_token_means_invalid_cookie() {
    let _guard = lock_profile();
    let _root = setup_profile();
    let mock = mock();
    let client = auth_client(&mock);
    let err = client
        .connect(&madrigal_spotify::Cookies::new("bad-dc", ""))
        .expect_err("anonymous rejected");
    let text = format!("{err}");
    assert!(text.contains("invalid or expired"));
    assert!(!text.contains("bad-dc"));
}

#[test]
fn library_pagination_parses() {
    let (page, total) =
        madrigal_spotify::playlists::parse_library_page(&serde_json::from_str(&fixture(
            "library_v3_page1.json",
        ))
        .expect("json"))
        .expect("page1");
    assert_eq!(total, 3);
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].id, "pl_rock");
    assert_eq!(page[0].name, "Rock Classics");
    assert_eq!(page[0].owner_name.as_deref(), Some("Owner One"));
    assert_eq!(page[0].track_total, Some(2));
    assert_eq!(
        page[0].images.iter().find(|image| image.width == Some(300)).map(|image| image.url.as_str()),
        Some("https://i.scdn.example/rock-mid.jpg")
    );
    assert_eq!(page[1].id, "pl_jazz");
    let (tail, _) =
        madrigal_spotify::playlists::parse_library_page(&serde_json::from_str(&fixture(
            "library_v3_page2.json",
        ))
        .expect("json"))
        .expect("page2");
    assert_eq!(tail.len(), 1);
    assert_eq!(tail[0].id, "pl_empty");
    assert_eq!(tail[0].track_total, None);
    assert!(tail[0].images.is_empty());
}

#[test]
fn fetch_playlist_parses_tracks_and_local_flag() {
    let detail = madrigal_spotify::playlists::parse_playlist_detail(
        "pl_rock",
        &serde_json::from_str(&fixture("fetch_playlist_detail.json")).expect("json"),
    )
    .expect("detail");
    assert_eq!(detail.name, "Rock Classics");
    assert_eq!(detail.owner_name.as_deref(), Some("Owner One"));
    assert_eq!(detail.track_total, Some(3));
    let (items, total) = madrigal_spotify::playlists::parse_playlist_tracks(
        &serde_json::from_str(&fixture("fetch_playlist_tracks.json")).expect("json"),
    )
    .expect("tracks");
    assert_eq!(total, 3);
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].uid.as_deref(), Some("uid-aaa"));
    let first = items[0].track.as_ref().expect("track");
    assert_eq!(first.id, "trk_tomorrow");
    assert_eq!(first.artists[0].name, "The Beatles");
    assert_eq!(first.duration_ms, 178000);
    assert!(!first.is_local);
    assert_eq!(items[1].track.as_ref().expect("flat").duration_ms, 210000);
    assert!(items[2].track.as_ref().expect("local").is_local);
    assert!(!items[2].track.as_ref().expect("local").is_valid());
}

#[test]
fn mapper_resolves_beatles_through_offline_parse() {
    let source = innertube_fixture("search_songs_beatles.json");
    let candidates =
        madrigal_innertube::CatalogClient::parse_search_songs(&source).expect("parse songs");
    assert_eq!(candidates.len(), 20);
    let track = madrigal_spotify::SpotifyTrack {
        id: "trk_tomorrow".to_owned(),
        name: "Tomorrow Never Knows".to_owned(),
        artists: vec![madrigal_spotify::models::SpotifyArtistRef {
            id: "art_beatles".to_owned(),
            name: "The Beatles".to_owned(),
        }],
        album: Some(madrigal_spotify::models::SpotifyAlbumRef {
            id: "alb_revolver".to_owned(),
            name: "Revolver (Deluxe)".to_owned(),
            images: vec![madrigal_spotify::models::SpotifyImage {
                url: "https://i.scdn.example/revolver.jpg".to_owned(),
                height: Some(300),
                width: Some(300),
            }],
        }),
        duration_ms: 178000,
        is_local: false,
        explicit: false,
        uri: Some("spotify:track:trk_tomorrow".to_owned()),
    };
    assert_eq!(madrigal_spotify::build_query(&track), "The Beatles Tomorrow Never Knows");
    let (index, score) =
        madrigal_spotify::pick_best(&track, &candidates).expect("resolved");
    assert_eq!(candidates[index].identity.id.as_str(), "J45RU9Iu8PA");
    assert!(score >= 0.9);
    let merged = madrigal_spotify::apply_resolution(&track, &candidates[index]);
    assert_eq!(merged.identity.id.as_str(), "J45RU9Iu8PA");
    assert_eq!(merged.album.as_deref(), Some("Revolver (Deluxe)"));
    assert_eq!(merged.duration_secs, Some(178));
    assert_eq!(
        merged.thumbnail.as_deref(),
        Some("https://i.scdn.example/revolver.jpg")
    );
}

struct FixtureSource {
    detail: madrigal_spotify::SpotifyPlaylist,
    tracks: Vec<madrigal_spotify::models::SpotifyPlaylistItem>,
}

struct FixtureSearch {
    candidates: Vec<Track>,
    calls: std::sync::Mutex<usize>,
}

impl madrigal_spotify::SpotifySource for FixtureSource {
    fn playlist_detail(&self, id: &str) -> Result<madrigal_spotify::SpotifyPlaylist, madrigal_spotify::Error> {
        assert_eq!(id, "pl_rock");
        Ok(self.detail.clone())
    }

    fn playlist_tracks(&self, id: &str) -> Result<Vec<madrigal_spotify::SpotifyTrack>, madrigal_spotify::Error> {
        assert_eq!(id, "pl_rock");
        Ok(self.tracks.iter().filter_map(|item| item.track.clone()).collect())
    }
}

impl madrigal_spotify::Search for FixtureSearch {
    fn search_songs(&self, _query: &str) -> Result<Vec<Track>, madrigal_spotify::Error> {
        *self.calls.lock().unwrap() += 1;
        Ok(self.candidates.clone())
    }
}

fn noise_track() -> madrigal_spotify::SpotifyTrack {
    madrigal_spotify::SpotifyTrack {
        id: "trk_noise".to_owned(),
        name: "Xqzwrt Plugh Zzz".to_owned(),
        artists: vec![madrigal_spotify::models::SpotifyArtistRef {
            id: "art_nobody".to_owned(),
            name: "Nobody Ever".to_owned(),
        }],
        album: None,
        duration_ms: 999000,
        is_local: false,
        explicit: false,
        uri: None,
    }
}

fn fixture_source() -> FixtureSource {
    let detail = madrigal_spotify::playlists::parse_playlist_detail(
        "pl_rock",
        &serde_json::from_str(&fixture("fetch_playlist_detail.json")).expect("json"),
    )
    .expect("detail");
    let (mut items, _) = madrigal_spotify::playlists::parse_playlist_tracks(
        &serde_json::from_str(&fixture("fetch_playlist_tracks.json")).expect("json"),
    )
    .expect("tracks");
    items.push(madrigal_spotify::models::SpotifyPlaylistItem {
        uid: None,
        track: Some(noise_track()),
        is_local: false,
    });
    FixtureSource { detail, tracks: items }
}

#[test]
fn import_caches_mapping_and_reports_unresolved() {
    let _guard = lock_profile();
    let _root = setup_profile();
    let conn = rusqlite::Connection::open_in_memory().expect("memory db");
    let candidates = madrigal_innertube::CatalogClient::parse_search_songs(&innertube_fixture(
        "search_songs_beatles.json",
    ))
    .expect("parse songs");
    let search = FixtureSearch {
        candidates,
        calls: std::sync::Mutex::new(0),
    };
    let source = fixture_source();
    let seeded = Track {
        identity: madrigal_core::TrackIdentity::youtube("J45RU9Iu8PA"),
        title: "Tomorrow Never Knows (Mono)".to_owned(),
        artists: vec!["The Beatles".to_owned()],
        album: Some("Revolver (Deluxe)".to_owned()),
        duration_secs: Some(178),
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    };
    madrigal_spotify::store_mapping(&conn, "trk_tomorrow", &seeded).expect("seed");
    let outcome = madrigal_spotify::import_playlist(&source, &search, &conn, "pl_rock", true)
        .expect("import");
    assert_eq!(outcome.playlist.id, "pl_rock");
    assert!(outcome.resolved.iter().any(|track| track.identity.id.as_str() == "J45RU9Iu8PA"));
    assert!(outcome.unresolved.iter().any(|entry| entry.track.id == "trk_noise"));
    assert_eq!(outcome.resolved.len() + outcome.unresolved.len(), 3);
    assert_eq!(*search.calls.lock().unwrap(), 2);
    let cached = madrigal_spotify::lookup_mapping(&conn, "trk_tomorrow")
        .expect("lookup")
        .expect("cached");
    assert_eq!(cached.identity.id.as_str(), "J45RU9Iu8PA");
    assert!(matches!(
        madrigal_spotify::import_playlist(&source, &search, &conn, "pl_rock", false),
        Err(madrigal_spotify::Error::Disabled)
    ));
    assert!(matches!(
        madrigal_spotify::import_playlist(&source, &search, &conn, "  ", true),
        Err(madrigal_spotify::Error::Protocol(_))
    ));
}

#[test]
fn expired_token_refreshes_then_requires_relogin() {
    let _guard = lock_profile();
    let _root = setup_profile();
    let mock = mock();
    let client = auth_client(&mock);
    let stale = madrigal_spotify::Token {
        access_token: "stale-access".to_owned(),
        expires_at_ms: 1,
    };
    client
        .save_session(&madrigal_spotify::Cookies::new("good-dc", "good-key"), &stale)
        .expect("seed expired");
    let (_, fresh) = client.load_session().expect("refresh");
    assert_eq!(fresh.access_token, "fresh-access");
    client
        .save_session(&madrigal_spotify::Cookies::new("bad-dc", ""), &stale)
        .expect("seed bad");
    assert!(matches!(
        client.load_session(),
        Err(madrigal_spotify::Error::NotLoggedIn)
    ));
    let rejected = madrigal_spotify::Token {
        access_token: "fresh-access".to_owned(),
        expires_at_ms: 1893456000000,
    };
    assert!(matches!(
        client.refresh(&rejected),
        Err(madrigal_spotify::Error::NotLoggedIn)
    ));
}

#[test]
fn profile_reads_gql_name_and_avatar() {
    let _guard = lock_profile();
    let _root = setup_profile();
    let mock = mock();
    let gql = gql_client(&mock);
    let token = madrigal_spotify::Token {
        access_token: "fresh-access".to_owned(),
        expires_at_ms: 1893456000000,
    };
    let (name, avatar) = gql.profile(&token).expect("profile");
    assert_eq!(name, "Fixture Me");
    assert_eq!(avatar.as_deref(), Some("https://i.scdn.example/me.jpg"));
}
