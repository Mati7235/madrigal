use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }
    };
}

id_type!(TrackId);
id_type!(AlbumId);
id_type!(ArtistId);
id_type!(PlaylistId);
id_type!(PodcastId);
id_type!(EpisodeId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TrackSource {
    #[default]
    Youtube,
    Tidal,
    Local,
}

impl TrackSource {
    pub fn as_str(self) -> &'static str {
        match self {
            TrackSource::Youtube => "youtube",
            TrackSource::Tidal => "tidal",
            TrackSource::Local => "local",
        }
    }

    pub fn is_remote(self) -> bool {
        self != TrackSource::Local
    }
}

impl fmt::Display for TrackSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PlaylistKind {
    #[default]
    Local,
    Remote,
    Auto,
    Spotify,
}

impl PlaylistKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PlaylistKind::Local => "local",
            PlaylistKind::Remote => "remote",
            PlaylistKind::Auto => "auto",
            PlaylistKind::Spotify => "spotify",
        }
    }
}

impl fmt::Display for PlaylistKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackIdentity {
    pub source: TrackSource,
    pub id: String,
}

impl TrackIdentity {
    pub fn new(source: TrackSource, id: impl Into<String>) -> Self {
        Self {
            source,
            id: id.into(),
        }
    }

    pub fn youtube(id: impl Into<String>) -> Self {
        Self::new(TrackSource::Youtube, id)
    }

    pub fn tidal(id: impl Into<String>) -> Self {
        Self::new(TrackSource::Tidal, id)
    }

    pub fn local(id: impl Into<String>) -> Self {
        Self::new(TrackSource::Local, id)
    }

    pub fn is_empty(&self) -> bool {
        self.id.is_empty()
    }

    pub fn is_remote(&self) -> bool {
        !self.is_empty() && self.source.is_remote()
    }

    pub fn parse_prefixed(value: &str) -> Self {
        if let Some(rest) = value.strip_prefix("tidal:") {
            Self::tidal(rest)
        } else if let Some(rest) = value.strip_prefix("local:") {
            Self::local(rest)
        } else {
            Self::youtube(value)
        }
    }

    pub fn to_storage_string(&self) -> String {
        match self.source {
            TrackSource::Youtube => self.id.clone(),
            TrackSource::Tidal => format!("tidal:{}", self.id),
            TrackSource::Local => format!("local:{}", self.id),
        }
    }
}

impl fmt::Display for TrackIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_storage_string())
    }
}

pub fn format_duration(seconds: Option<u32>) -> String {
    match seconds {
        None | Some(0) => String::new(),
        Some(total) => {
            let minutes = total / 60;
            let secs = total % 60;
            let hours = minutes / 60;
            if hours > 0 {
                format!("{}:{:02}:{:02}", hours, minutes % 60, secs)
            } else {
                format!("{minutes}:{secs:02}")
            }
        }
    }
}

pub fn parse_duration(value: &str) -> Option<u32> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let parts: Vec<&str> = value.split(':').collect();
    if parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    let mut seconds: u32 = 0;
    for part in parts {
        let n: u32 = part.parse().ok()?;
        seconds = seconds.checked_mul(60)?.checked_add(n)?;
    }
    if seconds == 0 { None } else { Some(seconds) }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub identity: TrackIdentity,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub duration_secs: Option<u32>,
    pub thumbnail: Option<String>,
    pub track_number: Option<u32>,
    pub local_path: Option<String>,
    pub set_video_id: Option<String>,
}

impl Track {
    pub fn source(&self) -> TrackSource {
        self.identity.source
    }

    pub fn artist_text(&self) -> String {
        self.artists.join(", ")
    }

    pub fn is_remote(&self) -> bool {
        self.identity.is_remote()
    }

    pub fn is_valid(&self) -> bool {
        !self.identity.is_empty() && !self.title.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Album {
    pub id: AlbumId,
    pub title: String,
    pub artists: Vec<String>,
    pub artist_id: Option<ArtistId>,
    pub year: Option<String>,
    pub thumbnail: Option<String>,
    pub track_count: Option<u32>,
    pub audio_playlist_id: Option<String>,
}

impl Album {
    pub fn artist_text(&self) -> String {
        self.artists.join(", ")
    }

    pub fn is_valid(&self) -> bool {
        !self.id.is_empty() && !self.title.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artist {
    pub id: ArtistId,
    pub name: String,
    pub subscribers: Option<String>,
    pub thumbnail: Option<String>,
    pub description: Option<String>,
}

impl Artist {
    pub fn is_valid(&self) -> bool {
        !self.id.is_empty() && !self.name.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: PlaylistId,
    #[serde(default)]
    pub kind: PlaylistKind,
    pub title: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub thumbnail: Option<String>,
    pub track_count: Option<u32>,
    #[serde(default)]
    pub owned: bool,
    pub privacy: Option<String>,
    pub author_id: Option<String>,
}

impl Playlist {
    pub fn is_valid(&self) -> bool {
        !self.id.is_empty() && !self.title.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Podcast {
    pub id: PodcastId,
    pub title: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub thumbnail: Option<String>,
}

impl Podcast {
    pub fn is_valid(&self) -> bool {
        !self.id.is_empty() && !self.title.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    pub id: EpisodeId,
    pub podcast_id: PodcastId,
    pub title: String,
    pub description: Option<String>,
    pub duration_secs: Option<u32>,
    pub thumbnail: Option<String>,
    pub published_at: Option<i64>,
    pub position_secs: Option<u32>,
}

impl Episode {
    pub fn is_valid(&self) -> bool {
        if self.id.is_empty() || self.podcast_id.is_empty() || self.title.is_empty() {
            return false;
        }
        match (self.position_secs, self.duration_secs) {
            (Some(pos), Some(len)) => pos <= len,
            _ => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub track: TrackIdentity,
    pub played_at: i64,
    pub duration_secs: Option<u32>,
    pub completed: bool,
}

impl Event {
    pub fn is_valid(&self) -> bool {
        !self.track.is_empty() && self.played_at > 0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHistoryItem {
    pub query: String,
    pub searched_at: i64,
}

impl SearchHistoryItem {
    pub fn is_valid(&self) -> bool {
        !self.query.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recognition {
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub score: f32,
    pub track: Option<TrackIdentity>,
}

impl Recognition {
    pub fn score_in_range(&self) -> bool {
        (0.0..=1.0).contains(&self.score)
    }

    pub fn is_reliable(&self) -> bool {
        self.score_in_range() && self.score >= 0.5 && !self.title.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip<T>(value: &T) -> T
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        let json = serde_json::to_string(value).expect("serializa");
        serde_json::from_str(&json).expect("deserializa")
    }

    fn sample_track() -> Track {
        Track {
            identity: TrackIdentity::youtube("dQw4w9WgXcQ"),
            title: "Never Gonna Give You Up".to_owned(),
            artists: vec!["Rick Astley".to_owned()],
            album: Some("Whenever You Need Somebody".to_owned()),
            duration_secs: Some(212),
            thumbnail: Some("https://img/rick.jpg".to_owned()),
            track_number: Some(1),
            local_path: None,
            set_video_id: None,
        }
    }

    #[test]
    fn identidades_tipadas_no_mezclan_dominios() {
        let track = TrackId::new("abc");
        let album = AlbumId::new("abc");
        assert_eq!(track.as_str(), album.as_str());
        assert!(!track.is_empty());
        assert!(TrackId::default().is_empty());
        assert_eq!(track.to_string(), "abc");
        assert_eq!(TrackId::from("abc"), track);
    }

    #[test]
    fn identidad_unificada_roundtrip_y_prefijos() {
        for identity in [
            TrackIdentity::youtube("dQw4w9WgXcQ"),
            TrackIdentity::tidal("12345"),
            TrackIdentity::local("/musica/tema.flac"),
        ] {
            assert_eq!(roundtrip(&identity), identity);
            assert!(!identity.is_empty());
            assert_eq!(
                TrackIdentity::parse_prefixed(&identity.to_storage_string()),
                identity
            );
        }
        assert!(TrackIdentity::new(TrackSource::Youtube, "").is_empty());
        assert_eq!(
            TrackIdentity::parse_prefixed("tidal:99"),
            TrackIdentity::tidal("99")
        );
        assert_eq!(
            TrackIdentity::parse_prefixed("local:/a.flac"),
            TrackIdentity::local("/a.flac")
        );
        assert_eq!(
            TrackIdentity::parse_prefixed("dQw4w9WgXcQ"),
            TrackIdentity::youtube("dQw4w9WgXcQ")
        );
    }

    #[test]
    fn fuentes_derivan_origen_y_remote() {
        let mut track = sample_track();
        assert_eq!(track.source(), TrackSource::Youtube);
        assert!(track.is_remote());
        assert_eq!(track.artist_text(), "Rick Astley");
        track.identity = TrackIdentity::local("/musica/tema.flac");
        track.local_path = Some("/musica/tema.flac".to_owned());
        assert!(!track.is_remote());
        assert_eq!(roundtrip(&track), track);
        assert!(track.is_valid());
        assert!(!Track {
            identity: TrackIdentity::youtube("x"),
            ..sample_track()
        }
        .title
        .is_empty());
    }

    #[test]
    fn duracion_formatea_y_parsea() {
        assert_eq!(format_duration(None), "");
        assert_eq!(format_duration(Some(0)), "");
        assert_eq!(format_duration(Some(212)), "3:32");
        assert_eq!(format_duration(Some(3723)), "1:02:03");
        assert_eq!(parse_duration("3:32"), Some(212));
        assert_eq!(parse_duration("1:02:03"), Some(3723));
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("abc"), None);
        assert_eq!(parse_duration("0:00"), None);
    }

    #[test]
    fn album_artist_playlist_roundtrip() {
        let album = Album {
            id: AlbumId::new("MPRE_test"),
            title: "Whenever You Need Somebody".to_owned(),
            artists: vec!["Rick Astley".to_owned()],
            artist_id: Some(ArtistId::new("UCtest")),
            year: Some("1987".to_owned()),
            thumbnail: None,
            track_count: Some(10),
            audio_playlist_id: Some("OLAKtest".to_owned()),
        };
        assert_eq!(roundtrip(&album), album);
        assert!(album.is_valid());
        assert_eq!(album.artist_text(), "Rick Astley");

        let artist = Artist {
            id: ArtistId::new("UCtest"),
            name: "Rick Astley".to_owned(),
            subscribers: Some("1M".to_owned()),
            thumbnail: None,
            description: None,
        };
        assert_eq!(roundtrip(&artist), artist);
        assert!(artist.is_valid());

        for kind in [
            PlaylistKind::Local,
            PlaylistKind::Remote,
            PlaylistKind::Auto,
            PlaylistKind::Spotify,
        ] {
            assert_eq!(roundtrip(&kind), kind);
        }
        let playlist = Playlist {
            id: PlaylistId::new("PLtest"),
            kind: PlaylistKind::Remote,
            title: "Mix".to_owned(),
            author: Some("yo".to_owned()),
            description: None,
            thumbnail: None,
            track_count: Some(3),
            owned: true,
            privacy: Some("private".to_owned()),
            author_id: None,
        };
        assert_eq!(roundtrip(&playlist), playlist);
        assert!(playlist.is_valid());
        assert!(!Playlist {
            id: PlaylistId::new("x"),
            title: String::new(),
            ..playlist.clone()
        }
        .is_valid());
    }

    #[test]
    fn podcast_episode_event_historial_recognition() {
        let podcast = Podcast {
            id: PodcastId::new("pc1"),
            title: "Charlas".to_owned(),
            author: Some("Ana".to_owned()),
            description: None,
            thumbnail: None,
        };
        assert_eq!(roundtrip(&podcast), podcast);
        assert!(podcast.is_valid());

        let episode = Episode {
            id: EpisodeId::new("ep7"),
            podcast_id: PodcastId::new("pc1"),
            title: "Episodio 7".to_owned(),
            description: None,
            duration_secs: Some(1800),
            thumbnail: None,
            published_at: Some(1_700_000_000),
            position_secs: Some(300),
        };
        assert_eq!(roundtrip(&episode), episode);
        assert!(episode.is_valid());
        assert!(!Episode {
            position_secs: Some(9999),
            ..episode.clone()
        }
        .is_valid());

        let event = Event {
            track: TrackIdentity::youtube("dQw4w9WgXcQ"),
            played_at: 1_700_000_000,
            duration_secs: Some(200),
            completed: true,
        };
        assert_eq!(roundtrip(&event), event);
        assert!(event.is_valid());
        assert!(!Event {
            played_at: 0,
            ..event.clone()
        }
        .is_valid());

        let item = SearchHistoryItem {
            query: "rick astley".to_owned(),
            searched_at: 1_700_000_000,
        };
        assert_eq!(roundtrip(&item), item);
        assert!(item.is_valid());
        assert!(!SearchHistoryItem {
            query: "   ".to_owned(),
            searched_at: 0,
        }
        .is_valid());

        let recognition = Recognition {
            title: "Never Gonna Give You Up".to_owned(),
            artists: vec!["Rick Astley".to_owned()],
            album: Some("Whenever You Need Somebody".to_owned()),
            score: 0.93,
            track: Some(TrackIdentity::youtube("dQw4w9WgXcQ")),
        };
        assert_eq!(roundtrip(&recognition), recognition);
        assert!(recognition.is_reliable());
        assert!(!Recognition {
            score: 0.2,
            ..recognition.clone()
        }
        .is_reliable());
        assert!(!Recognition {
            score: 1.5,
            ..recognition.clone()
        }
        .score_in_range());
    }
}
