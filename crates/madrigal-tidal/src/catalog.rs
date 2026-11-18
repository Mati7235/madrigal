use madrigal_core::{Track, TrackIdentity};

use crate::auth::{Client, TokenSet};
use crate::Error;

pub const SEARCH_TYPES: &str = "TRACKS";

pub fn track_id_param(track_id: &str) -> Result<&str, Error> {
    let raw = track_id.strip_prefix("tidal:").unwrap_or(track_id);
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::InvalidId(track_id.to_owned()));
    }
    Ok(raw)
}

fn cover_url(cover: Option<&str>, size: &str) -> Option<String> {
    let value = cover?;
    if value.starts_with("http://") || value.starts_with("https://") {
        return Some(value.to_owned());
    }
    Some(format!(
        "https://resources.tidal.com/images/{}/{size}.jpg",
        value.replace('-', "/")
    ))
}

fn artists_of(item: &serde_json::Value) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(list) = item.get("artists").and_then(|value| value.as_array()) {
        for artist in list {
            if let Some(name) = artist.get("name").and_then(|value| value.as_str()) {
                names.push(name.to_owned());
            }
        }
    }
    if names.is_empty() {
        if let Some(name) = item
            .get("artist")
            .and_then(|value| value.get("name"))
            .and_then(|value| value.as_str())
        {
            names.push(name.to_owned());
        }
    }
    names
}

pub fn parse_track(item: &serde_json::Value) -> Option<Track> {
    let id = item.get("id")?.as_u64()?;
    let title = item
        .get("title")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned();
    let duration = item
        .get("duration")
        .and_then(|value| value.as_u64())
        .map(|secs| secs as u32)
        .filter(|secs| *secs > 0);
    let album = item.get("album");
    Some(Track {
        identity: TrackIdentity::tidal(id.to_string()),
        title,
        artists: artists_of(item),
        album: album
            .and_then(|value| value.get("title"))
            .and_then(|value| value.as_str())
            .map(str::to_owned),
        duration_secs: duration,
        thumbnail: cover_url(
            album.and_then(|value| value.get("cover")).and_then(|value| value.as_str()),
            "320x320",
        ),
        track_number: None,
        local_path: None,
        set_video_id: None,
    })
}

pub fn search_tracks(
    client: &Client,
    token: &TokenSet,
    query: &str,
    limit: u32,
) -> Result<Vec<Track>, Error> {
    let count = limit.clamp(1, 50).to_string();
    let response = client.get(
        "/v1/search",
        token,
        &[
            ("query", query.to_owned()),
            ("limit", count),
            ("offset", "0".to_owned()),
            ("types", SEARCH_TYPES.to_owned()),
        ],
    )?;
    if !response.status().is_success() {
        return Err(Error::Status(response.status().as_u16()));
    }
    let body = crate::body_json(response)?;
    let items = body
        .get("tracks")
        .and_then(|value| value.get("items"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(items.iter().filter_map(parse_track).collect())
}
