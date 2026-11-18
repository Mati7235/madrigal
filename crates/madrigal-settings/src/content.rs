use serde::{Deserialize, Serialize};

use crate::de::{or_default, vec_filtered};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SearchSource {
    Local,
    #[default]
    Online,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SongSortType {
    #[default]
    CreateDate,
    Name,
    Artist,
    PlayTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlaylistSongSortType {
    #[default]
    Custom,
    CreateDate,
    Name,
    Artist,
    PlayTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AutoPlaylistSongSortType {
    #[default]
    CreateDate,
    Name,
    Artist,
    PlayTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ArtistSortType {
    #[default]
    CreateDate,
    Name,
    SongCount,
    PlayTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ArtistSongSortType {
    #[default]
    CreateDate,
    Name,
    PlayTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlbumSortType {
    #[default]
    CreateDate,
    Name,
    Artist,
    Year,
    SongCount,
    Length,
    PlayTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlaylistSortType {
    #[default]
    CreateDate,
    Name,
    SongCount,
    LastUpdated,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SongFilter {
    #[default]
    Library,
    Liked,
    Downloaded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ArtistFilter {
    #[default]
    Library,
    Liked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlbumFilter {
    #[default]
    Library,
    Liked,
    Downloaded,
    DownloadedFull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QuickPicks {
    #[default]
    QuickPicks,
    LastListen,
    DontShow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TopRange {
    #[default]
    AllTime,
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlaylistSuggestionSource {
    PlaylistTitle,
    PlaylistContent,
    #[default]
    Both,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Content {
    #[serde(deserialize_with = "or_default")]
    pub content_language: String,
    #[serde(deserialize_with = "or_default")]
    pub content_country: String,
    #[serde(deserialize_with = "or_default")]
    pub hide_explicit: bool,
    #[serde(deserialize_with = "or_default")]
    pub hide_video: bool,
    #[serde(deserialize_with = "or_default")]
    pub search_source: SearchSource,
    #[serde(deserialize_with = "or_default")]
    pub song_sort_type: SongSortType,
    #[serde(deserialize_with = "or_default")]
    pub song_sort_descending: bool,
    #[serde(deserialize_with = "or_default")]
    pub playlist_song_sort_type: PlaylistSongSortType,
    #[serde(deserialize_with = "or_default")]
    pub playlist_song_sort_descending: bool,
    #[serde(deserialize_with = "or_default")]
    pub auto_playlist_song_sort_type: AutoPlaylistSongSortType,
    #[serde(deserialize_with = "or_default")]
    pub auto_playlist_song_sort_descending: bool,
    #[serde(deserialize_with = "or_default")]
    pub artist_sort_type: ArtistSortType,
    #[serde(deserialize_with = "or_default")]
    pub artist_sort_descending: bool,
    #[serde(deserialize_with = "or_default")]
    pub artist_song_sort_type: ArtistSongSortType,
    #[serde(deserialize_with = "or_default")]
    pub artist_song_sort_descending: bool,
    #[serde(deserialize_with = "or_default")]
    pub album_sort_type: AlbumSortType,
    #[serde(deserialize_with = "or_default")]
    pub album_sort_descending: bool,
    #[serde(deserialize_with = "or_default")]
    pub playlist_sort_type: PlaylistSortType,
    #[serde(deserialize_with = "or_default")]
    pub playlist_sort_descending: bool,
    #[serde(deserialize_with = "or_default")]
    pub song_filter: SongFilter,
    #[serde(deserialize_with = "or_default")]
    pub artist_filter: ArtistFilter,
    #[serde(deserialize_with = "or_default")]
    pub album_filter: AlbumFilter,
    #[serde(deserialize_with = "or_default")]
    pub chip_sort_type: String,
    #[serde(deserialize_with = "or_default")]
    pub playlist_tags_filter: String,
    #[serde(deserialize_with = "or_default")]
    pub library_chip_order: String,
    #[serde(deserialize_with = "or_default")]
    pub playlist_tag_order: String,
    #[serde(deserialize_with = "or_default")]
    pub show_home_category_chips: bool,
    #[serde(deserialize_with = "or_default")]
    pub show_tags_in_library: bool,
    #[serde(deserialize_with = "or_default")]
    pub quick_picks: QuickPicks,
    #[serde(default, deserialize_with = "vec_filtered")]
    pub speed_dial_song_ids: Vec<String>,
    #[serde(deserialize_with = "or_default")]
    pub top_range: TopRange,
    #[serde(deserialize_with = "or_default")]
    pub playlist_suggestion_source: PlaylistSuggestionSource,
    #[serde(deserialize_with = "or_default")]
    pub artist_separators: String,
    #[serde(deserialize_with = "or_default")]
    pub auto_load_more: bool,
    #[serde(deserialize_with = "or_default")]
    pub local_songs_min_duration_secs: i32,
    #[serde(deserialize_with = "or_default")]
    pub playlist_edit_lock: bool,
    #[serde(deserialize_with = "or_default")]
    pub prefer_local_on_import: bool,
}

impl Default for Content {
    fn default() -> Self {
        Self {
            content_language: "en".to_owned(),
            content_country: "US".to_owned(),
            hide_explicit: false,
            hide_video: false,
            search_source: SearchSource::Online,
            song_sort_type: SongSortType::CreateDate,
            song_sort_descending: false,
            playlist_song_sort_type: PlaylistSongSortType::Custom,
            playlist_song_sort_descending: false,
            auto_playlist_song_sort_type: AutoPlaylistSongSortType::CreateDate,
            auto_playlist_song_sort_descending: false,
            artist_sort_type: ArtistSortType::CreateDate,
            artist_sort_descending: false,
            artist_song_sort_type: ArtistSongSortType::CreateDate,
            artist_song_sort_descending: false,
            album_sort_type: AlbumSortType::CreateDate,
            album_sort_descending: false,
            playlist_sort_type: PlaylistSortType::CreateDate,
            playlist_sort_descending: false,
            song_filter: SongFilter::Library,
            artist_filter: ArtistFilter::Library,
            album_filter: AlbumFilter::Library,
            chip_sort_type: String::new(),
            playlist_tags_filter: String::new(),
            library_chip_order: String::new(),
            playlist_tag_order: String::new(),
            show_home_category_chips: true,
            show_tags_in_library: true,
            quick_picks: QuickPicks::QuickPicks,
            speed_dial_song_ids: Vec::new(),
            top_range: TopRange::AllTime,
            playlist_suggestion_source: PlaylistSuggestionSource::Both,
            artist_separators: ",;".to_owned(),
            auto_load_more: true,
            local_songs_min_duration_secs: 30,
            playlist_edit_lock: false,
            prefer_local_on_import: false,
        }
    }
}

impl Content {
    pub fn sanitize(&mut self) {
        self.content_language = self.content_language.trim().to_owned();
        if self.content_language.is_empty() {
            self.content_language = "en".to_owned();
        }
        self.content_country = self.content_country.trim().to_owned();
        if self.content_country.is_empty() {
            self.content_country = "US".to_owned();
        }
        self.local_songs_min_duration_secs =
            crate::sanitize::clamp_i32(self.local_songs_min_duration_secs, 0, 3600);
        self.speed_dial_song_ids = self
            .speed_dial_song_ids
            .iter()
            .map(|id| id.trim().to_owned())
            .filter(|id| !id.is_empty())
            .collect();
    }
}
