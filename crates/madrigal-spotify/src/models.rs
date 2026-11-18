use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotifyImage {
    pub url: String,
    pub height: Option<u32>,
    pub width: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotifyArtistRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotifyAlbumRef {
    pub id: String,
    pub name: String,
    pub images: Vec<SpotifyImage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotifyPlaylist {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub images: Vec<SpotifyImage>,
    pub owner_id: String,
    pub owner_name: Option<String>,
    pub track_total: Option<u32>,
    pub uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotifyPlaylistItem {
    pub uid: Option<String>,
    pub track: Option<SpotifyTrack>,
    pub is_local: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotifyTrack {
    pub id: String,
    pub name: String,
    pub artists: Vec<SpotifyArtistRef>,
    pub album: Option<SpotifyAlbumRef>,
    pub duration_ms: i64,
    pub is_local: bool,
    pub explicit: bool,
    pub uri: Option<String>,
}

impl SpotifyTrack {
    pub fn artist_text(&self) -> String {
        self.artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn is_valid(&self) -> bool {
        !self.id.is_empty() && !self.name.is_empty() && !self.is_local
    }
}
