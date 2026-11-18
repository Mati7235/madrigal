use std::fmt;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde_json::Value;

pub const SOCKET_PREFIX: &str = "discord-ipc-";
pub const SOCKET_COUNT: u32 = 10;
pub const OP_HANDSHAKE: u32 = 0;
pub const OP_FRAME: u32 = 1;
const MAX_PAYLOAD: u32 = 1024 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq, Eq)]
pub enum IpcError {
    NotRunning,
    Io(String),
    Malformed(String),
    Refused(String),
}

impl fmt::Display for IpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IpcError::NotRunning => write!(f, "discord client not running"),
            IpcError::Io(detail) => write!(f, "ipc io failure: {detail}"),
            IpcError::Malformed(detail) => write!(f, "malformed ipc payload: {detail}"),
            IpcError::Refused(detail) => write!(f, "request refused: {detail}"),
        }
    }
}

impl std::error::Error for IpcError {}

impl From<io::Error> for IpcError {
    fn from(err: io::Error) -> Self {
        IpcError::Io(err.to_string())
    }
}

impl From<serde_json::Error> for IpcError {
    fn from(err: serde_json::Error) -> Self {
        IpcError::Malformed(err.to_string())
    }
}

pub fn socket_paths() -> Vec<std::path::PathBuf> {
    (0..SOCKET_COUNT)
        .map(|index| madrigal_paths::runtime_dir().join(format!("{SOCKET_PREFIX}{index}")))
        .collect()
}

fn write_frame(stream: &mut UnixStream, opcode: u32, payload: &[u8]) -> Result<(), IpcError> {
    let mut header = [0u8; 8];
    header[..4].copy_from_slice(&opcode.to_le_bytes());
    header[4..].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    stream.write_all(&header)?;
    stream.write_all(payload)?;
    stream.flush()?;
    Ok(())
}

fn read_frame(stream: &mut UnixStream) -> Result<(u32, Value), IpcError> {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header).map_err(|err| {
        if err.kind() == io::ErrorKind::UnexpectedEof {
            IpcError::Malformed("truncated ipc header".to_owned())
        } else {
            IpcError::Io(err.to_string())
        }
    })?;
    let opcode = u32::from_le_bytes(header[..4].try_into().expect("slice len"));
    let length = u32::from_le_bytes(header[4..].try_into().expect("slice len"));
    if length > MAX_PAYLOAD {
        return Err(IpcError::Malformed(format!("ipc payload too large: {length}")));
    }
    let mut payload = vec![0u8; length as usize];
    stream.read_exact(&mut payload).map_err(|err| {
        if err.kind() == io::ErrorKind::UnexpectedEof {
            IpcError::Malformed("truncated ipc payload".to_owned())
        } else {
            IpcError::Io(err.to_string())
        }
    })?;
    let value: Value = serde_json::from_slice(&payload)?;
    Ok((opcode, value))
}

fn refusal(value: &Value) -> Option<String> {
    value
        .get("data")
        .and_then(|data| data.get("code"))
        .map(|_| {
            value
                .get("data")
                .and_then(|data| data.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("unknown refusal")
                .to_owned()
        })
}

pub struct IpcClient {
    stream: UnixStream,
    nonce: u64,
}

impl IpcClient {
    pub fn connect() -> Result<Self, IpcError> {
        for path in socket_paths() {
            if let Ok(stream) = UnixStream::connect(&path) {
                stream.set_read_timeout(Some(IO_TIMEOUT))?;
                stream.set_write_timeout(Some(IO_TIMEOUT))?;
                return Ok(Self { stream, nonce: 0 });
            }
        }
        Err(IpcError::NotRunning)
    }

    pub fn connect_path(path: &std::path::Path) -> Result<Self, IpcError> {
        match UnixStream::connect(path) {
            Ok(stream) => {
                stream.set_read_timeout(Some(IO_TIMEOUT))?;
                stream.set_write_timeout(Some(IO_TIMEOUT))?;
                Ok(Self { stream, nonce: 0 })
            }
            Err(_) => Err(IpcError::NotRunning),
        }
    }

    fn next_nonce(&mut self) -> String {
        self.nonce += 1;
        format!("madrigal-{}", self.nonce)
    }

    fn round_trip(&mut self, opcode: u32, body: &Value) -> Result<Value, IpcError> {
        let raw = serde_json::to_vec(body)?;
        write_frame(&mut self.stream, opcode, &raw)?;
        let (_, value) = read_frame(&mut self.stream)?;
        if !value.is_object() {
            return Err(IpcError::Malformed("ipc response is not an object".to_owned()));
        }
        if let Some(detail) = refusal(&value) {
            return Err(IpcError::Refused(detail));
        }
        Ok(value)
    }

    pub fn handshake(&mut self, client_id: &str) -> Result<Value, IpcError> {
        let body = serde_json::json!({"v": 1, "client_id": client_id});
        self.round_trip(OP_HANDSHAKE, &body)
    }

    pub fn set_activity(&mut self, activity: &Value) -> Result<Value, IpcError> {
        let nonce = self.next_nonce();
        let body = serde_json::json!({
            "cmd": "SET_ACTIVITY",
            "args": {"pid": std::process::id(), "activity": activity},
            "nonce": nonce,
        });
        self.round_trip(OP_FRAME, &body)
    }

    pub fn clear_activity(&mut self) -> Result<Value, IpcError> {
        let nonce = self.next_nonce();
        let body = serde_json::json!({
            "cmd": "SET_ACTIVITY",
            "args": {"pid": std::process::id(), "activity": Value::Null},
            "nonce": nonce,
        });
        self.round_trip(OP_FRAME, &body)
    }
}
