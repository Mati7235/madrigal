use crate::auth::{Client, TokenSet};
use crate::catalog::track_id_param;
use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricsPayload {
    pub subtitles: Option<String>,
    pub text: Option<String>,
    pub provider: Option<String>,
}

impl LyricsPayload {
    pub fn to_lyrics(&self) -> Option<madrigal_lyrics::Lyrics> {
        if let Some(subtitles) = self.subtitles.as_deref() {
            if !subtitles.is_empty() {
                let parsed = madrigal_lyrics::lrc::parse(subtitles);
                if !parsed.lines.is_empty() {
                    return Some(parsed);
                }
            }
        }
        if let Some(text) = self.text.as_deref() {
            if !text.trim().is_empty() {
                let parsed = madrigal_lyrics::lrc::parse(text);
                if !parsed.lines.is_empty() {
                    return Some(parsed);
                }
            }
        }
        None
    }
}

pub fn fetch_lyrics(
    client: &Client,
    token: &TokenSet,
    track_id: &str,
) -> Result<Option<LyricsPayload>, Error> {
    let raw = track_id_param(track_id)?.to_owned();
    let response = client.get(&format!("/v1/tracks/{raw}/lyrics"), token, &[])?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(Error::Status(response.status().as_u16()));
    }
    let body = crate::body_json(response)?;
    let text = |key: &str| {
        body.get(key)
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let payload = LyricsPayload {
        subtitles: text("subtitles"),
        text: text("lyrics"),
        provider: text("lyricsProvider"),
    };
    if payload.subtitles.is_none() && payload.text.is_none() {
        return Ok(None);
    }
    Ok(Some(payload))
}
