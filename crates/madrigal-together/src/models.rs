use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use madrigal_core::Track;
use serde::{Deserialize, Serialize};

pub const ACTIVITY_CAP: usize = 256;

static COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn new_id(prefix: &str) -> String {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut h = DefaultHasher::new();
    (std::process::id() as u64, now, n).hash(&mut h);
    format!("{prefix}-{:016x}", h.finish())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Host,
    Guest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    pub id: String,
    pub name: String,
    pub role: Role,
    pub pending: bool,
    pub connected: bool,
}

impl Participant {
    pub fn is_host(&self) -> bool {
        self.role == Role::Host
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomFlags {
    pub guests_can_add: bool,
    pub guests_can_control: bool,
    pub require_approval: bool,
}

impl Default for RoomFlags {
    fn default() -> Self {
        Self {
            guests_can_add: true,
            guests_can_control: false,
            require_approval: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PlayState {
    pub is_playing: bool,
    pub position_ms: u64,
    pub track_index: usize,
    pub version: u64,
}

impl PlayState {
    pub fn accept(&mut self, candidate: &PlayState) -> bool {
        if candidate.version > self.version {
            *self = *candidate;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    Join,
    Leave,
    Approve,
    Reject,
    Kick,
    Ban,
    Transfer,
    QueueChange,
    PlayState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub seq: u64,
    pub kind: ActivityKind,
    pub actor_id: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub session_id: String,
    pub code: String,
    pub host_id: String,
    pub participants: Vec<Participant>,
    pub flags: RoomFlags,
    pub queue: Vec<Track>,
    pub play: PlayState,
    pub repeat_mode: u8,
    pub shuffle: bool,
    pub welcome_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionCode(pub String);

impl SessionCode {
    pub fn generate() -> Self {
        const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut h = DefaultHasher::new();
        (std::process::id() as u64, now, n).hash(&mut h);
        let mut x = h.finish() | 1;
        let mut out = String::with_capacity(6);
        for _ in 0..6 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            out.push(ALPHABET[(x % ALPHABET.len() as u64) as usize] as char);
        }
        Self(out)
    }

    pub fn normalize(raw: &str) -> Option<Self> {
        let code: String = raw
            .trim()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .to_uppercase();
        if (4..=12).contains(&code.len())
            && code.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            Some(Self(code))
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
