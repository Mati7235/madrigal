use std::env;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use madrigal_core::{Track, TrackIdentity};
use madrigal_discord::{
    Decision, HiddenReason, HoldReason, IpcClient, IpcError, PlaybackContext, PresenceInputs,
    Preview, RefreshGate, build_activity, derive_decision, derive_final_decision, has_credentials,
    valid_client_id,
};
use madrigal_discord::policy::{HoldContext, PresenceMode};
use madrigal_settings::{DiscordActivityType, Integrations};
use serde_json::Value;

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.get_or_init(|| Mutex::new(())).lock().expect("env lock")
}

fn setup() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    madrigal_paths::ensure_dirs().expect("dirs");
    root
}

fn sample_track() -> Track {
    Track {
        identity: TrackIdentity::youtube("dQw4w9WgXcQ"),
        title: "Never Gonna Give You Up".to_owned(),
        artists: vec!["Rick Astley".to_owned()],
        album: Some("Whenever You Need Somebody".to_owned()),
        duration_secs: Some(212),
        thumbnail: Some("https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg".to_owned()),
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

fn sample_config() -> Integrations {
    Integrations {
        discord_rpc_enabled: true,
        discord_client_id: "123456789012345678".to_owned(),
        discord_activity_name: "APP".to_owned(),
        discord_activity_details: "SONG".to_owned(),
        discord_activity_state: "ARTIST".to_owned(),
        discord_activity_type: DiscordActivityType::Listening,
        discord_large_image_type: "thumbnail".to_owned(),
        discord_large_text_source: "album".to_owned(),
        discord_small_image_type: "artist".to_owned(),
        discord_button1_enabled: true,
        discord_button1_label: "Listen on YouTube Music".to_owned(),
        discord_button1_url_source: "songurl".to_owned(),
        discord_button2_enabled: true,
        discord_button2_label: "Open Madrigal".to_owned(),
        discord_button2_url_source: "custom".to_owned(),
        discord_button2_custom_url: "example.org/madrigal".to_owned(),
        discord_show_when_paused: true,
        ..Integrations::default()
    }
}

fn write_frame(stream: &mut UnixStream, opcode: u32, body: &Value) {
    let raw = serde_json::to_vec(body).expect("encode");
    let mut header = [0u8; 8];
    header[..4].copy_from_slice(&opcode.to_le_bytes());
    header[4..].copy_from_slice(&(raw.len() as u32).to_le_bytes());
    stream.write_all(&header).expect("header");
    stream.write_all(&raw).expect("body");
    stream.flush().expect("flush");
}

fn read_frame(stream: &mut UnixStream) -> Value {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header).expect("header");
    let length = u32::from_le_bytes(header[4..].try_into().expect("len")) as usize;
    let mut raw = vec![0u8; length];
    stream.read_exact(&mut raw).expect("body");
    serde_json::from_slice(&raw).expect("json")
}

struct FakeServer {
    captured: Arc<Mutex<Vec<Value>>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl FakeServer {
    fn spawn(mode: ServerMode, max_connections: usize) -> Self {
        let path = madrigal_paths::runtime_dir().join("discord-ipc-0");
        let listener = UnixListener::bind(&path).expect("bind");
        let captured = Arc::new(Mutex::new(Vec::new()));
        let worker = Arc::clone(&captured);
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming().take(max_connections).flatten() {
                handle_connection(stream, &mode, &worker);
            }
        });
        Self {
            captured,
            handle: Some(handle),
        }
    }

    fn join(mut self) -> Vec<Value> {
        let _ = self.handle.take().map(|handle| handle.join());
        self.captured.lock().expect("capture").clone()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ServerMode {
    Healthy,
    Garbage,
}

fn handle_connection(mut stream: UnixStream, mode: &ServerMode, captured: &Arc<Mutex<Vec<Value>>>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let request = read_frame(&mut stream);
    if *mode == ServerMode::Garbage {
        stream.write_all(b"\x01\x02not-json{{{").expect("garbage");
        stream.flush().expect("flush");
        return;
    }
    if request.get("client_id").is_some() {
        write_frame(
            &mut stream,
            1,
            &serde_json::json!({"cmd": "DISPATCH", "evt": "READY", "data": {}, "nonce": Value::Null}),
        );
        let activity_request = read_frame(&mut stream);
        captured.lock().expect("capture").push(activity_request.clone());
        let nonce = activity_request.get("nonce").cloned().unwrap_or(Value::Null);
        write_frame(
            &mut stream,
            1,
            &serde_json::json!({"cmd": "SET_ACTIVITY", "evt": Value::Null, "data": {}, "nonce": nonce}),
        );
    }
}

fn playing_inputs(show_when_paused: bool) -> PresenceInputs {
    PresenceInputs {
        enabled: true,
        has_client_id: true,
        track_id: Some("dQw4w9WgXcQ".to_owned()),
        play_when_ready: true,
        is_playing: true,
        buffering: false,
        show_when_paused,
        service_stopping: false,
    }
}

#[test]
fn handshake_carries_client_id() {
    let _guard = lock();
    let _root = setup();
    let server = FakeServer::spawn(ServerMode::Healthy, 1);
    let config = sample_config();
    let mut client = IpcClient::connect().expect("connect");
    let reply = client.handshake(&config.discord_client_id).expect("handshake");
    assert_eq!(reply.get("evt").and_then(Value::as_str), Some("READY"));
    assert!(valid_client_id(&config.discord_client_id));
    assert!(has_credentials(&config.discord_client_id));
    assert!(!valid_client_id("not-a-snowflake"));
    assert!(!has_credentials(""));
    let captured = server.join();
    assert_eq!(captured.len(), 0);
}

#[test]
fn activity_payload_shape() {
    let _guard = lock();
    let _root = setup();
    let server = FakeServer::spawn(ServerMode::Healthy, 1);
    let config = sample_config();
    let track = sample_track();
    let now_ms = 1_700_000_000_000u64;
    let position_ms = 30_000u64;
    let activity = build_activity(&config, &track, position_ms, false, now_ms);
    assert_eq!(activity.get("type").and_then(Value::as_u64), Some(2));
    assert_eq!(
        activity.get("details").and_then(Value::as_str),
        Some("Never Gonna Give You Up")
    );
    assert_eq!(
        activity.get("state").and_then(Value::as_str),
        Some("Rick Astley")
    );
    let start = (now_ms - position_ms) / 1000;
    let stamps = activity.get("timestamps").expect("timestamps");
    assert_eq!(stamps.get("start").and_then(Value::as_u64), Some(start));
    assert_eq!(
        stamps.get("end").and_then(Value::as_u64),
        Some(start + 212)
    );
    let buttons = activity.get("buttons").and_then(Value::as_array).expect("buttons");
    assert_eq!(buttons.len(), 2);
    assert_eq!(
        buttons[0].get("url").and_then(Value::as_str),
        Some("https://music.youtube.com/watch?v=dQw4w9WgXcQ")
    );
    assert_eq!(
        buttons[1].get("url").and_then(Value::as_str),
        Some("https://example.org/madrigal")
    );
    let mut client = IpcClient::connect().expect("connect");
    client.handshake(&config.discord_client_id).expect("handshake");
    client.set_activity(&activity).expect("set activity");
    let captured = server.join();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].get("cmd").and_then(Value::as_str), Some("SET_ACTIVITY"));
    assert_eq!(captured[0].get("args").and_then(|args| args.get("activity")), Some(&activity));
}

#[test]
fn activity_type_mapping() {
    let _guard = lock();
    let _root = setup();
    let track = sample_track();
    let kinds = [
        (DiscordActivityType::Playing, 0),
        (DiscordActivityType::Streaming, 1),
        (DiscordActivityType::Listening, 2),
        (DiscordActivityType::Watching, 3),
        (DiscordActivityType::Competing, 5),
    ];
    for (kind, native) in kinds {
        let mut config = sample_config();
        config.discord_activity_type = kind;
        let activity = build_activity(&config, &track, 0, false, 1_000_000);
        assert_eq!(activity.get("type").and_then(Value::as_u64), Some(native));
    }
}

#[test]
fn paused_policy_and_clear() {
    let _guard = lock();
    let _root = setup();
    let hidden = playing_inputs(false);
    let hidden = PresenceInputs {
        play_when_ready: false,
        is_playing: false,
        ..hidden
    };
    assert_eq!(
        derive_decision(&hidden),
        Decision::Hidden {
            reason: HiddenReason::PausedByPreference
        }
    );
    let visible = PresenceInputs {
        show_when_paused: true,
        ..hidden
    };
    assert_eq!(
        derive_decision(&visible),
        Decision::Visible {
            track_id: "dQw4w9WgXcQ".to_owned(),
            mode: PresenceMode::Paused
        }
    );
    let config = sample_config();
    let track = sample_track();
    let paused = build_activity(&config, &track, 10_000, true, 2_000_000_000_000);
    let stamps = paused.get("timestamps").expect("timestamps");
    assert!(stamps.get("start").and_then(Value::as_u64).is_some());
    assert!(stamps.get("end").is_none());
    let server = FakeServer::spawn(ServerMode::Healthy, 1);
    let mut client = IpcClient::connect().expect("connect");
    client.handshake(&config.discord_client_id).expect("handshake");
    client.clear_activity().expect("clear");
    let captured = server.join();
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured[0]
            .get("args")
            .and_then(|args| args.get("activity")),
        Some(&Value::Null)
    );
}

#[test]
fn preview_matches_connected_payload() {
    let _guard = lock();
    let _root = setup();
    let server = FakeServer::spawn(ServerMode::Healthy, 1);
    let config = sample_config();
    let track = sample_track();
    let preview = Preview::new(&config);
    let rendered = preview.render(&track, 5_000, false, 1_800_000_000_000);
    let direct = build_activity(&config, &track, 5_000, false, 1_800_000_000_000);
    assert_eq!(rendered, direct);
    let mut client = IpcClient::connect().expect("connect");
    client.handshake(&config.discord_client_id).expect("handshake");
    client.set_activity(&rendered).expect("set activity");
    let captured = server.join();
    assert_eq!(
        captured[0].get("args").and_then(|args| args.get("activity")),
        Some(&rendered)
    );
}

#[test]
fn refresh_gate_coalesces_rapid_updates() {
    let _guard = lock();
    let _root = setup();
    let track = sample_track();
    let mut gate = RefreshGate::new(2_000);
    let first = PlaybackContext {
        track: Some(&track),
        is_playing: true,
        now_ms: 100_000,
    };
    assert!(gate.decide(first));
    let rapid = PlaybackContext {
        track: Some(&track),
        is_playing: true,
        now_ms: 100_500,
    };
    assert!(!gate.decide(rapid));
    let other = Track {
        identity: TrackIdentity::youtube("other-id"),
        ..sample_track()
    };
    let switched = PlaybackContext {
        track: Some(&other),
        is_playing: true,
        now_ms: 100_600,
    };
    assert!(gate.decide(switched));
    let flipped = PlaybackContext {
        track: Some(&other),
        is_playing: false,
        now_ms: 100_700,
    };
    assert!(gate.decide(flipped));
    let settled = PlaybackContext {
        track: Some(&other),
        is_playing: false,
        now_ms: 200_000,
    };
    assert!(gate.decide(settled));
}

#[test]
fn hold_tracks_buffering_before_stall() {
    let _guard = lock();
    let _root = setup();
    let buffering = PresenceInputs {
        buffering: true,
        ..playing_inputs(true)
    };
    assert_eq!(
        derive_decision(&buffering),
        Decision::Hidden {
            reason: HiddenReason::NoStablePlaybackYet
        }
    );
    let context = HoldContext {
        now_ms: 10_000,
        active_hold: None,
        last_visible: None,
        hold_timeout_ms: 5_000,
    };
    let resolution = derive_final_decision(&buffering, &context);
    assert_eq!(
        resolution.decision,
        Decision::Hidden {
            reason: HiddenReason::NoStablePlaybackYet
        }
    );
    let context = HoldContext {
        now_ms: 1_000,
        active_hold: None,
        last_visible: Some(madrigal_discord::policy::LastAppliedVisible {
            track_id: "dQw4w9WgXcQ".to_owned(),
            mode: PresenceMode::Playing,
            applied_at_ms: 0,
        }),
        hold_timeout_ms: 5_000,
    };
    let resolution = derive_final_decision(&buffering, &context);
    assert_eq!(
        resolution.decision,
        Decision::Hold {
            reason: HoldReason::BufferingWhilePlayRequested,
            track_id: Some("dQw4w9WgXcQ".to_owned())
        }
    );
    assert!(resolution.next_hold.is_some());
}

#[test]
fn missing_socket_reports_not_running() {
    let _guard = lock();
    let _root = setup();
    let result = IpcClient::connect();
    assert!(matches!(result, Err(IpcError::NotRunning)));
}

#[test]
fn malformed_response_is_clean_error() {
    let _guard = lock();
    let _root = setup();
    let server = FakeServer::spawn(ServerMode::Garbage, 1);
    let config = sample_config();
    let mut client = IpcClient::connect().expect("connect");
    let result = client.handshake(&config.discord_client_id);
    assert!(matches!(result, Err(IpcError::Malformed(_))));
    server.join();
}
