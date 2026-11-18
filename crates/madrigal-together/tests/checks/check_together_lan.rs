use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use madrigal_core::{Track, TrackIdentity};
use madrigal_together::discovery::discover;
use madrigal_together::guest::ConnectedGuest;
use madrigal_together::host::{HostConfig, RunningHost};
use madrigal_together::link::{JoinInfo, JoinLink};
use madrigal_together::models::{ActivityKind, PlayState, RoomFlags, SessionCode};
use madrigal_together::proto::{AddMode, ControlAction, Message};

static SERIAL: Mutex<()> = Mutex::new(());
const WAIT: Duration = Duration::from_secs(5);

fn isolate() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::env::set_var("MADRIGAL_PROFILE_ROOT", dir.path());
    std::env::set_var("HOME", dir.path());
    dir
}

fn config_with(flags: RoomFlags) -> HostConfig {
    let mut config = HostConfig::new("Host");
    config.welcome_message = String::from("welcome to the session");
    config.flags = flags;
    config
}

fn track(id: &str, title: &str) -> Track {
    Track {
        identity: TrackIdentity::youtube(id),
        title: title.to_owned(),
        artists: vec![String::from("Artist")],
        album: None,
        duration_secs: Some(180),
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

fn next_matching(
    guest: &mut ConnectedGuest,
    mut pred: impl FnMut(&Message) -> bool,
) -> Message {
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            panic!("timeout waiting for message");
        }
        match guest.recv(left) {
            Ok(message) => {
                if pred(&message) {
                    return message;
                }
            }
            Err(e) => panic!("recv failed: {e}"),
        }
    }
}

fn state_with_queue_len(guest: &mut ConnectedGuest, len: usize) -> Vec<Track> {
    match next_matching(guest, |m| {
        matches!(m, Message::RoomState { state } if state.queue.len() == len)
    }) {
        Message::RoomState { state } => state.queue,
        _ => unreachable!(),
    }
}

fn server_error_with(guest: &mut ConnectedGuest, code: &str) -> String {
    match next_matching(guest, |m| {
        matches!(m, Message::ServerError { code: Some(c), .. } if c == code)
    }) {
        Message::ServerError { message, .. } => message,
        _ => unreachable!(),
    }
}

#[test]
fn discovery_finds_host() {
    let _guard = SERIAL.lock().unwrap();
    let _home = isolate();
    let mut host = RunningHost::start(config_with(RoomFlags::default())).unwrap();
    let beacons = discover(host.beacon_addr(), Duration::from_secs(2));
    assert_eq!(beacons.len(), 1);
    assert_eq!(beacons[0].session_id, host.snapshot().session_id);
    assert_eq!(beacons[0].tcp_port, host.tcp_addr().port());
    assert_eq!(beacons[0].code, host.code().0);
    host.shutdown();
}

#[test]
fn approve_flow() {
    let _guard = SERIAL.lock().unwrap();
    let _home = isolate();
    let flags = RoomFlags {
        require_approval: true,
        ..RoomFlags::default()
    };
    let mut host = RunningHost::start(config_with(flags)).unwrap();
    let join = host.join_info();
    let mut guest = ConnectedGuest::connect(&join, "Guest").unwrap();
    assert!(guest.pending);
    let pid = guest.participant_id.clone();
    let pending = host.participant(&pid).expect("participant");
    assert!(pending.pending);
    assert!(host.approve(&pid, true));
    assert!(!host.participant(&pid).expect("participant").pending);
    match next_matching(&mut guest, |m| {
        matches!(m, Message::JoinDecision { approved: true, .. })
    }) {
        Message::JoinDecision {
            participant_id, ..
        } => assert_eq!(participant_id, pid),
        _ => unreachable!(),
    }
    let kinds: Vec<ActivityKind> = host
        .activity()
        .iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(kinds, vec![ActivityKind::Join, ActivityKind::Approve]);
    host.shutdown();
}

#[test]
fn guest_add_allowed_and_rejected() {
    let _guard = SERIAL.lock().unwrap();
    let _home = isolate();
    let mut host = RunningHost::start(config_with(RoomFlags::default())).unwrap();
    host.add_track(track("v1", "One"), AddMode::AddToQueue);
    let join = host.join_info();
    let mut guest = ConnectedGuest::connect(&join, "Guest").unwrap();
    guest
        .request_add(track("v2", "Two"), AddMode::AddToQueue)
        .unwrap();
    let queue = state_with_queue_len(&mut guest, 2);
    assert_eq!(queue[1].title, "Two");
    guest.request_control(ControlAction::Play).unwrap();
    let err = server_error_with(&mut guest, "control_forbidden");
    assert!(err.contains("control"));
    host.set_flags(RoomFlags {
        guests_can_control: true,
        ..RoomFlags::default()
    });
    guest.request_control(ControlAction::Play).unwrap();
    let playing = next_matching(&mut guest, |m| {
        matches!(m, Message::RoomState { state } if state.play.is_playing)
    });
    assert!(matches!(playing, Message::RoomState { .. }));
    host.shutdown();

    let closed_flags = RoomFlags {
        guests_can_add: false,
        ..RoomFlags::default()
    };
    let mut host = RunningHost::start(config_with(closed_flags)).unwrap();
    let join = host.join_info();
    let mut guest = ConnectedGuest::connect(&join, "Guest").unwrap();
    guest
        .request_add(track("v3", "Three"), AddMode::AddToQueue)
        .unwrap();
    let err = server_error_with(&mut guest, "add_forbidden");
    assert!(err.contains("add"));
    assert!(host.snapshot().queue.is_empty());
    host.shutdown();
}

#[test]
fn kick_ban_transfer() {
    let _guard = SERIAL.lock().unwrap();
    let _home = isolate();
    let mut host = RunningHost::start(config_with(RoomFlags::default())).unwrap();
    let join = host.join_info();
    let mut a = ConnectedGuest::connect(&join, "A").unwrap();
    let mut b = ConnectedGuest::connect_with_id(&join, "B", "device-b").unwrap();
    let pid_a = a.participant_id.clone();
    let pid_b = b.participant_id.clone();

    assert!(host.kick(&pid_a, Some("nope")));
    match next_matching(&mut a, |m| matches!(m, Message::Kick { .. })) {
        Message::Kick { reason, .. } => assert_eq!(reason.as_deref(), Some("nope")),
        _ => unreachable!(),
    }
    assert!(host.participant(&pid_a).is_none());
    assert!(!host.kick(&pid_a, None));

    assert!(host.ban(&pid_b, None));
    match next_matching(&mut b, |m| matches!(m, Message::Ban { .. })) {
        Message::Ban { .. } => {}
        _ => unreachable!(),
    }
    let join_b = JoinInfo {
        host: join.host.clone(),
        port: join.port,
        session_id: join.session_id.clone(),
        session_key: join.session_key.clone(),
    };
    match ConnectedGuest::connect_with_id(&join_b, "B", "device-b") {
        Ok(_) => panic!("banned guest reconnected"),
        Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied),
    }

    let mut c = ConnectedGuest::connect(&join, "C").unwrap();
    let pid_c = c.participant_id.clone();
    assert!(!host.transfer_host("missing"));
    assert!(host.transfer_host(&pid_c));
    let snapshot = host.snapshot();
    assert_eq!(snapshot.host_id, pid_c);
    let promoted = host.participant(&pid_c).expect("participant");
    assert!(promoted.is_host());
    match next_matching(&mut c, |m| matches!(m, Message::HostTransferred { .. })) {
        Message::HostTransferred { participant_id, .. } => {
            assert_eq!(participant_id, pid_c);
        }
        _ => unreachable!(),
    }
    host.shutdown();
}

#[test]
fn playback_version_converges() {
    let _guard = SERIAL.lock().unwrap();
    let _home = isolate();
    let flags = RoomFlags {
        guests_can_control: true,
        ..RoomFlags::default()
    };
    let mut host = RunningHost::start(config_with(flags)).unwrap();
    let base = PlayState {
        version: 3,
        is_playing: true,
        ..PlayState::default()
    };
    assert!(host.submit_play_state(base));
    assert!(!host.submit_play_state(PlayState::default()));
    let join = host.join_info();
    let handles: Vec<_> = (10u64..16)
        .map(|v| {
            let join = JoinInfo {
                host: join.host.clone(),
                port: join.port,
                session_id: join.session_id.clone(),
                session_key: join.session_key.clone(),
            };
            std::thread::spawn(move || {
                let mut guest = ConnectedGuest::connect(&join, "Racer").unwrap();
                guest
                    .submit_play(PlayState {
                        version: v,
                        is_playing: v % 2 == 0,
                        position_ms: v * 1000,
                        track_index: 0,
                    })
                    .unwrap();
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let deadline = Instant::now() + WAIT;
    loop {
        if host.snapshot().play.version == 15 || Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(host.snapshot().play.version, 15);
    let mut watcher = ConnectedGuest::connect(&host.join_info(), "Watcher").unwrap();
    let state = next_matching(&mut watcher, |m| {
        matches!(m, Message::RoomState { state } if state.play.version == 15)
    });
    assert!(matches!(state, Message::RoomState { .. }));
    host.shutdown();
}

#[test]
fn queue_move_propagates() {
    let _guard = SERIAL.lock().unwrap();
    let _home = isolate();
    let mut host = RunningHost::start(config_with(RoomFlags::default())).unwrap();
    host.add_track(track("v1", "One"), AddMode::AddToQueue);
    host.add_track(track("v2", "Two"), AddMode::AddToQueue);
    host.add_track(track("v3", "Three"), AddMode::AddToQueue);
    let mut guest = ConnectedGuest::connect(&host.join_info(), "Guest").unwrap();
    guest.request_move(0, 2).unwrap();
    let queue = match next_matching(&mut guest, |m| {
        matches!(m, Message::RoomState { state }
            if state.queue.len() == 3 && state.queue[0].title == "Two")
    }) {
        Message::RoomState { state } => state.queue,
        _ => unreachable!(),
    };
    let titles: Vec<&str> = queue.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, vec!["Two", "Three", "One"]);
    assert_eq!(host.snapshot().queue, queue);
    guest.request_move(0, 9).unwrap();
    let err = server_error_with(&mut guest, "move_forbidden");
    assert!(err.contains("range"));
    host.shutdown();
}

#[test]
fn activity_log_order() {
    let _guard = SERIAL.lock().unwrap();
    let _home = isolate();
    let flags = RoomFlags {
        require_approval: true,
        guests_can_add: true,
        guests_can_control: true,
    };
    let mut host = RunningHost::start(config_with(flags)).unwrap();
    let mut guest = ConnectedGuest::connect(&host.join_info(), "Guest").unwrap();
    let pid = guest.participant_id.clone();
    assert!(host.approve(&pid, true));
    guest
        .request_add(track("v1", "One"), AddMode::AddToQueue)
        .unwrap();
    state_with_queue_len(&mut guest, 1);
    guest.request_control(ControlAction::Play).unwrap();
    next_matching(&mut guest, |m| {
        matches!(m, Message::RoomState { state } if state.play.is_playing)
    });
    assert!(host.transfer_host(&pid));
    assert!(host.kick(&pid, None));
    let kinds: Vec<ActivityKind> = host.activity().iter().map(|e| e.kind).collect();
    let wanted = [
        ActivityKind::Join,
        ActivityKind::Approve,
        ActivityKind::QueueChange,
        ActivityKind::PlayState,
        ActivityKind::Transfer,
        ActivityKind::Kick,
    ];
    let mut pos = 0;
    for kind in kinds {
        if pos < wanted.len() && kind == wanted[pos] {
            pos += 1;
        }
    }
    assert_eq!(pos, wanted.len());
    let seqs: Vec<u64> = host.activity().iter().map(|e| e.seq).collect();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]));
    host.shutdown();
}

#[test]
fn deeplink_roundtrip() {
    let _guard = SERIAL.lock().unwrap();
    let _home = isolate();
    let code = SessionCode::generate();
    assert!((4..=12).contains(&code.as_str().len()));
    let link = JoinInfo::code_link(&code);
    assert!(link.starts_with("madrigal://together/join?code="));
    match madrigal_together::parse_link(&link) {
        Some(JoinLink::Code(back)) => assert_eq!(back, code),
        other => panic!("code link failed: {other:?}"),
    }
    let info = JoinInfo {
        host: String::from("192.168.1.5"),
        port: 42117,
        session_id: String::from("session-1"),
        session_key: String::from("key-1"),
    };
    let deep = info.deep_link();
    match madrigal_together::parse_link(&deep) {
        Some(JoinLink::Full(back)) => assert_eq!(back, info),
        other => panic!("deep link failed: {other:?}"),
    }
    match madrigal_together::parse_link("192.168.1.5|42117|session-1|key-1") {
        Some(JoinLink::Full(back)) => assert_eq!(back, info),
        other => panic!("compact link failed: {other:?}"),
    }
    assert!(madrigal_together::parse_link("not a link").is_none());
    assert!(madrigal_together::parse_link("https://example.com/x").is_none());
    assert!(madrigal_together::parse_link("madrigal://together/join?code=!!!").is_none());
}

#[test]
fn malformed_no_crash() {
    let _guard = SERIAL.lock().unwrap();
    let _home = isolate();
    let mut host = RunningHost::start(config_with(RoomFlags::default())).unwrap();
    let join = host.join_info();
    let mut guest = ConnectedGuest::connect(&join, "Guest").unwrap();
    guest.send_raw(b"{{not json\n").unwrap();
    let err = server_error_with(&mut guest, "bad_frame");
    assert!(err.contains("undecodable"));
    guest.ping(7).unwrap();
    match next_matching(&mut guest, |m| {
        matches!(m, Message::HeartbeatPong { ping_id: 7, .. })
    }) {
        Message::HeartbeatPong { .. } => {}
        _ => unreachable!(),
    }

    let mut raw = TcpStream::connect(format!("{}:{}", join.host, join.port)).unwrap();
    raw.set_read_timeout(Some(WAIT)).unwrap();
    let bad_hello = serde_json::json!({
        "type": "client_hello",
        "protocol_version": 999,
        "session_id": join.session_id,
        "session_key": join.session_key,
        "client_id": "x",
        "display_name": "Eve",
    });
    writeln!(raw, "{}", bad_hello).unwrap();
    let mut reader = BufReader::new(raw.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("bad_version"));

    let mut raw = TcpStream::connect(format!("{}:{}", join.host, join.port)).unwrap();
    raw.set_read_timeout(Some(WAIT)).unwrap();
    let bad_key = serde_json::json!({
        "type": "client_hello",
        "protocol_version": 1,
        "session_id": join.session_id,
        "session_key": "wrong",
        "client_id": "y",
        "display_name": "Mallory",
    });
    writeln!(raw, "{}", bad_key).unwrap();
    let mut reader = BufReader::new(raw);
    line.clear();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("bad_session"));
    assert!(host.snapshot().participants.len() == 2);
    host.shutdown();
}
