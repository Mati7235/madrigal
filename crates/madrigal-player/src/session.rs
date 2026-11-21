use std::io;
use std::path::{Path, PathBuf};

use crate::queue::QueueEntry;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    pub tracks: Vec<QueueEntry>,
    pub position: Option<usize>,
    pub paused: bool,
}

impl SessionState {
    pub fn empty() -> Self {
        Self {
            tracks: Vec::new(),
            position: None,
            paused: true,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::empty()
    }
}

pub fn session_path() -> PathBuf {
    madrigal_paths::data_dir().join("player-session.json")
}

pub fn save(state: &SessionState) -> io::Result<PathBuf> {
    save_to(state, &session_path())
}

pub fn save_to(state: &SessionState, path: &Path) -> io::Result<PathBuf> {
    let text = serde_json::to_string_pretty(state)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    madrigal_files::atomic_write_text(path, &text)
}

pub fn load() -> io::Result<Option<SessionState>> {
    load_from(&session_path())
}

pub fn load_from(path: &Path) -> io::Result<Option<SessionState>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let mut state: SessionState = serde_json::from_str(&text)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    if state.position.is_some_and(|pos| pos >= state.tracks.len()) {
        state.position = None;
    }
    Ok(Some(state))
}
