use madrigal_core::{Album, Artist, Playlist, Track};

#[derive(Debug, Clone, PartialEq)]
pub struct ArtistPage {
    pub artist: Artist,
    pub songs: Vec<Track>,
    pub albums: Vec<Album>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlbumPage {
    pub album: Album,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistPage {
    pub playlist: Playlist,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoodEntry {
    pub title: String,
    pub params: String,
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoodSection {
    pub name: String,
    pub entries: Vec<MoodEntry>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MoodListing {
    pub sections: Vec<MoodSection>,
}
