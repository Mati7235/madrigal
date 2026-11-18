use std::fmt;
use std::path::Path;

use lofty::file::TaggedFileExt;
use lofty::id3::v2::{FrameFlags, SynchronizedTextFrame, TimestampFormat};
use lofty::tag::ItemKey;

use crate::{lrc, Lyrics};
use crate::sylt::{self, SyltEntry};

#[derive(Debug)]
pub enum EmbeddedError {
    Io(std::io::Error),
    Lofty(String),
}

impl fmt::Display for EmbeddedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "embedded lyrics io: {error}"),
            Self::Lofty(message) => write!(formatter, "embedded lyrics tag: {message}"),
        }
    }
}

impl std::error::Error for EmbeddedError {}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmbeddedPair {
    pub synced: Option<Lyrics>,
    pub unsynced: Option<String>,
}

pub fn extract(path: &Path) -> Result<EmbeddedPair, EmbeddedError> {
    let tagged = lofty::read_from_path(path).map_err(|error| EmbeddedError::Lofty(error.to_string()))?;
    let mut unsynced = None;
    for tag in tagged.tags() {
        let candidate = lyrics_from_tag(tag).filter(|text| !text.trim().is_empty());
        if unsynced.is_none() {
            unsynced = candidate;
        }
    }
    let synced = sylt_entries_from_file(path)
        .ok()
        .and_then(|entries| sylt::parse(&entries).ok());
    Ok(EmbeddedPair { synced, unsynced })
}

pub fn extract_lyrics(path: &Path) -> Result<Option<Lyrics>, EmbeddedError> {
    let pair = extract(path)?;
    if let Some(synced) = pair.synced {
        return Ok(Some(synced));
    }
    Ok(pair.unsynced.map(|text| lrc::parse(&text)))
}

pub fn lyrics_from_tag(tag: &lofty::tag::Tag) -> Option<String> {
    tag.get_string(&ItemKey::Lyrics).map(str::to_string)
}

pub fn sylt_entries_from_file(path: &Path) -> Result<Vec<SyltEntry>, EmbeddedError> {
    let bytes = std::fs::read(path).map_err(EmbeddedError::Io)?;
    Ok(sylt_entries_from_bytes(&bytes))
}

pub fn sylt_entries_from_bytes(bytes: &[u8]) -> Vec<SyltEntry> {
    let mut entries = Vec::new();
    for payload in sylt_payloads(bytes) {
        let frame = SynchronizedTextFrame::parse(payload, FrameFlags::default());
        let Ok(frame) = frame else { continue };
        if frame.timestamp_format != TimestampFormat::MS {
            continue;
        }
        for (stamp, text) in &frame.content {
            let text = text.trim();
            if !text.is_empty() {
                entries.push(SyltEntry::new(*stamp as i64, text));
            }
        }
    }
    entries.sort_by_key(|entry| entry.timestamp_ms);
    entries
}

fn sylt_payloads(bytes: &[u8]) -> Vec<&[u8]> {
    let mut payloads = Vec::new();
    if bytes.len() < 10 || &bytes[0..3] != b"ID3" {
        return payloads;
    }
    let version = bytes[3];
    if version != 3 && version != 4 {
        return payloads;
    }
    if bytes[5] & 0x80 != 0 || bytes[5] & 0x40 != 0 {
        return payloads;
    }
    let Some(tag_end) = syncsafe(&bytes[6..10]).map(|size| 10 + size as usize) else {
        return payloads;
    };
    let tag_end = tag_end.min(bytes.len());
    let mut pos = 10;
    while pos + 10 <= tag_end {
        let id = &bytes[pos..pos + 4];
        if id[0] == 0 {
            break;
        }
        let size = if version == 4 {
            match syncsafe(&bytes[pos + 4..pos + 8]) {
                Some(size) => size as usize,
                None => break,
            }
        } else {
            u32::from_be_bytes([bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]])
                as usize
        };
        let flags = u16::from_be_bytes([bytes[pos + 8], bytes[pos + 9]]);
        let start = pos + 10;
        let Some(stop) = start.checked_add(size).filter(|stop| *stop <= tag_end) else {
            break;
        };
        if id == b"SYLT" && flags & 0x000E == 0 {
            payloads.push(&bytes[start..stop]);
        }
        pos = stop;
    }
    payloads
}

fn syncsafe(bytes: &[u8]) -> Option<u32> {
    if bytes.iter().any(|byte| *byte & 0x80 != 0) {
        return None;
    }
    Some(
        (bytes[0] as u32) << 21
            | (bytes[1] as u32) << 14
            | (bytes[2] as u32) << 7
            | bytes[3] as u32,
    )
}
