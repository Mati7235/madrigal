use serde_json::{Value, json};

use crate::{Error, Result};

pub const DEFAULT_MEDIA_RECEIVER_APP_ID: &str = "CC1AD845";
pub const DEFAULT_PORT: u16 = 8009;
pub const SERVICE_TYPE: &str = "_googlecast._tcp.local.";

pub const NS_CONNECTION: &str = "urn:x-cast:com.google.cast.tp.connection";
pub const NS_HEARTBEAT: &str = "urn:x-cast:com.google.cast.tp.heartbeat";
pub const NS_RECEIVER: &str = "urn:x-cast:com.google.cast.receiver";
pub const NS_MEDIA: &str = "urn:x-cast:com.google.cast.media";

pub const SENDER_ID: &str = "sender-0";
pub const RECEIVER_ID: &str = "receiver-0";

const MAX_FRAME_BODY: u32 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastMessage {
    pub source: String,
    pub dest: String,
    pub namespace: String,
    pub payload: String,
}

impl CastMessage {
    pub fn new(
        source: impl Into<String>,
        dest: impl Into<String>,
        namespace: impl Into<String>,
        payload: Value,
    ) -> Result<Self> {
        Ok(Self {
            source: source.into(),
            dest: dest.into(),
            namespace: namespace.into(),
            payload: serde_json::to_string(&payload)?,
        })
    }

    pub fn payload_value(&self) -> Result<Value> {
        serde_json::from_str(&self.payload).map_err(Error::from)
    }
}

fn encode_varint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn decode_varint(bytes: &[u8]) -> Result<(u64, usize)> {
    let mut value: u64 = 0;
    for (index, byte) in bytes.iter().enumerate().take(10) {
        let part = u64::from(byte & 0x7f);
        value |= part
            .checked_shl(u32::try_from(index * 7).map_err(|_| Error::Malformed("shift".into()))?)
            .ok_or_else(|| Error::Malformed("varint overflow".into()))?;
        if byte & 0x80 == 0 {
            return Ok((value, index + 1));
        }
    }
    Err(Error::Malformed("varint too long".into()))
}

fn encode_field_string(field: u32, text: &str, out: &mut Vec<u8>) {
    encode_varint(u64::from(field << 3 | 2), out);
    encode_varint(text.len() as u64, out);
    out.extend_from_slice(text.as_bytes());
}

fn encode_field_varint(field: u32, value: u64, out: &mut Vec<u8>) {
    encode_varint(u64::from(field << 3), out);
    encode_varint(value, out);
}

pub fn encode_proto(message: &CastMessage) -> Vec<u8> {
    let mut out = Vec::new();
    encode_field_varint(1, 0, &mut out);
    encode_field_string(2, &message.source, &mut out);
    encode_field_string(3, &message.dest, &mut out);
    encode_field_string(4, &message.namespace, &mut out);
    encode_field_varint(5, 0, &mut out);
    encode_field_string(6, &message.payload, &mut out);
    out
}

fn skip_field(wire: u64, bytes: &[u8]) -> Result<usize> {
    match wire {
        0 => decode_varint(bytes).map(|(_, used)| used),
        1 => {
            if bytes.len() < 8 {
                return Err(Error::Malformed("truncated fixed64".into()));
            }
            Ok(8)
        }
        2 => {
            let (length, used) = decode_varint(bytes)?;
            let length = usize::try_from(length).map_err(|_| Error::Malformed("length".into()))?;
            if bytes.len() < used + length {
                return Err(Error::Malformed("truncated field".into()));
            }
            Ok(used + length)
        }
        5 => {
            if bytes.len() < 4 {
                return Err(Error::Malformed("truncated fixed32".into()));
            }
            Ok(4)
        }
        _ => Err(Error::Malformed("unsupported wire type".into())),
    }
}

pub fn decode_proto(bytes: &[u8]) -> Result<CastMessage> {
    let mut source: Option<String> = None;
    let mut dest: Option<String> = None;
    let mut namespace: Option<String> = None;
    let mut payload: Option<String> = None;
    let mut version: Option<u64> = None;
    let mut payload_type: Option<u64> = None;
    let mut position = 0;
    while position < bytes.len() {
        let (tag, used) = decode_varint(&bytes[position..]).map_err(|_| Error::Malformed("tag".into()))?;
        position += used;
        let field = tag >> 3;
        let wire = tag & 7;
        match (u32::try_from(field).map_err(|_| Error::Malformed("field".into()))?, wire) {
            (1, 0) => {
                let (value, used) =
                    decode_varint(&bytes[position..]).map_err(|_| Error::Malformed("version".into()))?;
                version = Some(value);
                position += used;
            }
            (2, 2) | (3, 2) | (4, 2) | (6, 2) => {
                let (length, used) =
                    decode_varint(&bytes[position..]).map_err(|_| Error::Malformed("length".into()))?;
                let length =
                    usize::try_from(length).map_err(|_| Error::Malformed("length".into()))?;
                position += used;
                let end = position + length;
                if end > bytes.len() {
                    return Err(Error::Malformed("truncated string".into()));
                }
                let text = std::str::from_utf8(&bytes[position..end])
                    .map_err(|_| Error::Malformed("payload not utf8".into()))?
                    .to_owned();
                match field {
                    2 => source = Some(text),
                    3 => dest = Some(text),
                    4 => namespace = Some(text),
                    _ => payload = Some(text),
                }
                position = end;
            }
            (5, 0) => {
                let (value, used) =
                    decode_varint(&bytes[position..]).map_err(|_| Error::Malformed("type".into()))?;
                payload_type = Some(value);
                position += used;
            }
            _ => {
                let skipped = skip_field(wire, &bytes[position..])?;
                position += skipped;
            }
        }
    }
    if version != Some(0) {
        return Err(Error::Malformed("protocol version".into()));
    }
    if payload_type == Some(1) {
        return Err(Error::Malformed("binary payload unsupported".into()));
    }
    if payload_type != Some(0) {
        return Err(Error::Malformed("payload type".into()));
    }
    Ok(CastMessage {
        source: source.ok_or_else(|| Error::Malformed("source".into()))?,
        dest: dest.ok_or_else(|| Error::Malformed("dest".into()))?,
        namespace: namespace.ok_or_else(|| Error::Malformed("namespace".into()))?,
        payload: payload.ok_or_else(|| Error::Malformed("payload".into()))?,
    })
}

pub fn encode_frame(message: &CastMessage) -> Vec<u8> {
    let body = encode_proto(message);
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&u32::try_from(body.len()).unwrap_or(u32::MAX).to_be_bytes());
    out.extend_from_slice(&body);
    out
}

pub fn decode_frame(bytes: &[u8]) -> Result<(CastMessage, usize)> {
    if bytes.len() < 4 {
        return Err(Error::NeedMore);
    }
    let length =
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if length > MAX_FRAME_BODY {
        return Err(Error::Malformed("frame too large".into()));
    }
    let length = length as usize;
    if bytes.len() < 4 + length {
        return Err(Error::NeedMore);
    }
    let message = decode_proto(&bytes[4..4 + length])?;
    Ok((message, 4 + length))
}

pub fn read_frame(reader: &mut impl std::io::Read) -> Result<CastMessage> {
    let mut prefix = [0u8; 4];
    let mut filled = 0;
    while filled < 4 {
        let read = reader
            .read(&mut prefix[filled..])
            .map_err(|error| Error::Transport(error.to_string()))?;
        if read == 0 {
            return Err(Error::Transport("connection closed".into()));
        }
        filled += read;
    }
    let length = u32::from_be_bytes(prefix);
    if length > MAX_FRAME_BODY {
        return Err(Error::Malformed("frame too large".into()));
    }
    let mut body = vec![0u8; length as usize];
    let mut done = 0;
    while done < body.len() {
        let read = reader
            .read(&mut body[done..])
            .map_err(|error| Error::Transport(error.to_string()))?;
        if read == 0 {
            return Err(Error::Transport("connection closed".into()));
        }
        done += read;
    }
    decode_proto(&body)
}

pub fn connect() -> Result<CastMessage> {
    CastMessage::new(SENDER_ID, RECEIVER_ID, NS_CONNECTION, json!({"type": "CONNECT"}))
}

pub fn close(dest: &str) -> Result<CastMessage> {
    CastMessage::new(SENDER_ID, dest, NS_CONNECTION, json!({"type": "CLOSE"}))
}

pub fn ping() -> Result<CastMessage> {
    CastMessage::new(SENDER_ID, RECEIVER_ID, NS_HEARTBEAT, json!({"type": "PING"}))
}

pub fn pong() -> Result<CastMessage> {
    CastMessage::new(SENDER_ID, RECEIVER_ID, NS_HEARTBEAT, json!({"type": "PONG"}))
}

pub fn is_ping(message: &CastMessage) -> bool {
    message.namespace == NS_HEARTBEAT
        && message.payload_value().is_ok_and(|value| value.get("type") == Some(&Value::from("PING")))
}

pub fn get_status(request_id: u32) -> Result<CastMessage> {
    CastMessage::new(
        SENDER_ID,
        RECEIVER_ID,
        NS_RECEIVER,
        json!({"type": "GET_STATUS", "requestId": request_id}),
    )
}

pub fn launch(app_id: &str, request_id: u32) -> Result<CastMessage> {
    CastMessage::new(
        SENDER_ID,
        RECEIVER_ID,
        NS_RECEIVER,
        json!({"type": "LAUNCH", "appId": app_id, "requestId": request_id}),
    )
}

pub fn stop_app(session_id: &str, request_id: u32) -> Result<CastMessage> {
    CastMessage::new(
        SENDER_ID,
        RECEIVER_ID,
        NS_RECEIVER,
        json!({"type": "STOP", "sessionId": session_id, "requestId": request_id}),
    )
}

pub fn set_volume(level: f32, request_id: u32) -> Result<CastMessage> {
    CastMessage::new(
        SENDER_ID,
        RECEIVER_ID,
        NS_RECEIVER,
        json!({"type": "SET_VOLUME", "volume": {"level": level.clamp(0.0, 1.0)}, "requestId": request_id}),
    )
}

pub fn media_stop(dest: &str, media_session_id: i64, request_id: u32) -> Result<CastMessage> {
    CastMessage::new(
        SENDER_ID,
        dest,
        NS_MEDIA,
        json!({"type": "STOP", "mediaSessionId": media_session_id, "requestId": request_id}),
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppSession {
    pub session_id: String,
    pub transport_id: String,
    pub app_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReceiverStatus {
    pub applications: Vec<AppSession>,
    pub volume_level: Option<f32>,
    pub volume_muted: Option<bool>,
}

fn payload_type_is(payload: &Value, expected: &str) -> bool {
    payload.get("type") == Some(&Value::from(expected))
}

pub fn parse_receiver_status(payload: &str) -> Result<ReceiverStatus> {
    let value: Value = serde_json::from_str(payload)?;
    if !payload_type_is(&value, "RECEIVER_STATUS") {
        return Err(Error::UnexpectedType("expected RECEIVER_STATUS".into()));
    }
    let status = value.get("status").ok_or_else(|| Error::Malformed("status".into()))?;
    let mut applications = Vec::new();
    if let Some(list) = status.get("applications").and_then(Value::as_array) {
        for app in list {
            applications.push(AppSession {
                session_id: app
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::Malformed("sessionId".into()))?
                    .to_owned(),
                transport_id: app
                    .get("transportId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::Malformed("transportId".into()))?
                    .to_owned(),
                app_id: app
                    .get("appId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::Malformed("appId".into()))?
                    .to_owned(),
            });
        }
    }
    let volume_level = status
        .get("volume")
        .and_then(|volume| volume.get("level"))
        .and_then(Value::as_f64)
        .map(|level| level as f32);
    let volume_muted = status
        .get("volume")
        .and_then(|volume| volume.get("muted"))
        .and_then(Value::as_bool);
    Ok(ReceiverStatus { applications, volume_level, volume_muted })
}

pub fn parse_media_session_ids(payload: &str) -> Result<Vec<i64>> {
    let value: Value = serde_json::from_str(payload)?;
    if !payload_type_is(&value, "MEDIA_STATUS") {
        return Err(Error::UnexpectedType("expected MEDIA_STATUS".into()));
    }
    let mut ids = Vec::new();
    if let Some(list) = value.get("status").and_then(Value::as_array) {
        for entry in list {
            if let Some(id) = entry.get("mediaSessionId").and_then(Value::as_i64) {
                ids.push(id);
            }
        }
    }
    if ids.is_empty() {
        return Err(Error::Malformed("mediaSessionId".into()));
    }
    Ok(ids)
}
