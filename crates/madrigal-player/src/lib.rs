pub mod engine;
pub mod player;
pub mod queue;
pub mod session;
pub mod source;

use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    Mpv(String),
    Io(io::Error),
    Resolve(String),
    NoTrack,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Mpv(detail) => write!(f, "mpv: {detail}"),
            Error::Io(err) => write!(f, "io: {err}"),
            Error::Resolve(detail) => write!(f, "resolve: {detail}"),
            Error::NoTrack => write!(f, "queue is empty"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}
