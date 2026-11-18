use madrigal_core::Track;

use crate::{CanvasEntry, CanvasSource, Error};

pub const BASE_URL: &str = "https://artwork.boidu.dev/";

pub fn search_url(base: &str, track: &Track, storefront: &str) -> String {
    format!(
        "{base}?s={}&a={}&storefront={}",
        url_encode(&track.title),
        url_encode(&track.artist_text()),
        url_encode(storefront),
    )
}

fn url_encode(raw: &str) -> String {
    let mut out = String::new();
    for byte in raw.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn entry_of(body: &serde_json::Value) -> Option<CanvasEntry> {
    let entry = CanvasEntry {
        source: CanvasSource::BetterLyrics,
        name: body.get("name").and_then(|value| value.as_str()).map(str::to_owned),
        artist: body.get("artist").and_then(|value| value.as_str()).map(str::to_owned),
        album_id: body.get("albumId").and_then(|value| value.as_str()).map(str::to_owned),
        album_name: body.get("albumName").and_then(|value| value.as_str()).map(str::to_owned),
        static_url: body.get("static").and_then(|value| value.as_str()).map(str::to_owned),
        animated: body.get("animated").and_then(|value| value.as_str()).map(str::to_owned),
        animated_vertical: body
            .get("animatedVertical")
            .and_then(|value| value.as_str())
            .map(str::to_owned),
        video_url: body.get("videoUrl").and_then(|value| value.as_str()).map(str::to_owned),
        video_url_vertical: body
            .get("videoUrlVertical")
            .and_then(|value| value.as_str())
            .map(str::to_owned),
    };
    entry.preferred_animation_url()?;
    Some(entry)
}

pub fn parse_search(body: &serde_json::Value, track: &Track) -> Option<CanvasEntry> {
    let entry = entry_of(body)?;
    if entry.matches_identity(&track.title, &track.artist_text()) {
        Some(entry)
    } else {
        None
    }
}

pub fn fetch(
    http: &reqwest::blocking::Client,
    base: &str,
    track: &Track,
) -> Result<Option<CanvasEntry>, Error> {
    fetch_storefront(http, base, track, "us")
}

pub fn fetch_storefront(
    http: &reqwest::blocking::Client,
    base: &str,
    track: &Track,
    storefront: &str,
) -> Result<Option<CanvasEntry>, Error> {
    let response = http
        .get(search_url(base, track, storefront))
        .send()
        .map_err(|err| Error::Transport(err.to_string()))?;
    if !response.status().is_success() {
        return Err(Error::Status(response.status().as_u16()));
    }
    let body: serde_json::Value = response
        .json()
        .map_err(|err| Error::Protocol(err.to_string()))?;
    Ok(parse_search(&body, track))
}
