use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use madrigal_core::{Track, TrackIdentity};
use madrigal_scrobble::lastfm::{self, LastFmClient};
use madrigal_scrobble::listenbrainz::ListenBrainzClient;
use madrigal_scrobble::maloja::MalojaClient;
use madrigal_scrobble::{md5, Error};

const TIMEOUT: Duration = Duration::from_secs(5);

struct Captured {
    request_line: String,
    headers: HashMap<String, String>,
    body: String,
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

fn find_headers_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4).position(|window| window == b"\r\n\r\n")
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
            let reason = if status == 200 { "OK" } else { "Error" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (url, rx)
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&input[index + 1..index + 3], 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).expect("utf8 form value")
}

fn form_map(body: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in body.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            map.insert(percent_decode(key), percent_decode(value));
        }
    }
    map
}

fn sample_track() -> Track {
    Track {
        identity: TrackIdentity::youtube("dQw4w9WgXcQ"),
        title: "Never Gonna Give You Up".to_owned(),
        artists: vec!["Rick Astley".to_owned()],
        album: Some("Whenever You Need Somebody".to_owned()),
        duration_secs: Some(212),
        thumbnail: None,
        track_number: Some(1),
        local_path: None,
        set_video_id: None,
    }
}

fn documented_sig(params: &HashMap<String, String>, secret: &str) -> String {
    let mut pairs: Vec<(&String, &String)> = params
        .iter()
        .filter(|(key, _)| key.as_str() != "api_sig" && key.as_str() != "format")
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    let mut raw = String::new();
    for (key, value) in pairs {
        raw.push_str(key);
        raw.push_str(value);
    }
    raw.push_str(secret);
    md5::hexdigest(raw.as_bytes())
}

#[test]
fn maloja_payload_paths_and_connection_probe() {
    let (url, rx) = serve(vec![(200, "{}"), (200, "{}"), (200, "{}")]);
    let client = MalojaClient::new(&url, "key-1").with_timeout(TIMEOUT);

    client.now_playing(&sample_track()).expect("now playing");
    let captured = rx.recv_timeout(TIMEOUT).expect("request");
    assert!(captured
        .request_line
        .starts_with("POST /apis/track/nowplaying "));
    let form = form_map(&captured.body);
    assert_eq!(form.get("key").map(String::as_str), Some("key-1"));
    assert_eq!(form.get("artist").map(String::as_str), Some("Rick Astley"));
    assert_eq!(
        form.get("title").map(String::as_str),
        Some("Never Gonna Give You Up")
    );
    assert_eq!(
        form.get("album").map(String::as_str),
        Some("Whenever You Need Somebody")
    );
    assert_eq!(form.get("length").map(String::as_str), Some("212"));

    client
        .scrobble(&sample_track(), 1_700_000_000)
        .expect("scrobble");
    let captured = rx.recv_timeout(TIMEOUT).expect("request");
    assert!(captured
        .request_line
        .starts_with("POST /apis/track/scrobble "));
    let form = form_map(&captured.body);
    assert_eq!(form.get("time").map(String::as_str), Some("1700000000"));

    client.test_connection().expect("probe");
    let captured = rx.recv_timeout(TIMEOUT).expect("request");
    assert!(captured
        .request_line
        .starts_with("POST /apis/track/nowplaying "));
    let form = form_map(&captured.body);
    assert_eq!(form.get("artist").map(String::as_str), Some("upstream"));
    assert_eq!(
        form.get("title").map(String::as_str),
        Some("Connection test")
    );
}

#[test]
fn maloja_rejects_unconfigured_and_plain_remote() {
    let blank = MalojaClient::new("", "").with_timeout(TIMEOUT);
    assert!(matches!(
        blank.now_playing(&sample_track()),
        Err(Error::NotConfigured(_))
    ));

    let remote = MalojaClient::new("https://example.invalid", "key-1").with_timeout(TIMEOUT);
    assert!(matches!(
        remote.now_playing(&sample_track()),
        Err(Error::HttpsUnsupported(_))
    ));

    let plain = MalojaClient::new("http://example.invalid", "key-1").with_timeout(TIMEOUT);
    assert!(matches!(
        plain.now_playing(&sample_track()),
        Err(Error::BadUrl(_))
    ));

    let (url, _rx) = serve(vec![(500, "boom")]);
    let failing = MalojaClient::new(&url, "key-1").with_timeout(TIMEOUT);
    assert!(matches!(
        failing.scrobble(&sample_track(), 1_700_000_000),
        Err(Error::HttpStatus(500))
    ));
}

#[test]
fn lastfm_signature_vector_and_endpoint_normalization() {
    let params = [
        ("method", "auth.getMobileSession"),
        ("api_key", "key123"),
        ("username", "tester"),
        ("password", "secret-pw"),
    ];
    let got = lastfm::api_sig(&params, "hunter2");
    assert_eq!(
        got, "ab00fe125bfd4841fbbc413f5e4f88e1",
        "api_sig must equal md5 of sorted key/value pairs plus secret, verified with openssl"
    );

    assert_eq!(
        lastfm::normalize_endpoint("https://ws.audioscrobbler.com/2.0/").unwrap(),
        "https://ws.audioscrobbler.com/2.0/"
    );
    assert_eq!(
        lastfm::normalize_endpoint("https://libre.fm").unwrap(),
        "https://libre.fm/2.0/"
    );
    assert_eq!(
        lastfm::normalize_endpoint("http://127.0.0.1:9/custom").unwrap(),
        "http://127.0.0.1:9/custom/"
    );
    assert!(lastfm::normalize_endpoint("https://example.invalid/?x=1").is_err());
    assert!(lastfm::normalize_endpoint("ftp://example.invalid/2.0/").is_err());
    assert!(lastfm::normalize_endpoint("not-a-url").is_err());
}

#[test]
fn lastfm_auth_scrobble_wire_format() {
    let (url, rx) = serve(vec![
        (200, r#"{"session":{"name":"tester","key":"sk-1"}}"#),
        (200, r#"{"scrobbles":{"@attr":{}}}"#),
        (200, r#"{"nowplaying":{}}}"#),
    ]);
    let mut client = LastFmClient::new(&url, "key123", "hunter2")
        .expect("client")
        .with_timeout(TIMEOUT);
    assert_eq!(client.endpoint(), format!("{url}/2.0/"));

    let key = client
        .get_mobile_session("tester", "secret-pw")
        .expect("session");
    assert_eq!(key, "sk-1");
    assert_eq!(client.session_key(), Some("sk-1"));
    let captured = rx.recv_timeout(TIMEOUT).expect("request");
    assert!(captured.request_line.starts_with("POST /2.0/ "));
    let form = form_map(&captured.body);
    assert_eq!(
        form.get("method").map(String::as_str),
        Some("auth.getMobileSession")
    );
    assert_eq!(form.get("username").map(String::as_str), Some("tester"));
    assert_eq!(form.get("password").map(String::as_str), Some("secret-pw"));
    assert_eq!(
        form.get("api_sig").map(String::as_str),
        Some(documented_sig(&form, "hunter2").as_str()),
        "transmitted api_sig must match the documented sorted-params md5 recomputed from the wire body"
    );

    client
        .scrobble(&sample_track(), 1_700_000_000)
        .expect("scrobble");
    let captured = rx.recv_timeout(TIMEOUT).expect("request");
    let form = form_map(&captured.body);
    assert_eq!(
        form.get("method").map(String::as_str),
        Some("track.scrobble")
    );
    assert_eq!(form.get("sk").map(String::as_str), Some("sk-1"));
    assert_eq!(
        form.get("artist[0]").map(String::as_str),
        Some("Rick Astley")
    );
    assert_eq!(
        form.get("track[0]").map(String::as_str),
        Some("Never Gonna Give You Up")
    );
    assert_eq!(
        form.get("timestamp[0]").map(String::as_str),
        Some("1700000000")
    );
    assert_eq!(
        form.get("album[0]").map(String::as_str),
        Some("Whenever You Need Somebody")
    );

    client
        .update_now_playing(&sample_track())
        .expect("now playing");
    let captured = rx.recv_timeout(TIMEOUT).expect("request");
    let form = form_map(&captured.body);
    assert_eq!(
        form.get("method").map(String::as_str),
        Some("track.updateNowPlaying")
    );
    assert_eq!(form.get("artist").map(String::as_str), Some("Rick Astley"));
    assert_eq!(
        form.get("track").map(String::as_str),
        Some("Never Gonna Give You Up")
    );
    assert_eq!(form.get("duration").map(String::as_str), Some("212"));
}

#[test]
fn lastfm_errors_and_missing_session() {
    let (url, _rx) = serve(vec![
        (200, r#"{"error":4,"message":"Authentication failed"}"#),
        (500, "boom"),
    ]);
    let mut client = LastFmClient::new(&url, "key123", "hunter2")
        .expect("client")
        .with_timeout(TIMEOUT);
    assert!(matches!(
        client.update_now_playing(&sample_track()),
        Err(Error::NotConfigured(_))
    ));
    let auth_err = client
        .get_mobile_session("tester", "wrong")
        .expect_err("last.fm error payload must surface");
    assert!(matches!(auth_err, Error::Api(4, _)), "got {auth_err:?}");
    let status_err = client
        .get_mobile_session("tester", "secret-pw")
        .expect_err("500 must surface as HttpStatus");
    assert!(
        matches!(status_err, Error::HttpStatus(500)),
        "got {status_err:?}"
    );
}

#[test]
fn listenbrainz_headers_and_payloads() {
    let (url, rx) = serve(vec![
        (200, r#"{"status":"ok"}"#),
        (200, r#"{"status":"ok"}"#),
    ]);
    let endpoint = format!("{url}/1/submit-listens");
    let client = ListenBrainzClient::new(&endpoint, "tok-1").with_timeout(TIMEOUT);

    client.now_playing(&sample_track()).expect("playing now");
    let captured = rx.recv_timeout(TIMEOUT).expect("request");
    assert!(captured.request_line.starts_with("POST /1/submit-listens "));
    assert_eq!(
        captured.headers.get("authorization").map(String::as_str),
        Some("Token tok-1")
    );
    let payload: serde_json::Value = serde_json::from_str(&captured.body).expect("json");
    assert_eq!(payload["listen_type"], "playing_now");
    let track = &payload["payload"][0]["track_metadata"];
    assert_eq!(track["artist_name"], "Rick Astley");
    assert_eq!(track["track_name"], "Never Gonna Give You Up");
    assert_eq!(track["release_name"], "Whenever You Need Somebody");
    assert_eq!(track["additional_info"]["duration_ms"], 212000);
    assert_eq!(track["additional_info"]["submission_client"], "Madrigal");

    client
        .submit_listen(&sample_track(), 1_700_000_000)
        .expect("single");
    let captured = rx.recv_timeout(TIMEOUT).expect("request");
    let payload: serde_json::Value = serde_json::from_str(&captured.body).expect("json");
    assert_eq!(payload["listen_type"], "single");
    assert_eq!(payload["payload"][0]["listened_at"], 1_700_000_000);
    assert_eq!(
        payload["payload"][0]["track_metadata"]["track_name"],
        "Never Gonna Give You Up"
    );

    let blank = ListenBrainzClient::new(&endpoint, "  ").with_timeout(TIMEOUT);
    assert!(matches!(
        blank.now_playing(&sample_track()),
        Err(Error::NotConfigured(_))
    ));
    let (down, _down_rx) = serve(vec![(500, "boom")]);
    let failing =
        ListenBrainzClient::new(&format!("{down}/1/submit-listens"), "tok-1").with_timeout(TIMEOUT);
    assert!(matches!(
        failing.now_playing(&sample_track()),
        Err(Error::HttpStatus(500))
    ));
}
