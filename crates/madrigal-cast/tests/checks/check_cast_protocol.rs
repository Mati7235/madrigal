use std::collections::{HashMap, VecDeque};

use madrigal_cast::discovery::{CastDevice, ServiceRecord, device_from_record};
use madrigal_cast::media::{CastMedia, load_payload, normalize_mime};
use madrigal_cast::protocol::{
    CastMessage, DEFAULT_MEDIA_RECEIVER_APP_ID, NS_CONNECTION, NS_MEDIA, NS_RECEIVER, RECEIVER_ID,
    SENDER_ID, close, connect, decode_frame, decode_proto, encode_frame, encode_proto, get_status,
    is_ping, launch, media_stop, parse_media_session_ids, parse_receiver_status, ping, pong,
    set_volume, stop_app,
};
use madrigal_cast::session::{Session, State, Transport};
use madrigal_cast::{Error, Result};
use madrigal_core::Track;
use serde_json::{Value, json};

struct MockTransport {
    outbound: Vec<CastMessage>,
    inbound: VecDeque<CastMessage>,
}

impl MockTransport {
    fn with_inbound(messages: Vec<CastMessage>) -> Self {
        Self { outbound: Vec::new(), inbound: messages.into() }
    }
}

impl Transport for MockTransport {
    fn send(&mut self, message: &CastMessage) -> Result<()> {
        self.outbound.push(message.clone());
        Ok(())
    }

    fn recv(&mut self) -> Result<CastMessage> {
        self.inbound.pop_front().ok_or_else(|| Error::Transport("exhausted".into()))
    }
}

fn status_message(app_id: &str, session_id: &str, transport_id: &str) -> CastMessage {
    CastMessage::new(
        RECEIVER_ID,
        SENDER_ID,
        NS_RECEIVER,
        json!({
            "type": "RECEIVER_STATUS",
            "requestId": 1,
            "status": {
                "applications": [{
                    "appId": app_id,
                    "sessionId": session_id,
                    "transportId": transport_id,
                    "namespaces": [{"name": NS_MEDIA}],
                }],
                "volume": {"level": 1.0, "muted": false},
            },
        }),
    )
    .expect("status")
}

fn media_status_message(media_session_id: i64) -> CastMessage {
    CastMessage::new(
        "web-9",
        SENDER_ID,
        NS_MEDIA,
        json!({
            "type": "MEDIA_STATUS",
            "requestId": 2,
            "status": [{"mediaSessionId": media_session_id, "playerState": "PLAYING"}],
        }),
    )
    .expect("media status")
}

fn fixture_track() -> Track {
    Track {
        identity: madrigal_core::TrackIdentity::youtube("vid"),
        title: "Song".into(),
        artists: vec!["Singer".into(), "Feat".into()],
        album: Some("Record".into()),
        duration_secs: Some(200),
        thumbnail: Some("http://art/img.jpg".into()),
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

#[test]
fn frame_roundtrip_preserves_message() {
    let message = connect().expect("connect");
    let frame = encode_frame(&message);
    let (decoded, consumed) = decode_frame(&frame).expect("decode");
    assert_eq!(decoded, message);
    assert_eq!(consumed, frame.len());
    assert_eq!(&frame[..4], &u32::try_from(frame.len() - 4).expect("len").to_be_bytes());
}

#[test]
fn frame_prefix_matches_proto_body() {
    let message = launch(DEFAULT_MEDIA_RECEIVER_APP_ID, 7).expect("launch");
    let frame = encode_frame(&message);
    assert_eq!(&frame[4..], &encode_proto(&message));
}

#[test]
fn proto_rejects_truncated_and_garbage() {
    let message = ping().expect("ping");
    let frame = encode_frame(&message);
    assert!(matches!(decode_frame(&frame[..5]), Err(Error::NeedMore)));
    assert!(matches!(decode_frame(&frame[..3]), Err(Error::NeedMore)));
    let oversized = [0xffu8, 0xff, 0xff, 0xff];
    assert!(matches!(decode_frame(&oversized), Err(Error::Malformed(_))));
    assert!(matches!(decode_proto(b"not a message"), Err(Error::Malformed(_))));
    assert!(matches!(decode_proto(&[]), Err(Error::Malformed(_))));
}

#[test]
fn connect_and_heartbeat_shapes() {
    assert_eq!(connect().expect("c").payload, r#"{"type":"CONNECT"}"#);
    assert_eq!(ping().expect("p").payload, r#"{"type":"PING"}"#);
    assert_eq!(pong().expect("p").payload, r#"{"type":"PONG"}"#);
    let message = connect().expect("c");
    assert_eq!((message.source.as_str(), message.dest.as_str()), (SENDER_ID, RECEIVER_ID));
    assert_eq!(message.namespace, NS_CONNECTION);
    assert!(is_ping(&ping().expect("p")));
    assert!(!is_ping(&connect().expect("c")));
}

#[test]
fn receiver_control_shapes() {
    assert_eq!(
        get_status(3).expect("s").payload_value().expect("json"),
        json!({"type": "GET_STATUS", "requestId": 3}),
    );
    assert_eq!(
        launch(DEFAULT_MEDIA_RECEIVER_APP_ID, 4).expect("l").payload_value().expect("json"),
        json!({"type": "LAUNCH", "appId": "CC1AD845", "requestId": 4}),
    );
    assert_eq!(
        stop_app("session-1", 5).expect("s").payload_value().expect("json"),
        json!({"type": "STOP", "sessionId": "session-1", "requestId": 5}),
    );
    assert_eq!(
        set_volume(0.5, 6).expect("v").payload_value().expect("json"),
        json!({"type": "SET_VOLUME", "volume": {"level": 0.5}, "requestId": 6}),
    );
    assert_eq!(
        set_volume(9.0, 6).expect("v").payload_value().expect("json")["volume"]["level"],
        Value::from(1.0),
    );
    assert_eq!(
        set_volume(-2.0, 6).expect("v").payload_value().expect("json")["volume"]["level"],
        Value::from(0.0),
    );
    assert_eq!(
        media_stop("web-9", 11, 8).expect("s").payload_value().expect("json"),
        json!({"type": "STOP", "mediaSessionId": 11, "requestId": 8}),
    );
    assert_eq!(
        close("web-9").expect("c").payload_value().expect("json"),
        json!({"type": "CLOSE"}),
    );
    for message in
        [get_status(1).expect("g"), launch("x", 1).expect("l"), set_volume(0.1, 1).expect("v")]
    {
        assert_eq!(message.namespace, NS_RECEIVER);
        assert_eq!(message.dest, RECEIVER_ID);
    }
    assert_eq!(media_stop("web-9", 1, 1).expect("m").namespace, NS_MEDIA);
}

#[test]
fn mime_normalization_matches_oracle() {
    assert_eq!(normalize_mime(Some("audio/mp4")), Some("audio/mp4".into()));
    assert_eq!(
        normalize_mime(Some("audio/webm; codecs=\"opus\"")),
        Some("audio/webm; codecs=\"opus\"".into())
    );
    assert_eq!(normalize_mime(Some("  video/mp4  ")), Some("video/mp4".into()));
    assert_eq!(normalize_mime(Some("audio/*")), None);
    assert_eq!(normalize_mime(Some("audio")), None);
    assert_eq!(normalize_mime(Some("")), None);
    assert_eq!(normalize_mime(Some("   ")), None);
    assert_eq!(normalize_mime(None), None);
}

#[test]
fn load_payload_matches_media3_metadata() {
    let mut media = CastMedia::from_track(&fixture_track(), "http://lan:8000/cast/local/abc".into());
    media.mime = Some("audio/mp4; codecs=\"mp4a.40.2\"".into());
    let payload = load_payload(&media, "session-1", 9);
    assert_eq!(payload["type"], Value::from("LOAD"));
    assert_eq!(payload["requestId"], Value::from(9));
    assert_eq!(payload["sessionId"], Value::from("session-1"));
    assert_eq!(payload["autoplay"], Value::from(true));
    let info = &payload["media"];
    assert_eq!(info["contentId"], Value::from("http://lan:8000/cast/local/abc"));
    assert_eq!(info["streamType"], Value::from("BUFFERED"));
    assert_eq!(info["contentType"], Value::from("audio/mp4; codecs=\"mp4a.40.2\""));
    assert_eq!(info["duration"], Value::from(200.0));
    let metadata = &info["metadata"];
    assert_eq!(metadata["metadataType"], Value::from(3));
    assert_eq!(metadata["title"], Value::from("Song"));
    assert_eq!(metadata["artist"], Value::from("Singer, Feat"));
    assert_eq!(metadata["albumName"], Value::from("Record"));
    assert_eq!(metadata["images"][0]["url"], Value::from("http://art/img.jpg"));
}

#[test]
fn load_payload_omits_missing_fields() {
    let media = CastMedia {
        url: "https://cdn/s.mp3".into(),
        mime: Some("audio/*".into()),
        title: "Only".into(),
        artist: None,
        album: None,
        artwork: None,
        duration_secs: None,
    };
    let payload = load_payload(&media, "s", 1);
    assert!(payload["media"].get("contentType").is_none());
    assert!(payload["media"].get("duration").is_none());
    assert!(payload["media"]["metadata"].get("artist").is_none());
    assert!(payload["media"]["metadata"].get("images").is_none());
}

#[test]
fn receiver_status_parses_applications_and_volume() {
    let status = parse_receiver_status(&status_message("CC1AD845", "s-1", "web-9").payload)
        .expect("status");
    assert_eq!(status.applications.len(), 1);
    assert_eq!(status.applications[0].session_id, "s-1");
    assert_eq!(status.applications[0].transport_id, "web-9");
    assert_eq!(status.volume_level, Some(1.0));
    assert_eq!(status.volume_muted, Some(false));
    assert!(matches!(
        parse_receiver_status(r#"{"type":"MEDIA_STATUS","status":{}}"#),
        Err(Error::UnexpectedType(_))
    ));
    assert!(matches!(
        parse_receiver_status(r#"{"type":"RECEIVER_STATUS"}"#),
        Err(Error::Malformed(_))
    ));
}

#[test]
fn media_status_parses_session_ids() {
    let ids =
        parse_media_session_ids(&media_status_message(42).payload).expect("ids");
    assert_eq!(ids, vec![42]);
    assert!(matches!(
        parse_media_session_ids(r#"{"type":"RECEIVER_STATUS","status":[]}"#),
        Err(Error::UnexpectedType(_))
    ));
    assert!(matches!(
        parse_media_session_ids(r#"{"type":"MEDIA_STATUS","status":[]}"#),
        Err(Error::Malformed(_))
    ));
}

#[test]
fn session_runs_full_lifecycle() {
    let mut session = Session::new(MockTransport::with_inbound(vec![
        ping().expect("ping"),
        status_message("CC1AD845", "s-1", "web-9"),
        media_status_message(42),
    ]));
    assert_eq!(session.state(), State::Idle);
    session.connect().expect("connect");
    assert_eq!(session.state(), State::Connected);
    assert_eq!(
        session.transport().outbound[0].payload_value().expect("json").get("type"),
        Some(&Value::from("CONNECT"))
    );
    session.launch(DEFAULT_MEDIA_RECEIVER_APP_ID).expect("launch");
    assert_eq!(session.state(), State::Launched);
    assert_eq!(session.app().expect("app").transport_id, "web-9");
    let sent: Vec<Value> = session
        .transport()
        .outbound
        .iter()
        .map(|message| message.payload_value().expect("json"))
        .collect();
    assert_eq!(sent[1], json!({"type": "LAUNCH", "appId": "CC1AD845", "requestId": 1}));
    assert_eq!(sent[2], json!({"type": "PONG"}));
    let media = CastMedia::from_track(&fixture_track(), "http://lan/t.mp3".into());
    session.load(&media).expect("load");
    assert_eq!(session.state(), State::Loaded);
    assert_eq!(session.media_session_id(), Some(42));
    let last = session.transport().outbound.last().expect("last");
    assert_eq!(last.dest, "web-9");
    let load_sent = last.payload_value().expect("json");
    assert_eq!(load_sent["type"], Value::from("LOAD"));
    assert_eq!(load_sent["sessionId"], Value::from("s-1"));
    let before = session.transport().outbound.len();
    session.set_volume(0.25).expect("volume");
    assert_eq!(session.state(), State::Loaded);
    assert_eq!(
        session.transport().outbound[before].payload_value().expect("json"),
        json!({"type": "SET_VOLUME", "volume": {"level": 0.25}, "requestId": 3}),
    );
    session.stop_media().expect("stop");
    assert_eq!(session.state(), State::Stopped);
    assert_eq!(session.media_session_id(), None);
    let stop = session.transport().outbound.last().expect("last");
    assert_eq!(stop.dest, "web-9");
    assert_eq!(
        stop.payload_value().expect("json")["type"],
        Value::from("STOP")
    );
    session.disconnect().expect("disconnect");
    assert_eq!(session.state(), State::Closed);
    let close = session.transport().outbound.last().expect("last");
    assert_eq!(
        close.payload_value().expect("json"),
        json!({"type": "CLOSE"})
    );
}

#[test]
fn session_rejects_wrong_state_and_malformed() {
    let mut session = Session::new(MockTransport::with_inbound(Vec::new()));
    assert!(matches!(session.load(&CastMedia::from_track(&fixture_track(), "u".into())), Err(Error::WrongState(_))));
    assert!(matches!(session.disconnect(), Err(Error::WrongState(_))));
    let mut session = Session::new(MockTransport::with_inbound(vec![
        CastMessage::new(RECEIVER_ID, SENDER_ID, NS_RECEIVER, json!({"type": "RECEIVER_STATUS", "status": {}}))
            .expect("empty"),
    ]));
    session.connect().expect("connect");
    assert!(matches!(session.launch("CC1AD845"), Err(Error::Malformed(_))));
    let mut session = Session::new(MockTransport::with_inbound(Vec::new()));
    session.connect().expect("connect");
    assert!(matches!(session.launch("CC1AD845"), Err(Error::Transport(_))));
}

#[test]
fn discovery_maps_txt_with_fallbacks() {
    let mut txt = HashMap::new();
    txt.insert("fn".into(), "Living Room".into());
    txt.insert("md".into(), "Chromecast".into());
    txt.insert("id".into(), "abc123".into());
    let device = device_from_record(&ServiceRecord {
        fullname: "Living-Room._googlecast._tcp.local.".into(),
        host: "192.168.1.10".into(),
        port: 8009,
        txt,
    })
    .expect("device");
    assert_eq!(
        device,
        CastDevice {
            id: "abc123".into(),
            name: "Living Room".into(),
            model: "Chromecast".into(),
            host: "192.168.1.10".into(),
            port: 8009,
        }
    );
    let bare = device_from_record(&ServiceRecord {
        fullname: "Kitchen._googlecast._tcp.local.".into(),
        host: "192.168.1.11".into(),
        port: 8009,
        txt: HashMap::new(),
    })
    .expect("bare");
    assert_eq!(bare.name, "Kitchen");
    assert_eq!(bare.model, "");
    assert!(bare.id.contains("Kitchen"));
    assert!(
        device_from_record(&ServiceRecord {
            fullname: "x.".into(),
            host: "".into(),
            port: 8009,
            txt: HashMap::new(),
        })
        .is_none()
    );
}
