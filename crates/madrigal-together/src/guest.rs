use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use madrigal_core::Track;

use crate::link::JoinInfo;
use crate::proto::{AddMode, ControlAction, Message, decode_line, frame};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

pub struct ConnectedGuest {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    pub participant_id: String,
    pub pending: bool,
    pub session_id: String,
}

impl ConnectedGuest {
    pub fn connect(join: &JoinInfo, display_name: &str) -> io::Result<Self> {
        Self::connect_with_id(join, display_name, &crate::models::new_id("client"))
    }

    pub fn connect_with_id(
        join: &JoinInfo,
        display_name: &str,
        client_id: &str,
    ) -> io::Result<Self> {
        let addr = format!("{}:{}", join.host, join.port);
        let stream = TcpStream::connect(addr)?;
        stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
        stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;
        let mut writer = stream.try_clone()?;
        let hello = Message::ClientHello {
            protocol_version: crate::proto::PROTOCOL_VERSION,
            session_id: join.session_id.clone(),
            session_key: join.session_key.clone(),
            client_id: client_id.to_owned(),
            display_name: display_name.to_owned(),
        };
        writer.write_all(frame(&hello).as_bytes())?;
        writer.flush()?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line.trim().is_empty() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "empty welcome"));
        }
        match decode_line(&line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))? {
            Message::ServerWelcome {
                session_id,
                participant_id,
                pending,
                ..
            } => {
                if session_id != join.session_id {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "session mismatch",
                    ));
                }
                Ok(Self {
                    reader,
                    writer,
                    participant_id,
                    pending,
                    session_id,
                })
            }
            Message::ServerError { message, .. } => Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                message,
            )),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("expected welcome, got {line:?}"),
            )),
        }
    }

    pub fn send(&mut self, message: &Message) -> io::Result<()> {
        self.writer.write_all(frame(message).as_bytes())?;
        self.writer.flush()
    }

    pub fn send_raw(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writer.write_all(bytes)?;
        self.writer.flush()
    }

    pub fn recv(&mut self, timeout: Duration) -> io::Result<Message> {
        self.reader
            .get_ref()
            .set_read_timeout(Some(timeout))?;
        let mut line = String::new();
        loop {
            line.clear();
            match self.reader.read_line(&mut line) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "connection closed",
                    ));
                }
                Ok(_) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    return decode_line(&line)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
                }
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::TimedOut =>
                {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "recv timeout"));
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub fn request_add(&mut self, track: Track, mode: AddMode) -> io::Result<()> {
        self.send(&Message::AddTrackRequest {
            session_id: self.session_id.clone(),
            participant_id: self.participant_id.clone(),
            track,
            mode,
        })
    }

    pub fn request_control(&mut self, action: ControlAction) -> io::Result<()> {
        self.send(&Message::ControlRequest {
            session_id: self.session_id.clone(),
            participant_id: self.participant_id.clone(),
            action,
        })
    }

    pub fn request_move(&mut self, from: usize, to: usize) -> io::Result<()> {
        self.send(&Message::QueueMove {
            session_id: self.session_id.clone(),
            participant_id: self.participant_id.clone(),
            from,
            to,
        })
    }

    pub fn request_remove(&mut self, index: usize) -> io::Result<()> {
        self.send(&Message::QueueRemove {
            session_id: self.session_id.clone(),
            participant_id: self.participant_id.clone(),
            index,
        })
    }

    pub fn submit_play(&mut self, play: crate::models::PlayState) -> io::Result<()> {
        self.send(&Message::PlayStateSubmit {
            session_id: self.session_id.clone(),
            participant_id: self.participant_id.clone(),
            play,
        })
    }

    pub fn ping(&mut self, ping_id: u64) -> io::Result<()> {
        self.send(&Message::HeartbeatPing {
            session_id: self.session_id.clone(),
            ping_id,
        })
    }

    pub fn leave(mut self) {
        let _ = self.send(&Message::ClientLeave {
            session_id: self.session_id.clone(),
            participant_id: self.participant_id.clone(),
        });
    }
}
