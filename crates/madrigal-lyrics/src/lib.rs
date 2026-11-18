pub mod embedded;
pub mod karaoke;
pub mod lrc;
pub mod lrc_enhanced;
pub mod share_image;
pub mod sylt;
pub mod translation;
pub mod ttml;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    EmptyInput,
    NoSyncedLines,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordTiming {
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineMarkers {
    pub background: Vec<String>,
    pub translation: Option<String>,
    pub romanization: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricLine {
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub text: String,
    pub words: Option<Vec<WordTiming>>,
    pub markers: LineMarkers,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    pub synced: bool,
}

impl Lyrics {
    pub fn plain_text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn has_words(&self) -> bool {
        self.lines
            .iter()
            .any(|line| line.words.as_ref().is_some_and(|words| !words.is_empty()))
    }
}

pub(crate) fn secs_to_ms(seconds: f64) -> i64 {
    (seconds * 1000.0).round() as i64
}

pub fn format_timestamp(start_ms: i64) -> String {
    let clamped = start_ms.max(0);
    let minutes = clamped / 60_000;
    let seconds = (clamped % 60_000) as f64 / 1000.0;
    format!("{minutes:02}:{seconds:05.2}")
}

pub fn to_lrc(lyrics: &Lyrics) -> String {
    lyrics
        .lines
        .iter()
        .filter(|line| !line.text.is_empty())
        .map(|line| format!("[{}]{}", format_timestamp(line.start_ms), with_word_marks(line)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn with_word_marks(line: &LyricLine) -> String {
    match &line.words {
        Some(words) if !words.is_empty() => words
            .iter()
            .enumerate()
            .map(|(index, word)| {
                let gap = if index == 0 { "" } else { " " };
                format!("{gap}<{}>{}", format_timestamp(word.start_ms), word.text)
            })
            .collect::<String>(),
        _ => line.text.clone(),
    }
}
