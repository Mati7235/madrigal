use madrigal_core::Track;
use serde::{Deserialize, Serialize};

use crate::models::{Participant, PlayState, RoomFlags, SessionSnapshot};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddMode {
    PlayNext,
    AddToQueue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ControlAction {
    Play,
    Pause,
    SeekTo { position_ms: u64 },
    SkipNext,
    SkipPrevious,
    SeekToIndex { index: usize, position_ms: u64 },
    SeekToTrack { track_id: String, position_ms: u64 },
    SetRepeatMode { repeat_mode: u8 },
    SetShuffleEnabled { shuffle_enabled: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    ClientHello {
        protocol_version: u32,
        session_id: String,
        session_key: String,
        client_id: String,
        display_name: String,
    },
    ServerWelcome {
        protocol_version: u32,
        session_id: String,
        participant_id: String,
        role: String,
        pending: bool,
        welcome_message: String,
        flags: RoomFlags,
    },
    ServerError {
        session_id: Option<String>,
        message: String,
        code: Option<String>,
    },
    RoomState {
        state: SessionSnapshot,
    },
    ControlRequest {
        session_id: String,
        participant_id: String,
        action: ControlAction,
    },
    AddTrackRequest {
        session_id: String,
        participant_id: String,
        track: Track,
        mode: AddMode,
    },
    QueueMove {
        session_id: String,
        participant_id: String,
        from: usize,
        to: usize,
    },
    QueueRemove {
        session_id: String,
        participant_id: String,
        index: usize,
    },
    JoinDecision {
        session_id: String,
        participant_id: String,
        approved: bool,
    },
    JoinRequest {
        session_id: String,
        participant: Participant,
    },
    ParticipantJoined {
        session_id: String,
        participant: Participant,
    },
    ParticipantLeft {
        session_id: String,
        participant_id: String,
        reason: Option<String>,
    },
    HostTransfer {
        session_id: String,
        participant_id: String,
    },
    HostTransferred {
        session_id: String,
        participant_id: String,
    },
    HeartbeatPing {
        session_id: String,
        ping_id: u64,
    },
    HeartbeatPong {
        session_id: String,
        ping_id: u64,
    },
    ClientLeave {
        session_id: String,
        participant_id: String,
    },
    Kick {
        session_id: String,
        participant_id: String,
        reason: Option<String>,
    },
    Ban {
        session_id: String,
        participant_id: String,
        reason: Option<String>,
    },
    PlayStateSubmit {
        session_id: String,
        participant_id: String,
        play: PlayState,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtoError {
    Empty,
    Garbage(String),
    VersionMismatch(u32),
}

impl std::fmt::Display for ProtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtoError::Empty => f.write_str("empty frame"),
            ProtoError::Garbage(line) => write!(f, "undecodable frame: {line}"),
            ProtoError::VersionMismatch(v) => write!(f, "unsupported protocol version {v}"),
        }
    }
}

impl std::error::Error for ProtoError {}

pub fn frame(message: &Message) -> String {
    let mut out = serde_json::to_string(message).unwrap_or_else(|_| {
        String::from("{\"type\":\"server_error\",\"message\":\"encode failed\"}")
    });
    out.push('\n');
    out
}

pub fn decode_line(line: &str) -> Result<Message, ProtoError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(ProtoError::Empty);
    }
    let message: Message = serde_json::from_str(trimmed)
        .map_err(|_| ProtoError::Garbage(trimmed.chars().take(96).collect()))?;
    check_version(&message)?;
    Ok(message)
}

pub fn check_version(message: &Message) -> Result<(), ProtoError> {
    let version = match message {
        Message::ClientHello {
            protocol_version, ..
        }
        | Message::ServerWelcome {
            protocol_version, ..
        } => Some(*protocol_version),
        _ => None,
    };
    match version {
        Some(v) if v == PROTOCOL_VERSION => Ok(()),
        Some(v) => Err(ProtoError::VersionMismatch(v)),
        None => Ok(()),
    }
}

pub fn server_error(
    session_id: Option<&str>,
    code: &str,
    message: impl Into<String>,
) -> Message {
    Message::ServerError {
        session_id: session_id.map(str::to_owned),
        message: message.into(),
        code: Some(code.to_owned()),
    }
}
