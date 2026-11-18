use madrigal_core::Track;
use regex::Regex;
use std::sync::OnceLock;

use crate::{CanvasEntry, CanvasSource, Error};

fn cover_id() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}").expect("regex")
    })
}

fn cover_url(id: Option<&str>, kind: &str, file: &str) -> Option<String> {
    let id = id?;
    if !cover_id().is_match(id) {
        return None;
    }
    Some(format!(
        "https://resources.tidal.com/{kind}/{}/{file}",
        id.replace('-', "/")
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
    names
}

fn entry_of(item: &serde_json::Value) -> Option<CanvasEntry> {
    let album = item.get("album")?;
    let video = cover_url(
        album.get("videoCover").and_then(|value| value.as_str()),
        "videos",
        "1280x1280.mp4",
    )?;
    Some(CanvasEntry {
        source: CanvasSource::Tidal,
        name: item.get("title").and_then(|value| value.as_str()).map(str::to_owned),
        artist: Some(artists_of(item).join(", ")),
        album_id: None,
        album_name: album
            .get("title")
            .and_then(|value| value.as_str())
            .map(str::to_owned),
        static_url: cover_url(
            album.get("cover").and_then(|value| value.as_str()),
            "images",
            "640x640.jpg",
        ),
        animated: None,
        animated_vertical: None,
        video_url: Some(video.clone()),
        video_url_vertical: Some(video),
    })
}

pub fn parse_search(body: &serde_json::Value, track: &Track) -> Option<CanvasEntry> {
    let items = body
        .get("tracks")
        .and_then(|value| value.get("items"))
        .and_then(|value| value.as_array())?;
    for item in items {
        let entry = match entry_of(item) {
            Some(entry) => entry,
            None => continue,
        };
        if entry.matches_identity(&track.title, &track.artist_text()) {
            return Some(entry);
        }
    }
    None
}

pub fn fetch(
    client: &madrigal_tidal::auth::Client,
    token: &madrigal_tidal::auth::TokenSet,
    track: &Track,
) -> Result<Option<CanvasEntry>, Error> {
    let query = format!("{} {}", track.artist_text(), track.title);
    let response = client
        .get(
            "/v1/search",
            token,
            &[
                ("query", query),
                ("types", "TRACKS".to_owned()),
                ("limit", "10".to_owned()),
                ("offset", "0".to_owned()),
            ],
        )
        .map_err(Error::from)?;
    if !response.status().is_success() {
        return Err(Error::Status(response.status().as_u16()));
    }
    let body: serde_json::Value = response
        .json()
        .map_err(|err| Error::Protocol(err.to_string()))?;
    Ok(parse_search(&body, track))
}
