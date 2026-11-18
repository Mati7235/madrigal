use ytmapi_rs::Client;
use ytmapi_rs::auth::noauth::NoAuthToken;
use ytmapi_rs::common::{AlbumID, ArtistChannelID, PlaylistID, SearchSuggestion, VideoID, YoutubeID};
use ytmapi_rs::parse::{
    GetAlbum, GetArtist, GetPlaylistDetails, ParseFrom, PlaylistItem, SearchResultAlbum,
    SearchResultArtist, SearchResultSong, WatchPlaylistTrack,
};
use ytmapi_rs::query::search::{AlbumsFilter, ArtistsFilter, FilteredSearch, SearchQuery, SongsFilter};
use ytmapi_rs::query::{
    GetAlbumQuery, GetArtistQuery, GetMoodCategoriesQuery, GetPlaylistDetailsQuery,
    GetPlaylistTracksQuery, GetSearchSuggestionsQuery, GetWatchPlaylistQuery, Query,
};
use ytmapi_rs::json::Json;
use ytmapi_rs::{ProcessedResult, YtMusic, YtMusicBuilder};

use madrigal_core::{Album, Artist, Track};

use crate::error::Error;
use crate::mapping;
use crate::pages::{AlbumPage, ArtistPage, MoodListing, PlaylistPage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchFilter {
    Songs,
    Artists,
    Albums,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchOutcome {
    Songs(Vec<Track>),
    Artists(Vec<Artist>),
    Albums(Vec<Album>),
}

pub struct CatalogClient {
    yt: YtMusic<NoAuthToken>,
    client: Client,
}

impl CatalogClient {
    pub async fn new() -> Result<Self, Error> {
        let client = Client::new()?;
        let yt = YtMusicBuilder::new_with_client(client.clone()).build().await?;
        Ok(Self { yt, client })
    }

    pub fn from_inner(yt: YtMusic<NoAuthToken>, client: Client) -> Self {
        Self { yt, client }
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn into_parts(self) -> (YtMusic<NoAuthToken>, Client) {
        (self.yt, self.client)
    }

    pub async fn raw_json<Q>(&self, query: Q) -> Result<serde_json::Value, Error>
    where
        Q: Query<NoAuthToken>,
    {
        Ok(self.yt.json_query(query).await?.into_inner())
    }

    pub async fn search(&self, query: &str, filter: SearchFilter) -> Result<SearchOutcome, Error> {
        match filter {
            SearchFilter::Songs => {
                let raw = self
                    .yt
                    .raw_json_query(SearchQuery::new_filtered(query, SongsFilter))
                    .await?;
                Self::parse_search(filter, &raw)
            }
            SearchFilter::Artists => {
                let raw = self
                    .yt
                    .raw_json_query(SearchQuery::new_filtered(query, ArtistsFilter))
                    .await?;
                Self::parse_search(filter, &raw)
            }
            SearchFilter::Albums => {
                let raw = self
                    .yt
                    .raw_json_query(SearchQuery::new_filtered(query, AlbumsFilter))
                    .await?;
                Self::parse_search(filter, &raw)
            }
        }
    }

    pub async fn search_songs(&self, query: &str) -> Result<Vec<Track>, Error> {
        match self.search(query, SearchFilter::Songs).await? {
            SearchOutcome::Songs(tracks) => Ok(tracks),
            _ => Err(Error::Fixture("songs query returned other outcome".to_owned())),
        }
    }

    pub async fn search_artists(&self, query: &str) -> Result<Vec<Artist>, Error> {
        match self.search(query, SearchFilter::Artists).await? {
            SearchOutcome::Artists(artists) => Ok(artists),
            _ => Err(Error::Fixture(
                "artists query returned other outcome".to_owned(),
            )),
        }
    }

    pub async fn search_albums(&self, query: &str) -> Result<Vec<Album>, Error> {
        match self.search(query, SearchFilter::Albums).await? {
            SearchOutcome::Albums(albums) => Ok(albums),
            _ => Err(Error::Fixture(
                "albums query returned other outcome".to_owned(),
            )),
        }
    }

    pub async fn suggestions(&self, query: &str) -> Result<Vec<String>, Error> {
        let raw = self
            .yt
            .raw_json_query(GetSearchSuggestionsQuery::new(query))
            .await?;
        Self::parse_suggestions(&raw)
    }

    pub async fn artist_page(&self, channel_id: &str) -> Result<ArtistPage, Error> {
        let raw = self
            .yt
            .raw_json_query(GetArtistQuery::new(ArtistChannelID::from_raw(channel_id)))
            .await?;
        Self::parse_artist_page(channel_id, &raw)
    }

    pub async fn album_page(&self, browse_id: &str) -> Result<AlbumPage, Error> {
        let raw = self
            .yt
            .raw_json_query(GetAlbumQuery::new(AlbumID::from_raw(browse_id)))
            .await?;
        Self::parse_album_page(browse_id, &raw)
    }

    pub async fn playlist_page(&self, playlist_id: &str) -> Result<PlaylistPage, Error> {
        let details_raw = self
            .yt
            .raw_json_query(GetPlaylistDetailsQuery::new(PlaylistID::from_raw(
                playlist_id,
            )))
            .await?;
        let tracks_raw = self
            .yt
            .raw_json_query(GetPlaylistTracksQuery::new(PlaylistID::from_raw(playlist_id)))
            .await?;
        Self::parse_playlist_page(playlist_id, &details_raw, &tracks_raw)
    }

    pub async fn watch_playlist(&self, video_id: &str) -> Result<Vec<Track>, Error> {
        let raw = self
            .yt
            .raw_json_query(GetWatchPlaylistQuery::new_from_video_id(VideoID::from_raw(
                video_id,
            )))
            .await?;
        Self::parse_watch_playlist(&raw)
    }

    pub async fn mood_listing(&self) -> Result<MoodListing, Error> {
        let query = GetMoodCategoriesQuery;
        let json = self.yt.json_query(query.clone()).await?;
        let processed = ProcessedResult {
            query: &query,
            source: String::new(),
            json,
        };
        Ok(MoodListing::parse_from(processed)?)
    }

    pub fn parse_search(filter: SearchFilter, source: &str) -> Result<SearchOutcome, Error> {
        match filter {
            SearchFilter::Songs => Ok(SearchOutcome::Songs(Self::parse_search_songs(source)?)),
            SearchFilter::Artists => Ok(SearchOutcome::Artists(Self::parse_search_artists(source)?)),
            SearchFilter::Albums => Ok(SearchOutcome::Albums(Self::parse_search_albums(source)?)),
        }
    }

    pub fn parse_search_songs(source: &str) -> Result<Vec<Track>, Error> {
        let query = SearchQuery::new_filtered("", SongsFilter);
        let parsed = offline::<SearchQuery<FilteredSearch<SongsFilter>>, Vec<SearchResultSong>>(
            &query, source,
        )?;
        Ok(parsed.iter().map(mapping::song_to_track).collect())
    }

    pub fn parse_search_artists(source: &str) -> Result<Vec<Artist>, Error> {
        let query = SearchQuery::new_filtered("", ArtistsFilter);
        let parsed = offline::<
            SearchQuery<FilteredSearch<ArtistsFilter>>,
            Vec<SearchResultArtist>,
        >(&query, source)?;
        Ok(parsed.iter().map(mapping::artist_to_artist).collect())
    }

    pub fn parse_search_albums(source: &str) -> Result<Vec<Album>, Error> {
        let query = SearchQuery::new_filtered("", AlbumsFilter);
        let parsed = offline::<SearchQuery<FilteredSearch<AlbumsFilter>>, Vec<SearchResultAlbum>>(
            &query, source,
        )?;
        Ok(parsed.iter().map(mapping::album_to_album).collect())
    }

    pub fn parse_suggestions(source: &str) -> Result<Vec<String>, Error> {
        let query = GetSearchSuggestionsQuery::new("");
        let parsed =
            offline::<GetSearchSuggestionsQuery, Vec<SearchSuggestion>>(&query, source)?;
        Ok(parsed.iter().map(|s| s.get_text()).collect())
    }

    pub fn parse_artist_page(channel_id: &str, source: &str) -> Result<ArtistPage, Error> {
        let query = GetArtistQuery::new(ArtistChannelID::from_raw(channel_id));
        let parsed = offline::<GetArtistQuery, GetArtist>(&query, source)?;
        Ok(mapping::artist_page(&parsed))
    }

    pub fn parse_album_page(browse_id: &str, source: &str) -> Result<AlbumPage, Error> {
        let query = GetAlbumQuery::new(AlbumID::from_raw(browse_id));
        let parsed = offline::<GetAlbumQuery, GetAlbum>(&query, source)?;
        Ok(mapping::album_page(browse_id, &parsed))
    }

    pub fn parse_playlist_page(
        playlist_id: &str,
        details_source: &str,
        tracks_source: &str,
    ) -> Result<PlaylistPage, Error> {
        let details_query =
            GetPlaylistDetailsQuery::new(PlaylistID::from_raw(playlist_id));
        let details =
            offline::<GetPlaylistDetailsQuery, GetPlaylistDetails>(&details_query, details_source)?;
        let tracks_query = GetPlaylistTracksQuery::new(PlaylistID::from_raw(playlist_id));
        let items = offline::<GetPlaylistTracksQuery, Vec<PlaylistItem>>(&tracks_query, tracks_source)?;
        Ok(mapping::playlist_page(&details, &items))
    }

    pub fn parse_watch_playlist(source: &str) -> Result<Vec<Track>, Error> {
        let query = GetWatchPlaylistQuery::new_from_video_id(VideoID::from_raw(""));
        let parsed = offline::<
            GetWatchPlaylistQuery<VideoID<'_>>,
            Vec<WatchPlaylistTrack>,
        >(&query, source)?;
        Ok(parsed.iter().map(mapping::watch_track_to_track).collect())
    }

    pub fn parse_mood_listing(source: &str) -> Result<MoodListing, Error> {
        let query = GetMoodCategoriesQuery;
        offline::<GetMoodCategoriesQuery, MoodListing>(&query, source)
    }
}

fn offline<Q, O>(query: &Q, source: &str) -> Result<O, Error>
where
    O: ParseFrom<Q>,
{
    let json: Json = serde_json::from_str(source)?;
    let processed = ProcessedResult {
        query,
        source: source.to_owned(),
        json,
    };
    Ok(O::parse_from(processed)?)
}
