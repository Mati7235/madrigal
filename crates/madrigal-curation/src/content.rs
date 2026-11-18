use madrigal_core::{Track, TrackSource};
use serde::Serialize;

const BROWSE_PREFIXES: [&str; 7] = ["UC", "PL", "VL", "RD", "OLAK", "MPRE", "FE"];
const VIDEO_ID_LEN: usize = 11;

fn is_base64url(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

pub fn is_video_id_format(id: &str) -> bool {
    if id.starts_with("tidal:") || id.starts_with("local:") {
        return false;
    }
    if BROWSE_PREFIXES.iter().any(|p| id.starts_with(p)) {
        return false;
    }
    id.len() == VIDEO_ID_LEN && id.bytes().all(is_base64url)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentFlags {
    pub explicit: bool,
    pub is_video: bool,
}

impl ContentFlags {
    pub fn clean() -> Self {
        Self {
            explicit: false,
            is_video: false,
        }
    }
}

pub fn flags_of(track: &Track) -> ContentFlags {
    ContentFlags {
        explicit: false,
        is_video: track.source() == TrackSource::Youtube
            && is_video_id_format(&track.identity.id),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ContentReport {
    pub explicit_hidden: usize,
    pub video_hidden: usize,
}

impl ContentReport {
    pub fn total(&self) -> usize {
        self.explicit_hidden + self.video_hidden
    }
}

pub fn apply_content_filters(
    tracks: Vec<Track>,
    flags: impl Fn(&Track) -> ContentFlags,
    hide_explicit: bool,
    hide_video: bool,
) -> (Vec<Track>, ContentReport) {
    let mut report = ContentReport::default();
    if !hide_explicit && !hide_video {
        return (tracks, report);
    }
    let mut kept = Vec::with_capacity(tracks.len());
    for track in tracks {
        let flag = flags(&track);
        if hide_explicit && flag.explicit {
            report.explicit_hidden += 1;
            continue;
        }
        if hide_video && flag.is_video {
            report.video_hidden += 1;
            continue;
        }
        kept.push(track);
    }
    (kept, report)
}

pub fn apply_from_settings(
    tracks: Vec<Track>,
    flags: impl Fn(&Track) -> ContentFlags,
    settings: &madrigal_settings::Settings,
) -> (Vec<Track>, ContentReport) {
    apply_content_filters(
        tracks,
        flags,
        settings.content.hide_explicit,
        settings.content.hide_video,
    )
}
