use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use madrigal_ai::mixes::{
    collect_mix_seeds, parse_track_list, request_suggestions, request_top_mix, MixSeeds, SeedTrack,
};
use madrigal_ai::providers::{
    AiConfig, ChatModel, ChatRequest, GeminiChat, ModelOption, OpenAiChat, chat_model,
    list_models,
};
use madrigal_ai::translation::AiTranslationProvider;
use madrigal_ai::validate::validate;
use madrigal_ai::{Error, GEMINI_BASE, OPENAI_BASE, OPENROUTER_BASE};
use madrigal_core::{Event, Track, TrackIdentity};
use madrigal_lyrics::translation::TranslationProvider;
use madrigal_settings::{Ai, AiProvider, Settings, TopRange};

const TIMEOUT: Duration = Duration::from_secs(5);
const FAKE_KEY: &str = "fake-test-key-1";

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

fn serve(responses: Vec<(u16, String)>) -> (String, Receiver<Captured>) {
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
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (url, rx)
}

fn chat_completion(content: &str) -> String {
    serde_json::json!({
        "choices": [{"message": {"content": content}}]
    })
    .to_string()
}

fn gemini_completion(content: &str) -> String {
    serde_json::json!({
        "candidates": [{"content": {"parts": [{"text": content}]}}]
    })
    .to_string()
}

fn settings_for(provider: AiProvider, endpoint: &str, model: &str) -> Ai {
    Ai {
        provider,
        custom_endpoint: endpoint.to_owned(),
        api_key: FAKE_KEY.to_owned(),
        validation_status: madrigal_settings::AiValidationStatus::Unknown,
        selected_model: model.to_owned(),
        custom_model: String::new(),
        custom_prompt: String::new(),
    }
}

fn body_json(body: &str) -> serde_json::Value {
    serde_json::from_str(body).expect("request body is json")
}

#[test]
fn openai_request_shape_auth_model_and_parse() {
    let (url, rx) = serve(vec![(200, chat_completion("Hello world"))]);
    let endpoint = format!("{url}/v1/chat/completions");
    let model = OpenAiChat::new(&endpoint, FAKE_KEY, "gpt-4o-test").with_timeout(TIMEOUT);
    let text = model
        .complete(&ChatRequest::new("sys-prompt", "user-prompt", 0.2, 128))
        .expect("complete");
    assert_eq!(text, "Hello world");
    let captured = rx.recv_timeout(TIMEOUT).expect("request");
    assert!(captured.request_line.starts_with("POST /v1/chat/completions "));
    assert_eq!(
        captured.headers.get("authorization").map(String::as_str),
        Some("Bearer fake-test-key-1")
    );
    let body = body_json(&captured.body);
    assert_eq!(body["model"], "gpt-4o-test");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][0]["content"], "sys-prompt");
    assert_eq!(body["messages"][1]["content"], "user-prompt");
    assert!((body["temperature"].as_f64().expect("temp") - 0.2).abs() < 1e-6);
    assert_eq!(body["max_tokens"], 128);
}

#[test]
fn production_endpoints_and_defaults_resolve_from_settings() {
    let chatgpt = AiConfig::from_settings(&settings_for(AiProvider::Chatgpt, "", "")).expect("cfg");
    assert_eq!(
        chatgpt.endpoint,
        format!("{OPENAI_BASE}/chat/completions")
    );
    assert_eq!(chatgpt.model, "gpt-4o");
    let router =
        AiConfig::from_settings(&settings_for(AiProvider::Openrouter, "", "")).expect("cfg");
    assert_eq!(
        router.endpoint,
        format!("{OPENROUTER_BASE}/chat/completions")
    );
    assert_eq!(router.model, "~openai/gpt-latest");
    let gemini = AiConfig::from_settings(&settings_for(AiProvider::Gemini, "", "")).expect("cfg");
    assert_eq!(gemini.endpoint, GEMINI_BASE);
    assert_eq!(gemini.model, "gemini-3.5-flash");
}

#[test]
fn custom_endpoint_and_model_are_honored() {
    let (url, rx) = serve(vec![(200, chat_completion("custom ok"))]);
    let endpoint = format!("{url}/v1/custom/chat");
    let mut ai = settings_for(AiProvider::Custom, &endpoint, "");
    ai.custom_model = "my-model".to_owned();
    let config = AiConfig::from_settings(&ai).expect("config");
    assert_eq!(config.endpoint, endpoint);
    assert_eq!(config.model, "my-model");
    let text = chat_model(&config)
        .complete(&ChatRequest::new("s", "u", 0.0, 8))
        .expect("complete");
    assert_eq!(text, "custom ok");
    let captured = rx.recv_timeout(TIMEOUT).expect("request");
    assert!(captured.request_line.starts_with("POST /v1/custom/chat "));
    assert_eq!(body_json(&captured.body)["model"], "my-model");
}

#[test]
fn disabled_missing_key_and_custom_gaps_reject_config() {
    assert!(matches!(
        AiConfig::from_settings(&settings_for(AiProvider::None, "", "")),
        Err(Error::NotConfigured(_))
    ));
    let mut ai = settings_for(AiProvider::Chatgpt, "", "");
    ai.api_key = "   ".to_owned();
    assert!(matches!(
        AiConfig::from_settings(&ai),
        Err(Error::NotConfigured(_))
    ));
    let missing_endpoint = settings_for(AiProvider::Custom, "", "");
    assert!(matches!(
        AiConfig::from_settings(&missing_endpoint),
        Err(Error::NotConfigured(_))
    ));
    let mut missing_model = settings_for(AiProvider::Custom, "http://x/y", "");
    missing_model.custom_model = String::new();
    assert!(matches!(
        AiConfig::from_settings(&missing_model),
        Err(Error::NotConfigured(_))
    ));
}

#[test]
fn gemini_schema_uses_query_key_and_parts_parse() {
    let (url, rx) = serve(vec![(200, gemini_completion("Gemini says hi"))]);
    let base = format!("{url}/v1beta");
    let model = GeminiChat::new(&base, FAKE_KEY, "gem-test").with_timeout(TIMEOUT);
    let text = model
        .complete(&ChatRequest::new("sys-part", "user-part", 0.5, 64))
        .expect("complete");
    assert_eq!(text, "Gemini says hi");
    let captured = rx.recv_timeout(TIMEOUT).expect("request");
    assert!(captured
        .request_line
        .starts_with("POST /v1beta/models/gem-test:generateContent?key=fake-test-key-1 "));
    let body = body_json(&captured.body);
    let sent = body["contents"][0]["parts"][0]["text"]
        .as_str()
        .expect("text part");
    assert!(sent.contains("sys-part") && sent.contains("user-part"));
    assert_eq!(body["generationConfig"]["maxOutputTokens"], 64);
}

#[test]
fn validation_flow_success_and_failure() {
    let (url, rx) = serve(vec![(200, chat_completion("OK"))]);
    let ok = OpenAiChat::new(&format!("{url}/chat"), FAKE_KEY, "m").with_timeout(TIMEOUT);
    validate(&ok).expect("valid test response");
    assert!(rx.recv_timeout(TIMEOUT).is_ok());

    let failure = serde_json::json!({"error": {"message": "invalid key"}}).to_string();
    let (url, _) = serve(vec![(401, failure)]);
    let bad = OpenAiChat::new(&format!("{url}/chat"), FAKE_KEY, "m").with_timeout(TIMEOUT);
    let error = validate(&bad).expect_err("invalid key fails");
    let shown = error.to_string();
    assert!(shown.contains("401") && shown.contains("invalid key"));
    assert!(!shown.contains(FAKE_KEY));

    let (url, _) = serve(vec![(200, chat_completion("Sure, I am ready to help"))]);
    let off_script = OpenAiChat::new(&format!("{url}/chat"), FAKE_KEY, "m").with_timeout(TIMEOUT);
    assert!(matches!(
        validate(&off_script),
        Err(Error::Unparseable(_))
    ));
}

#[test]
fn model_listing_shapes_per_provider() {
    let listed = serde_json::json!({"data": [{"id": "b"}, {"id": "a"}, {"id": ""}] }).to_string();
    let (url, rx) = serve(vec![(200, listed)]);
    let config = AiConfig {
        provider: AiProvider::Chatgpt,
        endpoint: format!("{url}/chat/completions"),
        api_key: FAKE_KEY.to_owned(),
        model: "m".to_owned(),
    };
    let models = list_models(&config).expect("models");
    assert_eq!(
        models,
        vec![
            ModelOption {
                id: "a".to_owned(),
                display: "a".to_owned(),
            },
            ModelOption {
                id: "b".to_owned(),
                display: "b".to_owned(),
            },
        ]
    );
    let captured = rx.recv_timeout(TIMEOUT).expect("request");
    assert!(captured.request_line.starts_with("GET /models "));
    assert_eq!(
        captured.headers.get("authorization").map(String::as_str),
        Some("Bearer fake-test-key-1")
    );

    let gemini = serde_json::json!({"models": [
        {"name": "models/gem-x", "displayName": "Gem X", "supportedGenerationMethods": ["generateContent"]},
        {"name": "models/nope", "supportedGenerationMethods": ["embedContent"]},
    ]})
    .to_string();
    let (url, _) = serve(vec![(200, gemini)]);
    let config = AiConfig {
        provider: AiProvider::Gemini,
        endpoint: format!("{url}/v1beta"),
        api_key: FAKE_KEY.to_owned(),
        model: "m".to_owned(),
    };
    let models = list_models(&config).expect("gemini models");
    assert_eq!(
        models,
        vec![ModelOption {
            id: "gem-x".to_owned(),
            display: "Gem X".to_owned(),
        }]
    );

    let custom = AiConfig {
        provider: AiProvider::Custom,
        endpoint: format!("{url}/chat"),
        api_key: FAKE_KEY.to_owned(),
        model: "m".to_owned(),
    };
    assert!(list_models(&custom).expect("custom").is_empty());
}

struct Scripted {
    replies: Vec<String>,
    seen: std::sync::Mutex<Vec<ChatRequest>>,
}

impl Scripted {
    fn once(reply: &str) -> Self {
        Self {
            replies: vec![reply.to_owned()],
            seen: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl ChatModel for Scripted {
    fn complete(&self, request: &ChatRequest) -> Result<String, Error> {
        self.seen.lock().expect("seen").push(request.clone());
        self.replies
            .first()
            .cloned()
            .ok_or(Error::EmptyResponse)
    }
}

#[test]
fn translation_success_fences_and_custom_prompt() {
    let scripted = Scripted::once("```json\n[\"Hola\", \"Mundo\"]\n```");
    let provider = AiTranslationProvider::new(&scripted, "Keep slang casual.");
    assert_eq!(provider.name(), "ai");
    let lines = vec!["Hello".to_owned(), "World".to_owned()];
    let translated = provider
        .translate_lines(&lines, "spanish")
        .expect("translate");
    assert_eq!(translated, vec!["Hola".to_owned(), "Mundo".to_owned()]);
    let seen = scripted.seen.lock().expect("seen");
    assert_eq!(seen.len(), 1);
    assert!(seen[0].system.contains("exactly 2 items"));
    assert!(seen[0].system.contains("Keep slang casual."));
    assert!(seen[0].user.contains("\"Hello\""));
}

#[test]
fn translation_line_count_mismatch_is_rejected() {
    let scripted = Scripted::once("[\"Uno\"]");
    let provider = AiTranslationProvider::new(&scripted, "");
    let error = provider
        .translate_lines(&["one".to_owned(), "two".to_owned()], "spanish")
        .expect_err("mismatch");
    assert!(matches!(
        error,
        madrigal_lyrics::translation::TranslationError::MismatchedLines {
            expected: 2,
            got: 1
        }
    ));
    let empty_target = provider.translate_lines(&["one".to_owned()], "  ");
    assert!(empty_target.is_err());
    let empty_input = provider.translate_lines(&[], "spanish");
    assert!(empty_input.is_err());
}

#[test]
fn mixes_parser_handles_canned_model_outputs() {
    let numbered = "1. Midnight Run - Neon Palms\n2. Glass Hearts - Aluna\n3. Solar Drift - Neon Palms\n";
    let tracks = parse_track_list(numbered).expect("numbered");
    assert_eq!(tracks.len(), 3);
    assert_eq!(tracks[0].title, "Midnight Run");
    assert_eq!(tracks[1].artist, "Aluna");

    let chatty = "Here are my picks for you:\n- \"Paper Moons\" – Luna Grey\n* Static Bloom | Wave Riders\n```\n";
    let tracks = parse_track_list(chatty).expect("chatty");
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].title, "Paper Moons");
    assert_eq!(tracks[0].artist, "Luna Grey");
    assert_eq!(tracks[1].artist, "Wave Riders");

    assert!(matches!(
        parse_track_list(""),
        Err(Error::Unparseable(_))
    ));
    assert!(matches!(
        parse_track_list("No tracks come to mind right now, sorry."),
        Err(Error::Unparseable(_))
    ));

    let dupes = "1. Echoes - Far Out\n2. Echoes - Far Out\n3. Echoes - far out\n";
    assert_eq!(parse_track_list(dupes).expect("dedup").len(), 1);
}

fn seed_track(id: &str, title: &str, artists: &[&str]) -> Track {
    Track {
        identity: TrackIdentity::youtube(id),
        title: title.to_owned(),
        artists: artists.iter().map(|name| name.to_string()).collect(),
        album: Some("Seed Album".to_owned()),
        duration_secs: Some(200),
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

#[test]
fn mix_seeds_prompt_and_request_roundtrip() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = root.path().join("ai-mixes.sqlite3");
    let conn = madrigal_db::open(&path).expect("open");
    let first = seed_track("aaa111", "Alpha", &["Alice"]);
    let second = seed_track("bbb222", "Beta", &["Bob"]);
    let third = seed_track("ccc333", "Gamma", &["Alice"]);
    for track in [&first, &second, &third] {
        madrigal_db::upsert_song(&conn, track).expect("seed");
    }
    let at = madrigal_library::now_unix();
    for (identity, stamp) in [
        (&first.identity, at - 100),
        (&first.identity, at - 90),
        (&first.identity, at - 80),
        (&second.identity, at - 70),
        (&third.identity, at - 60),
    ] {
        madrigal_db::record_event(
            &conn,
            &Event {
                track: identity.clone(),
                played_at: stamp,
                duration_secs: Some(200),
                completed: true,
            },
        )
        .expect("event");
    }
    let library = madrigal_library::Library::new(&conn, Settings::default());
    let seeds = collect_mix_seeds(&library, TopRange::AllTime, 10).expect("seeds");
    assert_eq!(seeds.top_tracks.len(), 3);
    assert_eq!(seeds.top_tracks[0].title, "Alpha");
    assert_eq!(seeds.top_artists[0].0, "Alice");
    assert_eq!(seeds.top_artists[0].1, 4);

    let request = madrigal_ai::mixes::top_mix_prompt(&seeds, 5);
    assert!(request.system.contains("numbered list"));
    assert!(request.user.contains("Alice"));

    let scripted = Scripted::once("1. Night Drive - Vector Seven\n2. Alpha - Alice\n");
    let tracks = request_top_mix(&scripted, &seeds, 5).expect("top mix");
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].artist, "Vector Seven");

    let empty: MixSeeds = MixSeeds::default();
    assert!(matches!(
        request_top_mix(&scripted, &empty, 5),
        Err(Error::NotConfigured(_))
    ));

    let playlist = vec![SeedTrack {
        title: "Alpha".to_owned(),
        artists: vec!["Alice".to_owned()],
        plays: 3,
    }];
    let suggestions = request_suggestions(&scripted, "Night Drive", &playlist, 5)
        .expect("suggestions");
    assert_eq!(suggestions.len(), 2);
    assert!(matches!(
        request_suggestions(&scripted, "  ", &[], 5),
        Err(Error::NotConfigured(_))
    ));
}

#[test]
fn api_key_never_leaks_into_errors() {
    let failure = serde_json::json!({"error": {"message": "bad request"}}).to_string();
    let (url, _) = serve(vec![(500, failure)]);
    let model = OpenAiChat::new(&format!("{url}/chat"), FAKE_KEY, "m").with_timeout(TIMEOUT);
    let shown = model
        .complete(&ChatRequest::new("s", "u", 0.0, 8))
        .expect_err("http 500")
        .to_string();
    assert!(!shown.contains(FAKE_KEY));
    assert!(shown.contains("500"));

    let closed = OpenAiChat::new("http://127.0.0.1:9/chat", FAKE_KEY, "m").with_timeout(TIMEOUT);
    let shown = closed
        .complete(&ChatRequest::new("s", "u", 0.0, 8))
        .expect_err("refused")
        .to_string();
    assert!(!shown.contains(FAKE_KEY));
    assert!(!shown.contains("127.0.0.1"));
}
