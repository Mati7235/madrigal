use serde_json::{json, Value};
use ytmapi_rs::Client;
use ytmapi_rs::auth::{AuthToken, noauth::NoAuthToken};
use ytmapi_rs::common::{AlbumID, ArtistChannelID, PlaylistID, SearchSuggestion, VideoID, YoutubeID};
use ytmapi_rs::parse::{
    GetAlbum, GetArtist, GetPlaylistDetails, ParseFrom, PlaylistItem, SearchResultAlbum,
    SearchResultArtist, SearchResultSong, WatchPlaylistTrack,
};
use ytmapi_rs::query::search::{AlbumsFilter, ArtistsFilter, FilteredSearch, SearchQuery, SongsFilter};
use ytmapi_rs::query::{
    GetAlbumQuery, GetArtistQuery, GetMoodCategoriesQuery, GetPlaylistDetailsQuery,
    GetPlaylistTracksQuery, GetSearchSuggestionsQuery, GetWatchPlaylistQuery, PostQuery,
};
use ytmapi_rs::json::Json;
use ytmapi_rs::{ProcessedResult, YtMusic};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locale {
    pub hl: String,
    pub gl: String,
}

impl Locale {
    pub fn new(hl: impl Into<String>, gl: impl Into<String>) -> Self {
        Self {
            hl: hl.into(),
            gl: gl.into(),
        }
    }

    pub fn context(&self, client_version: &str) -> Value {
        json!({
            "client": {
                "clientName": "WEB_REMIX",
                "clientVersion": client_version,
                "hl": self.hl,
                "gl": self.gl,
                "user": {},
            },
            "user": {},
            "request": {},
        })
    }
}

impl Default for Locale {
    fn default() -> Self {
        Self::new("en", "US")
    }
}

const INNERTUBE_API_URL: &str = "https://music.youtube.com/youtubei/v1/";
const INNERTUBE_PARAMS: &str =
    "?alt=json&prettyPrint=false&key=AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30";

// ytmapi-rs 0.3.4 hardcodes the innertube context without hl/gl.
pub(crate) async fn localized_json<Q, A>(
    client: &Client,
    token: &A,
    locale: &Locale,
    query: &Q,
) -> Result<Value, Error>
where
    Q: PostQuery,
    A: AuthToken,
{
    let url = format!("{INNERTUBE_API_URL}{}{INNERTUBE_PARAMS}", query.path());
    let mut body = json!({ "context": locale.context(token.client_version().as_ref()) });
    if let Some(map) = body.as_object_mut() {
        map.append(&mut query.header());
    }
    let params = query.params();
    let response = client
        .post_json_query(url, token.headers()?, &body, &params)
        .await?;
    if !(200..300).contains(&response.status_code) {
        return Err(Error::Request(format!(
            "innertube returned HTTP {}",
            response.status_code
        )));
    }
    let value: Value = serde_json::from_str(&response.text)?;
    if let Some(error) = value.pointer("/error") {
        let message = error
            .pointer("/message")
            .and_then(Value::as_str)
            .unwrap_or("innertube error");
        return Err(Error::Request(message.to_owned()));
    }
    Ok(value)
}

pub struct CatalogClient {
    yt: YtMusic<NoAuthToken>,
    client: Client,
    token: NoAuthToken,
    locale: Locale,
}

impl CatalogClient {
    pub async fn new() -> Result<Self, Error> {
        Self::with_locale(Locale::default()).await
    }

    pub async fn with_locale(locale: Locale) -> Result<Self, Error> {
        let client = Client::new()?;
        let token = NoAuthToken::new(&client).await?;
        let yt = YtMusic::from_auth_token(token.clone());
        Ok(Self {
            yt,
            client,
            token,
            locale,
        })
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn into_parts(self) -> (YtMusic<NoAuthToken>, Client) {
        (self.yt, self.client)
    }

    pub async fn raw_json<Q>(&self, query: Q) -> Result<Value, Error>
    where
        Q: PostQuery,
    {
        localized_json(&self.client, &self.token, &self.locale, &query).await
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
                self.raw_json(SearchQuery::new_filtered(query, AlbumsFilter))
                    .await?
            }
        };
        Self::parse_search(filter, &raw.to_string())
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
            .raw_json(GetSearchSuggestionsQuery::new(query))
            .await?;
        Self::parse_suggestions(&raw.to_string())
    }

    pub async fn artist_page(&self, channel_id: &str) -> Result<ArtistPage, Error> {
        let raw = self
            .raw_json(GetArtistQuery::new(ArtistChannelID::from_raw(channel_id)))
            .await?;
        Self::parse_artist_page(channel_id, &raw.to_string())
    }

    pub async fn album_page(&self, browse_id: &str) -> Result<AlbumPage, Error> {
        let raw = self
            .raw_json(GetAlbumQuery::new(AlbumID::from_raw(browse_id)))
            .await?;
        Self::parse_album_page(browse_id, &raw.to_string())
    }

    pub async fn playlist_page(&self, playlist_id: &str) -> Result<PlaylistPage, Error> {
        let details = self
            .raw_json(GetPlaylistDetailsQuery::new(PlaylistID::from_raw(
                playlist_id,
            )))
            .await?;
        let tracks = self
            .raw_json(GetPlaylistTracksQuery::new(PlaylistID::from_raw(playlist_id)))
            .await?;
        Self::parse_playlist_page(playlist_id, &details.to_string(), &tracks.to_string())
    }

    pub async fn watch_playlist(&self, video_id: &str) -> Result<Vec<Track>, Error> {
        let raw = self
            .raw_json(
                GetWatchPlaylistQuery::new_from_video_id(VideoID::from_raw(video_id)),
            )
            .await?;
        Self::parse_watch_playlist(&raw.to_string())
    }

    pub async fn mood_listing(&self) -> Result<MoodListing, Error> {
        let raw = self.raw_json(GetMoodCategoriesQuery).await?;
        Self::parse_mood_listing(&raw.to_string())
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
        let mut page = mapping::artist_page(&parsed);
        let thumbnails = row_thumbnails(source);
        for song in &mut page.songs {
            if song.thumbnail.is_none() {
                song.thumbnail = thumbnails.get(song.identity.id.as_str()).cloned();
            }
        }
        Ok(page)
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

fn row_thumbnails(source: &str) -> std::collections::HashMap<String, String> {
    fn walk(value: &serde_json::Value, found: &mut std::collections::HashMap<String, String>) {
        if let Some(renderer) = value.get("musicResponsiveListItemRenderer") {
            let id = renderer
                .pointer("/playlistItemData/videoId")
                .and_then(serde_json::Value::as_str);
            let best = renderer
                .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails")
                .and_then(serde_json::Value::as_array)
                .and_then(|thumbnails| {
                    thumbnails
                        .iter()
                        .filter_map(|thumb| {
                            let url = thumb.get("url")?.as_str()?;
                            let area = thumb
                                .get("width")
                                .and_then(serde_json::Value::as_u64)
                                .unwrap_or(0)
                                .saturating_mul(
                                    thumb
                                        .get("height")
                                        .and_then(serde_json::Value::as_u64)
                                        .unwrap_or(0),
                                );
                            Some((area, url))
                        })
                        .max_by_key(|(area, _)| *area)
                        .map(|(_, url)| url)
                });
            if let (Some(id), Some(url)) = (id, best) {
                found.insert(id.to_owned(), url.to_owned());
            }
        }
        match value {
            serde_json::Value::Object(entries) => {
                for child in entries.values() {
                    walk(child, found);
                }
            }
            serde_json::Value::Array(items) => {
                for child in items {
                    walk(child, found);
                }
            }
            _ => {}
        }
    }
    let mut found = std::collections::HashMap::new();
    if let Ok(value) = serde_json::from_str(source) {
        walk(&value, &mut found);
    }
    found
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contexto_lleva_idioma_y_pais() {
        let context = Locale::new("es", "AR").context("1.20261118.01.00");
        assert_eq!(context.pointer("/client/hl").and_then(Value::as_str), Some("es"));
        assert_eq!(context.pointer("/client/gl").and_then(Value::as_str), Some("AR"));
        assert_eq!(
            context.pointer("/client/clientName").and_then(Value::as_str),
            Some("WEB_REMIX")
        );
        assert_eq!(
            context.pointer("/client/clientVersion").and_then(Value::as_str),
            Some("1.20261118.01.00")
        );
    }

    #[test]
    fn locale_por_defecto_es_ingles_estados_unidos() {
        let locale = Locale::default();
        assert_eq!(locale.hl, "en");
        assert_eq!(locale.gl, "US");
    }
}

