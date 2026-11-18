use std::collections::HashMap;

use madrigal_core::Track;
use madrigal_settings::{Player as PlayerSettings, RepeatMode};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

pub const BUS_NAME: &str = "org.mpris.MediaPlayer2.madrigal";
pub const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
pub const TRACK_PATH: &str = "/org/mpris/MediaPlayer2/Track";
pub const IDENTITY: &str = "Madrigal";
pub const DESKTOP_ENTRY: &str = "madrigal";
pub const MIN_RATE: f64 = 0.5;
pub const MAX_RATE: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    #[default]
    Stopped,
}

impl PlaybackStatus {
    pub fn as_mpris(self) -> &'static str {
        match self {
            PlaybackStatus::Playing => "Playing",
            PlaybackStatus::Paused => "Paused",
            PlaybackStatus::Stopped => "Stopped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoopStatus {
    #[default]
    None,
    Track,
    Playlist,
}

impl LoopStatus {
    pub fn as_mpris(self) -> &'static str {
        match self {
            LoopStatus::None => "None",
            LoopStatus::Track => "Track",
            LoopStatus::Playlist => "Playlist",
        }
    }

    pub fn from_mpris(value: &str) -> Self {
        match value {
            "Track" => LoopStatus::Track,
            "Playlist" => LoopStatus::Playlist,
            _ => LoopStatus::None,
        }
    }

    pub fn from_repeat(mode: RepeatMode) -> Self {
        match mode {
            RepeatMode::Off => LoopStatus::None,
            RepeatMode::One => LoopStatus::Track,
            RepeatMode::All => LoopStatus::Playlist,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NowPlaying {
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub art_url: Option<String>,
    pub url: Option<String>,
    pub length_secs: f64,
}

impl NowPlaying {
    pub fn from_track(track: &Track, url: Option<String>) -> Self {
        Self {
            id: track.identity.to_storage_string(),
            title: track.title.clone(),
            artists: track.artists.clone(),
            album: track.album.clone(),
            art_url: track.thumbnail.clone(),
            url,
            length_secs: track.duration_secs.unwrap_or(0) as f64,
        }
    }

    pub fn track_key(&self) -> String {
        format!("{}|{}|{}", self.id, self.title, self.art_url.as_deref().unwrap_or(""))
    }
}

pub type ActionResult = Result<(), String>;

pub trait NowPlayingProvider: Send + Sync + 'static {
    fn now_playing(&self) -> Option<NowPlaying>;
    fn playback_status(&self) -> PlaybackStatus;
    fn position_secs(&self) -> f64;
    fn loop_status(&self) -> LoopStatus;
    fn shuffle(&self) -> bool;
    fn volume(&self) -> f64;
    fn rate(&self) -> f64;
    fn can_go_next(&self) -> bool {
        true
    }
    fn can_go_previous(&self) -> bool {
        true
    }
    fn can_play(&self) -> bool {
        true
    }
    fn can_pause(&self) -> bool {
        true
    }
    fn can_seek(&self) -> bool {
        true
    }
    fn can_control(&self) -> bool {
        true
    }
    fn play(&self) -> ActionResult {
        Ok(())
    }
    fn pause(&self) -> ActionResult {
        Ok(())
    }
    fn play_pause(&self) -> ActionResult {
        Ok(())
    }
    fn stop(&self) -> ActionResult {
        Ok(())
    }
    fn next(&self) -> ActionResult {
        Ok(())
    }
    fn previous(&self) -> ActionResult {
        Ok(())
    }
    fn seek_relative(&self, _offset_secs: f64) -> ActionResult {
        Ok(())
    }
    fn seek_absolute(&self, _position_secs: f64) -> ActionResult {
        Ok(())
    }
    fn set_loop(&self, _status: LoopStatus) -> ActionResult {
        Ok(())
    }
    fn set_shuffle(&self, _shuffle: bool) -> ActionResult {
        Ok(())
    }
    fn set_volume(&self, _volume: f64) -> ActionResult {
        Ok(())
    }
    fn set_rate(&self, _rate: f64) -> ActionResult {
        Ok(())
    }
    fn open_uri(&self, _uri: &str) -> ActionResult {
        Ok(())
    }
}

pub fn track_object_path(identifier: &str) -> OwnedObjectPath {
    let base = if identifier.is_empty() { "none" } else { identifier };
    let safe: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(120)
        .collect();
    let safe = if safe.is_empty() { "none".to_owned() } else { safe };
    OwnedObjectPath::try_from(format!("{TRACK_PATH}/{safe}"))
        .unwrap_or_else(|_| fallback_track_path())
}

fn fallback_track_path() -> OwnedObjectPath {
    OwnedObjectPath::try_from(format!("{TRACK_PATH}/none")).expect("constant path is valid")
}

fn owned(value: Value<'_>) -> OwnedValue {
    OwnedValue::try_from(value).expect("metadata values are owned")
}

pub fn metadata_map(track: &NowPlaying) -> HashMap<String, OwnedValue> {
    let mut metadata = HashMap::new();
    metadata.insert(
        "mpris:trackid".to_owned(),
        owned(Value::from(track_object_path(&track.id))),
    );
    metadata.insert(
        "xesam:title".to_owned(),
        owned(Value::from(track.title.clone())),
    );
    metadata.insert(
        "xesam:artist".to_owned(),
        owned(Value::from(track.artists.clone())),
    );
    metadata.insert(
        "mpris:length".to_owned(),
        owned(Value::from((track.length_secs * 1_000_000.0) as i64)),
    );
    if let Some(album) = track.album.as_deref().filter(|s| !s.is_empty()) {
        metadata.insert("xesam:album".to_owned(), owned(Value::from(album.to_owned())));
    }
    if let Some(art_url) = track.art_url.as_deref().filter(|s| !s.is_empty()) {
        metadata.insert(
            "mpris:artUrl".to_owned(),
            owned(Value::from(art_url.to_owned())),
        );
    }
    if let Some(url) = track.url.as_deref().filter(|s| !s.is_empty()) {
        metadata.insert("xesam:url".to_owned(), owned(Value::from(url.to_owned())));
    }
    metadata
}

pub fn volume_from_settings(player: &PlayerSettings) -> f64 {
    (player.volume as f64 / 100.0).clamp(0.0, 1.0)
}

pub fn rate_from_settings(player: &PlayerSettings) -> f64 {
    player.playback_speed as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track {
            identity: madrigal_core::TrackIdentity::youtube("dQw4w9WgXcQ"),
            title: "Never Gonna Give You Up".to_owned(),
            artists: vec!["Rick Astley".to_owned()],
            album: Some("Whenever You Need Somebody".to_owned()),
            duration_secs: Some(212),
            thumbnail: Some("https://img/rick.jpg".to_owned()),
            track_number: None,
            local_path: None,
            set_video_id: None,
        }
    }

    #[test]
    fn mapea_track_a_now_playing() {
        let now = NowPlaying::from_track(&track(), Some("https://stream/x".to_owned()));
        assert_eq!(now.id, "dQw4w9WgXcQ");
        assert_eq!(now.length_secs, 212.0);
        assert_eq!(now.art_url.as_deref(), Some("https://img/rick.jpg"));
    }

    #[test]
    fn metadata_respeta_mapeo_oracle() {
        let now = NowPlaying::from_track(&track(), Some("https://stream/x".to_owned()));
        let meta = metadata_map(&now);
        assert_eq!(
            meta["mpris:trackid"],
            owned(Value::from(
                OwnedObjectPath::try_from("/org/mpris/MediaPlayer2/Track/dQw4w9WgXcQ").unwrap()
            ))
        );
        assert_eq!(meta["xesam:title"], owned(Value::from("Never Gonna Give You Up")));
        assert_eq!(
            meta["xesam:artist"],
            owned(Value::from(vec!["Rick Astley".to_owned()]))
        );
        assert_eq!(meta["mpris:length"], owned(Value::from(212_000_000i64)));
        assert_eq!(
            meta["xesam:album"],
            owned(Value::from("Whenever You Need Somebody"))
        );
        assert_eq!(meta["mpris:artUrl"], owned(Value::from("https://img/rick.jpg")));
        assert_eq!(meta["xesam:url"], owned(Value::from("https://stream/x")));
    }

    #[test]
    fn track_path_sanitiza_y_trunca() {
        assert_eq!(
            track_object_path("a/b:c"),
            OwnedObjectPath::try_from("/org/mpris/MediaPlayer2/Track/a_b_c").unwrap()
        );
        assert_eq!(
            track_object_path(""),
            OwnedObjectPath::try_from("/org/mpris/MediaPlayer2/Track/none").unwrap()
        );
        let long = "x".repeat(200);
        let path = track_object_path(&long);
        assert_eq!(path.as_str().len(), TRACK_PATH.len() + 1 + 120);
    }

    #[test]
    fn loop_status_ida_y_vuelta() {
        assert_eq!(LoopStatus::from_mpris("Track"), LoopStatus::Track);
        assert_eq!(LoopStatus::from_mpris("Playlist"), LoopStatus::Playlist);
        assert_eq!(LoopStatus::from_mpris("bogus"), LoopStatus::None);
        assert_eq!(LoopStatus::Track.as_mpris(), "Track");
        assert_eq!(LoopStatus::from_repeat(RepeatMode::One), LoopStatus::Track);
        assert_eq!(LoopStatus::from_repeat(RepeatMode::All), LoopStatus::Playlist);
        assert_eq!(LoopStatus::from_repeat(RepeatMode::Off), LoopStatus::None);
    }
}
