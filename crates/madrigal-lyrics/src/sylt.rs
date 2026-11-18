use serde::{Deserialize, Serialize};

use crate::{LineMarkers, LyricLine, Lyrics, ParseError, WordTiming};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyltEntry {
    pub timestamp_ms: i64,
    pub text: String,
}

impl SyltEntry {
    pub fn new(timestamp_ms: i64, text: impl Into<String>) -> Self {
        Self {
            timestamp_ms,
            text: text.into(),
        }
    }
}

pub fn parse(entries: &[SyltEntry]) -> Result<Lyrics, ParseError> {
    if entries.is_empty() {
        return Err(ParseError::EmptyInput);
    }
    let mut ordered: Vec<&SyltEntry> = entries
        .iter()
        .filter(|entry| !entry.text.trim().is_empty())
        .collect();
    if ordered.is_empty() {
        return Err(ParseError::NoSyncedLines);
    }
    ordered.sort_by_key(|entry| entry.timestamp_ms);
    let lines = ordered
        .into_iter()
        .map(|entry| {
            let text = entry.text.trim().to_string();
            LyricLine {
                start_ms: entry.timestamp_ms,
                end_ms: None,
                words: Some(vec![WordTiming {
                    start_ms: entry.timestamp_ms,
                    end_ms: None,
                    text: text.clone(),
                }]),
                markers: LineMarkers::default(),
                text,
            }
        })
        .collect();
    Ok(Lyrics {
        lines,
        synced: true,
    })
}
