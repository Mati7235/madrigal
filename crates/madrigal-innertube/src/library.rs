use ytmapi_rs::Client;
use ytmapi_rs::auth::BrowserToken;
use ytmapi_rs::common::{AlbumID, ArtistChannelID, PlaylistID, YoutubeID};
use ytmapi_rs::parse::{
    HistoryItem, HistoryPeriod as NativePeriod, LibraryPlaylist as NativePlaylist, ParseFrom,
    TableListSong,
};
use ytmapi_rs::query::search::{AlbumsFilter, ArtistsFilter, SongsFilter};
use ytmapi_rs::query::{
    GetAlbumQuery, GetArtistQuery, GetHistoryQuery, GetLibraryPlaylistsQuery,
    GetLibrarySongsQuery, GetPlaylistDetailsQuery, GetPlaylistTracksQuery,
    GetSearchSuggestionsQuery, PostQuery, SearchQuery,
};
use ytmapi_rs::{ProcessedResult, YtMusic};

use madrigal_core::{Playlist, PlaylistId, PlaylistKind, Track, TrackIdentity, parse_duration};

use crate::browse::{BrowsePage, GetBrowseQuery};
use crate::charts::{ChartsPage, GetChartsQuery, GetNewReleasesQuery, NewReleases};
use crate::client::{localized_json, Locale, SearchFilter, SearchOutcome};
use crate::error::Error;
use crate::home::{GetHomeQuery, HomeFeed};
use crate::mapping;
use crate::pages::{AlbumPage, ArtistPage, MoodListing, PlaylistPage};
use crate::CatalogClient;

pub struct AuthCatalogClient {
    pub(crate) yt: YtMusic<BrowserToken>,
    client: Client,
    token: BrowserToken,
    locale: Locale,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryPeriod {
    pub name: String,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct HistoryFeed {
    pub periods: Vec<HistoryPeriod>,
}

impl CatalogClient {
    pub fn with_cookies(cookie_header: &str) -> Result<AuthCatalogClient, Error> {
        Self::with_cookies_locale(cookie_header, Locale::default())
    }

    pub fn with_cookies_locale(
        cookie_header: &str,
        locale: Locale,
    ) -> Result<AuthCatalogClient, Error> {
        let pairs = madrigal_account::cookies::parse_cookie_header(cookie_header)
            .map_err(|e| Error::Fixture(e.to_string()))?;
        if !madrigal_account::cookies::has_login_cookie(&pairs) {
            return Err(Error::Fixture(
                "cookie header lacks login cookies".to_owned(),
            ));
        }
        let sapisid = madrigal_account::cookies::login_cookie_value(&pairs)
            .ok_or_else(|| Error::Fixture("cookie header lacks SAPISID".to_owned()))?;
        let token: BrowserToken = serde_json::from_value(serde_json::json!({
            "sapisid": sapisid,
            "client_version": "1.20261118.01.00",
            "cookies": cookie_header,
        }))
        // BrowserToken has no sync constructor; field names pinned to ytmapi-rs =0.3.4.
        .map_err(|_| Error::Fixture("browser token shape changed".to_owned()))?;
        Self::auth_from_token(token, locale)
    }

    fn auth_from_token(token: BrowserToken, locale: Locale) -> Result<AuthCatalogClient, Error> {
        let client = Client::new()?;
        Ok(AuthCatalogClient {
            yt: YtMusic::from_auth_token(token.clone()),
            client,
            token,
            locale,
        })
    }
}

impl AuthCatalogClient {
    pub fn from_token(token: BrowserToken) -> Result<Self, Error> {
        CatalogClient::auth_from_token(token, Locale::default())
    }

    pub async fn liked_songs(&self) -> Result<Vec<Track>, Error> {
        Ok(self
            .yt
            .get_library_songs()
            .await?
            .iter()
            .map(table_song_to_track)
            .collect())
    }

    pub async fn library_playlists(&self) -> Result<Vec<Playlist>, Error> {
        Ok(self
            .yt
            .get_library_playlists()
            .await?
            .iter()
            .map(library_playlist_to_playlist)
            .collect())
    }

    pub async fn history(&self) -> Result<HistoryFeed, Error> {
        Ok(history_from_native(&self.yt.get_history().await?))
    }

    async fn raw_json<Q>(&self, query: Q) -> Result<serde_json::Value, Error>
    where
        Q: PostQuery,
    {
        localized_json(&self.client, &self.token, &self.locale, &query).await
    }

    pub async fn get_home(&self) -> Result<HomeFeed, Error> {
        let value = self.raw_json(GetHomeQuery).await?;
        HomeFeed::from_value(&value)
    }

    pub async fn suggestions(&self, query: &str) -> Result<Vec<String>, Error> {
        let raw = self
            .raw_json(GetSearchSuggestionsQuery::new(query))
            .await?;
        CatalogClient::parse_suggestions(&raw.to_string())
    }

    pub async fn search(&self, query: &str, filter: SearchFilter) -> Result<SearchOutcome, Error> {
        let raw = match filter {
            SearchFilter::Songs => {
                self.raw_json(SearchQuery::new_filtered(query, SongsFilter))
                    .await?
            }
            SearchFilter::Artists => {
                self.raw_json(SearchQuery::new_filtered(query, ArtistsFilter))
                    .await?
            }
            SearchFilter::Albums => {
                // ytmapi-rs parses the album type label in English only.
                let mut locale = self.locale.clone();
                locale.hl = "en".to_owned();
                let query = SearchQuery::new_filtered(query, AlbumsFilter);
                localized_json(&self.client, &self.token, &locale, &query).await?
            }
        };
        CatalogClient::parse_search(filter, &raw.to_string())
    }

    pub async fn mood_listing(&self) -> Result<MoodListing, Error> {
        let raw = self
            .raw_json(ytmapi_rs::query::GetMoodCategoriesQuery)
            .await?;
        CatalogClient::parse_mood_listing(&raw.to_string())
    }

    pub async fn browse(&self, browse_id: &str, params: Option<&str>) -> Result<BrowsePage, Error> {
        let query = match params {
            Some(params) => GetBrowseQuery::with_params(browse_id, params),
            None => GetBrowseQuery::new(browse_id),
        };
        let value = self.raw_json(query).await?;
        CatalogClient::parse_browse(&value.to_string())
    }

    pub async fn charts(&self, country: &str) -> Result<ChartsPage, Error> {
        let value = self.raw_json(GetChartsQuery::new(country)).await?;
        CatalogClient::parse_charts(&value.to_string())
    }

    pub async fn charts_page(&self, country: &str) -> Result<BrowsePage, Error> {
        let value = self.raw_json(GetChartsQuery::new(country)).await?;
        CatalogClient::parse_charts_page(&value.to_string())
    }

    pub async fn artist_page(&self, channel_id: &str) -> Result<ArtistPage, Error> {
        let raw = self
            .raw_json(GetArtistQuery::new(ArtistChannelID::from_raw(channel_id)))
            .await?;
        CatalogClient::parse_artist_page(channel_id, &raw.to_string())
    }

    pub async fn album_page(&self, browse_id: &str) -> Result<AlbumPage, Error> {
        let raw = self
            .raw_json(GetAlbumQuery::new(AlbumID::from_raw(browse_id)))
            .await?;
        CatalogClient::parse_album_page(browse_id, &raw.to_string())
    }

    pub async fn playlist_page(&self, playlist_id: &str) -> Result<PlaylistPage, Error> {
        let details_raw = self
            .raw_json(GetPlaylistDetailsQuery::new(PlaylistID::from_raw(
                playlist_id,
            )))
            .await?;
        let tracks_raw = self
            .raw_json(GetPlaylistTracksQuery::new(PlaylistID::from_raw(playlist_id)))
            .await?;
        CatalogClient::parse_playlist_page(
            playlist_id,
            &details_raw.to_string(),
            &tracks_raw.to_string(),
        )
    }

    pub async fn new_releases(&self) -> Result<NewReleases, Error> {
        let value = self.raw_json(GetNewReleasesQuery).await?;
        CatalogClient::parse_new_releases(&value.to_string())
    }

    pub fn parse_liked_songs(source: &str) -> Result<Vec<Track>, Error> {
        let query = GetLibrarySongsQuery::default();
        let parsed = offline::<GetLibrarySongsQuery, Vec<TableListSong>>(&query, source)?;
        Ok(parsed.iter().map(table_song_to_track).collect())
    }

    pub fn parse_library_playlists(source: &str) -> Result<Vec<Playlist>, Error> {
        let query = GetLibraryPlaylistsQuery;
        let parsed =
            offline::<GetLibraryPlaylistsQuery, Vec<NativePlaylist>>(&query, source)?;
        Ok(parsed.iter().map(library_playlist_to_playlist).collect())
    }

    pub fn parse_history(source: &str) -> Result<HistoryFeed, Error> {
        let query = GetHistoryQuery;
        let parsed = offline::<GetHistoryQuery, Vec<NativePeriod>>(&query, source)?;
        Ok(history_from_native(&parsed))
    }
}

fn history_from_native(periods: &[NativePeriod]) -> HistoryFeed {
    HistoryFeed {
        periods: periods
            .iter()
            .map(|p| HistoryPeriod {
                name: p.period_name.clone(),
                tracks: p.items.iter().filter_map(history_item_to_track).collect(),
            })
            .collect(),
    }
}

fn history_item_to_track(item: &HistoryItem) -> Option<Track> {
    match item {
        HistoryItem::Song(song) => Some(Track {
            identity: TrackIdentity::youtube(song.video_id.get_raw()),
            title: song.title.clone(),
            artists: song.artists.iter().map(|a| a.name.clone()).collect(),
            album: Some(song.album.name.clone()),
            duration_secs: parse_duration(&song.duration),
            thumbnail: mapping::best_thumbnail(&song.thumbnails),
            track_number: None,
            local_path: None,
            set_video_id: None,
        }),
        HistoryItem::Video(video) => Some(Track {
            identity: TrackIdentity::youtube(video.video_id.get_raw()),
            title: video.title.clone(),
            artists: vec![video.channel_name.clone()],
            album: None,
            duration_secs: parse_duration(&video.duration),
            thumbnail: mapping::best_thumbnail(&video.thumbnails),
            track_number: None,
            local_path: None,
            set_video_id: None,
        }),
        HistoryItem::Episode(_) | HistoryItem::UploadSong(_) => None,
    }
}

fn table_song_to_track(song: &TableListSong) -> Track {
    Track {
        identity: TrackIdentity::youtube(song.video_id.get_raw()),
        title: song.title.clone(),
        artists: song.artists.iter().map(|a| a.name.clone()).collect(),
        album: Some(song.album.name.clone()),
        duration_secs: parse_duration(&song.duration),
        thumbnail: mapping::best_thumbnail(&song.thumbnails),
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

fn library_playlist_to_playlist(playlist: &NativePlaylist) -> Playlist {
    Playlist {
        id: PlaylistId::new(playlist.playlist_id.get_raw()),
        kind: PlaylistKind::Remote,
        title: playlist.title.clone(),
        author: Some(playlist.author.clone()),
        description: None,
        thumbnail: mapping::best_thumbnail(&playlist.thumbnails),
        track_count: mapping::leading_count(&playlist.tracks),
        owned: false,
        privacy: None,
        author_id: playlist.author_id.as_ref().map(|id| id.get_raw().to_owned()),
    }
}

fn offline<Q, O>(query: &Q, source: &str) -> Result<O, Error>
where
    O: ParseFrom<Q>,
{
    let json: ytmapi_rs::json::Json = serde_json::from_str(source)?;
    let processed = ProcessedResult {
        query,
        source: source.to_owned(),
        json,
    };
    Ok(O::parse_from(processed)?)
}
