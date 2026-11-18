use madrigal_core::Track;
use serde_json::{Value, json};

use crate::protocol::{CastMessage, NS_MEDIA, RECEIVER_ID, SENDER_ID};
use crate::{Error, Result};

pub const MUSIC_TRACK_METADATA_TYPE: u32 = 3;
pub const STREAM_TYPE_BUFFERED: &str = "BUFFERED";

#[derive(Debug, Clone, PartialEq)]
pub struct CastMedia {
    pub url: String,
    pub mime: Option<String>,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub artwork: Option<String>,
    pub duration_secs: Option<u32>,
}

impl CastMedia {
    pub fn from_track(track: &Track, url: String) -> Self {
        let artist = non_empty(track.artist_text());
        Self {
            url,
            mime: None,
            title: track.title.clone(),
            artist,
            album: track.album.as_ref().and_then(|album| non_empty(album.clone())),
            artwork: track.thumbnail.clone(),
            duration_secs: track.duration_secs,
        }
    }
}

fn non_empty(text: String) -> Option<String> {
    (!text.trim().is_empty()).then_some(text)
}

pub fn normalize_mime(raw: Option<&str>) -> Option<String> {
    let normalized = raw?.trim();
    if normalized.is_empty() {
        return None;
    }
    let base = normalized.split(';').next().unwrap_or("").trim();
    if !base.contains('/') || base.ends_with("/*") {
        return None;
    }
    Some(normalized.to_owned())
}

pub fn load_payload(
    media: &CastMedia,
    session_id: &str,
    request_id: u32,
) -> Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert("metadataType".into(), Value::from(MUSIC_TRACK_METADATA_TYPE));
    metadata.insert("title".into(), Value::from(media.title.clone()));
    if let Some(artist) = &media.artist {
        metadata.insert("artist".into(), Value::from(artist.clone()));
    }
    if let Some(album) = &media.album {
        metadata.insert("albumName".into(), Value::from(album.clone()));
    }
    if let Some(artwork) = &media.artwork {
        metadata.insert("images".into(), json!([{"url": artwork}]));
    }
    let mut info = serde_json::Map::new();
    info.insert("contentId".into(), Value::from(media.url.clone()));
    info.insert("streamType".into(), Value::from(STREAM_TYPE_BUFFERED));
    if let Some(mime) = normalize_mime(media.mime.as_deref()) {
        info.insert("contentType".into(), Value::from(mime));
    }
    info.insert("metadata".into(), Value::Object(metadata));
    if let Some(duration) = media.duration_secs {
        info.insert("duration".into(), Value::from(f64::from(duration)));
    }
    json!({
        "type": "LOAD",
        "requestId": request_id,
        "sessionId": session_id,
        "media": info,
        "autoplay": true,
        "currentTime": 0.0,
    })
}

pub fn load(dest: &str, media: &CastMedia, session_id: &str, request_id: u32) -> Result<CastMessage> {
    CastMessage::new(SENDER_ID, dest, NS_MEDIA, load_payload(media, session_id, request_id))
}

pub fn receiver_dest() -> &'static str {
    RECEIVER_ID
}

pub fn parse_error(payload: &str) -> Result<Option<String>> {
    let value: Value = serde_json::from_str(payload)?;
    if value.get("type") != Some(&Value::from("LOAD_FAILED")) {
        return Ok(None);
    }
    Ok(Some(
        value
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("load failed")
            .to_owned(),
    ))
}

pub fn check_load_failed(payload: &str) -> Result<()> {
    if let Some(reason) = parse_error(payload)? {
        return Err(Error::UnexpectedType(reason));
    }
    Ok(())
}
