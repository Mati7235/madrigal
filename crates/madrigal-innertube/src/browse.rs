use std::borrow::Cow;

use madrigal_core::{
    Album, AlbumId, Artist, ArtistId, Playlist, PlaylistId, PlaylistKind, Track, TrackIdentity,
};
use ytmapi_rs::auth::AuthToken;
use ytmapi_rs::common::{MoodCategoryParams, YoutubeID};
use ytmapi_rs::json::Json;
use ytmapi_rs::parse::{MoodPlaylistCategory, ParseFrom, ProcessedResult};
use ytmapi_rs::query::{GetMoodPlaylistsQuery, PostMethod, PostQuery, Query};

use crate::error::Error;
use crate::mapping;
use crate::CatalogClient;

pub struct GetBrowseQuery {
    browse_id: String,
    params: Option<String>,
}

impl GetBrowseQuery {
    pub fn new(browse_id: &str) -> Self {
        Self {
            browse_id: browse_id.to_owned(),
            params: None,
        }
    }

    pub fn with_params(browse_id: &str, params: &str) -> Self {
        Self {
            browse_id: browse_id.to_owned(),
            params: Some(params.to_owned()),
        }
    }
}

impl<A: AuthToken> Query<A> for GetBrowseQuery {
    type Output = BrowsePage;
    type Method = PostMethod;
}

impl PostQuery for GetBrowseQuery {
    fn header(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut map = serde_json::Map::new();
        map.insert(
            "browseId".to_owned(),
            serde_json::Value::String(self.browse_id.clone()),
        );
        if let Some(params) = &self.params {
            map.insert(
                "params".to_owned(),
                serde_json::Value::String(params.clone()),
            );
        }
        map
    }

    fn params(&self) -> Vec<(&str, Cow<'_, str>)> {
        vec![]
    }

    fn path(&self) -> &str {
        "browse"
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BrowseItem {
    Track(Track),
    Album(Album),
    Artist(Artist),
    Playlist(Playlist),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BrowseSection {
    pub title: String,
    pub items: Vec<BrowseItem>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BrowsePage {
    pub sections: Vec<BrowseSection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoodPlaylistGroup {
    pub name: String,
    pub playlists: Vec<Playlist>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MoodPlaylistsPage {
    pub categories: Vec<MoodPlaylistGroup>,
}

impl<'a> ParseFrom<GetMoodPlaylistsQuery<'a>> for MoodPlaylistsPage {
    fn parse_from(
        p: ProcessedResult<GetMoodPlaylistsQuery<'a>>,
    ) -> ytmapi_rs::Result<Self> {
        let native = Vec::<MoodPlaylistCategory>::parse_from(p)?;
        Ok(Self {
            categories: native
                .into_iter()
                .map(|category| MoodPlaylistGroup {
                    name: category.category_name,
                    playlists: category
                        .playlists
                        .iter()
                        .map(|playlist| Playlist {
                            id: PlaylistId::new(playlist.playlist_id.get_raw()),
                            kind: PlaylistKind::Remote,
                            title: playlist.title.clone(),
                            author: Some(playlist.author.clone()),
                            description: None,
                            thumbnail: mapping::best_thumbnail(&playlist.thumbnails),
                            track_count: None,
                            owned: false,
                            privacy: None,
                            author_id: None,
                        })
                        .collect(),
                })
                .collect(),
        })
    }
}

impl ParseFrom<GetBrowseQuery> for BrowsePage {
    fn parse_from(p: ProcessedResult<GetBrowseQuery>) -> ytmapi_rs::Result<Self> {
        let value = p.json.into_inner();
        page_from_value(&value).map_err(io_error)
    }
}

impl CatalogClient {
    pub async fn browse(&self, browse_id: &str, params: Option<&str>) -> Result<BrowsePage, Error> {
        let query = match params {
            Some(params) => GetBrowseQuery::with_params(browse_id, params),
            None => GetBrowseQuery::new(browse_id),
        };
        let value = self.raw_json(query).await?;
        Self::parse_browse(&value.to_string())
    }

    pub fn parse_browse(source: &str) -> Result<BrowsePage, Error> {
        let query = GetBrowseQuery::new("");
        offline_parse(&query, source)
    }

    pub async fn mood_playlists(&self, params: &str) -> Result<MoodPlaylistsPage, Error> {
        let query = GetMoodPlaylistsQuery::new(MoodCategoryParams::from_raw(params));
        let value = self.raw_json(query).await?;
        Self::parse_mood_playlists(&value.to_string())
    }

    pub fn parse_mood_playlists(source: &str) -> Result<MoodPlaylistsPage, Error> {
        let query = GetMoodPlaylistsQuery::new(MoodCategoryParams::from_raw(""));
        offline_parse(&query, source)
    }
}

fn offline_parse<Q, O>(query: &Q, source: &str) -> Result<O, Error>
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

pub(crate) fn page_from_value(value: &serde_json::Value) -> Result<BrowsePage, Error> {
    let sections = ptr(
        value,
        "/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents",
    )?;
    let list = sections
        .as_array()
        .ok_or_else(|| Error::Fixture("browse contents is not an array".to_owned()))?;
    let mut out = Vec::new();
    for section in list {
        if let Some(parsed) = section_from_value(section)? {
            out.push(parsed);
        }
    }
    Ok(BrowsePage { sections: out })
}

pub(crate) fn section_from_value(section: &serde_json::Value) -> Result<Option<BrowseSection>, Error> {
    if let Ok(grid) = ptr(section, "/gridRenderer") {
        let Some(title) = ptr(grid, "/header/gridHeaderRenderer/title")
            .ok()
            .and_then(|v| text(v).ok())
        else {
            return Ok(None);
        };
        let title = title.to_owned();
        let items_raw = ptr(grid, "/items")?;
        let entries = items_raw
            .as_array()
            .ok_or_else(|| Error::Fixture("browse grid items is not an array".to_owned()))?;
        let mut items = Vec::new();
        for entry in entries {
            if entry.get("musicTwoRowItemRenderer").is_some() {
                items.push(classify_two_row(entry)?);
            }
        }
        return Ok(unless_empty(title, items));
    }
    if let Ok(carousel) = ptr(section, "/musicCarouselShelfRenderer") {
        let title = text(ptr(
            carousel,
            "/header/musicCarouselShelfBasicHeaderRenderer/title",
        )?)?
        .to_owned();
        let mut items = Vec::new();
        carousel_items(carousel, "/contents", &mut items)?;
        return Ok(unless_empty(title, items));
    }
    if let Ok(carousel) = ptr(section, "/musicImmersiveCarouselShelfRenderer") {
        let title = text(ptr(
            carousel,
            "/header/musicCarouselShelfBasicHeaderRenderer/title",
        )?)?
        .to_owned();
        let mut items = Vec::new();
        carousel_items(carousel, "/contents", &mut items)?;
        return Ok(unless_empty(title, items));
    }
    if let Ok(shelf) = ptr(section, "/musicShelfRenderer") {
        let title = text(ptr(shelf, "/title")?)?.to_owned();
        let contents = ptr(shelf, "/contents")?;
        let entries = contents
            .as_array()
            .ok_or_else(|| Error::Fixture("browse shelf contents is not an array".to_owned()))?;
        let mut items = Vec::new();
        for entry in entries {
            if entry.get("musicResponsiveListItemRenderer").is_some() {
                items.push(classify_list_item(entry)?);
            }
        }
        return Ok(unless_empty(title, items));
    }
    Ok(None)
}

fn carousel_items(
    carousel: &serde_json::Value,
    pointer: &str,
    items: &mut Vec<BrowseItem>,
) -> Result<(), Error> {
    let contents = ptr(carousel, pointer)?;
    let entries = contents
        .as_array()
        .ok_or_else(|| Error::Fixture("browse carousel contents is not an array".to_owned()))?;
    for entry in entries {
        if entry.get("musicTwoRowItemRenderer").is_some() {
            items.push(classify_two_row(entry)?);
        } else if entry.get("musicResponsiveListItemRenderer").is_some() {
            items.push(classify_list_item(entry)?);
        }
    }
    Ok(())
}

fn unless_empty(title: String, items: Vec<BrowseItem>) -> Option<BrowseSection> {
    if items.is_empty() {
        None
    } else {
        Some(BrowseSection { title, items })
    }
}

fn classify_two_row(item: &serde_json::Value) -> Result<BrowseItem, Error> {
    let renderer = ptr(item, "/musicTwoRowItemRenderer")?;
    if let Some(browse_id) = ptr(
        renderer,
        "/title/runs/0/navigationEndpoint/browseEndpoint/browseId",
    )
    .ok()
    .and_then(|v| v.as_str())
    {
        if browse_id.starts_with("MPRE") {
            return Ok(BrowseItem::Album(mtrir_album(renderer)?));
        }
        if browse_id.starts_with("UC") {
            return Ok(BrowseItem::Artist(artist_from_parts(
                browse_id,
                text(ptr(renderer, "/title")?)?,
                thumbs_two_row(renderer),
            )));
        }
        return Ok(BrowseItem::Playlist(mtrir_playlist(renderer)?));
    }
    Ok(BrowseItem::Track(mtrir_track(renderer)?))
}

fn classify_list_item(item: &serde_json::Value) -> Result<BrowseItem, Error> {
    let renderer = ptr(item, "/musicResponsiveListItemRenderer")?;
    let head = ptr(
        renderer,
        "/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text/runs/0",
    )?;
    let title = head
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Fixture("missing list item title".to_owned()))?;
    if let Ok(video_id) = ptr(head, "/navigationEndpoint/watchEndpoint/videoId") {
        let video_id = video_id
            .as_str()
            .ok_or_else(|| Error::Fixture("browse list video id is not a string".to_owned()))?;
        return Ok(BrowseItem::Track(track_from_parts(
            video_id,
            title,
            &list_artists(renderer),
            thumbs_list(renderer),
        )));
    }
    let browse_id = ptr(head, "/navigationEndpoint/browseEndpoint/browseId")
        .ok()
        .and_then(|value| value.as_str())
        .or_else(|| {
            ptr(renderer, "/navigationEndpoint/browseEndpoint/browseId")
                .ok()
                .and_then(|value| value.as_str())
        })
        .ok_or_else(|| Error::Fixture("browse list id is not a string".to_owned()))?;
    if browse_id.starts_with("MPRE") {
        Ok(BrowseItem::Album(Album {
            id: AlbumId::new(browse_id),
            title: title.to_owned(),
            artists: list_artists(renderer),
            artist_id: None,
            year: None,
            thumbnail: thumbs_list(renderer),
            track_count: None,
            audio_playlist_id: None,
        }))
    } else if browse_id.starts_with("UC") {
        Ok(BrowseItem::Artist(artist_from_parts(
            browse_id,
            title,
            thumbs_list(renderer),
        )))
    } else {
        Ok(BrowseItem::Playlist(Playlist {
            id: PlaylistId::new(browse_id),
            kind: PlaylistKind::Remote,
            title: title.to_owned(),
            author: None,
            description: None,
            thumbnail: thumbs_list(renderer),
            track_count: None,
            owned: false,
            privacy: None,
            author_id: None,
        }))
    }
}

pub(crate) fn mtrir_album(renderer: &serde_json::Value) -> Result<Album, Error> {
    let browse_id = ptr(
        renderer,
        "/title/runs/0/navigationEndpoint/browseEndpoint/browseId",
    )?
    .as_str()
    .ok_or_else(|| Error::Fixture("album browse id is not a string".to_owned()))?;
    Ok(Album {
        id: AlbumId::new(browse_id),
        title: text(ptr(renderer, "/title")?)?.to_owned(),
        artists: subtitle_linked_artists(renderer),
        artist_id: None,
        year: subtitle_year(renderer),
        thumbnail: thumbs_two_row(renderer),
        track_count: None,
        audio_playlist_id: overlay_playlist_id(renderer),
    })
}

pub(crate) fn mtrir_playlist(renderer: &serde_json::Value) -> Result<Playlist, Error> {
    let browse_id = ptr(
        renderer,
        "/title/runs/0/navigationEndpoint/browseEndpoint/browseId",
    )?
    .as_str()
    .ok_or_else(|| Error::Fixture("playlist browse id is not a string".to_owned()))?;
    Ok(Playlist {
        id: PlaylistId::new(browse_id),
        kind: PlaylistKind::Remote,
        title: text(ptr(renderer, "/title")?)?.to_owned(),
        author: None,
        description: None,
        thumbnail: thumbs_two_row(renderer),
        track_count: None,
        owned: false,
        privacy: None,
        author_id: None,
    })
}

pub(crate) fn mtrir_track(renderer: &serde_json::Value) -> Result<Track, Error> {
    let video_id = two_row_video_id(renderer)?;
    Ok(track_from_parts(
        &video_id,
        text(ptr(renderer, "/title")?)?,
        &subtitle_artists(renderer),
        thumbs_two_row(renderer),
    ))
}

fn two_row_video_id(renderer: &serde_json::Value) -> Result<String, Error> {
    if let Some(video_id) = ptr(
        renderer,
        "/title/runs/0/navigationEndpoint/watchEndpoint/videoId",
    )
    .ok()
    .and_then(|v| v.as_str())
    {
        return Ok(video_id.to_owned());
    }
    // New-release carousels omit the title play endpoint; the id lives in the item menu.
    if let Some(video_id) = menu_video_id(renderer) {
        return Ok(video_id);
    }
    Err(Error::Fixture("video id missing".to_owned()))
}

fn menu_video_id(renderer: &serde_json::Value) -> Option<String> {
    let items = ptr(renderer, "/menu/menuRenderer/items").ok()?.as_array()?;
    for item in items {
        if let Some(video_id) = item
            .pointer("/menuNavigationItemRenderer/navigationEndpoint/watchEndpoint/videoId")
            .and_then(|v| v.as_str())
        {
            return Some(video_id.to_owned());
        }
        if let Some(video_id) = item
            .pointer(
                "/menuServiceItemRenderer/serviceEndpoint/queueAddEndpoint/queueTarget/videoId",
            )
            .and_then(|v| v.as_str())
        {
            return Some(video_id.to_owned());
        }
    }
    None
}

fn track_from_parts(
    video_id: &str,
    title: &str,
    artists: &[String],
    thumbnail: Option<String>,
) -> Track {
    Track {
        identity: TrackIdentity::youtube(video_id),
        title: title.to_owned(),
        artists: artists.to_vec(),
        album: None,
        duration_secs: None,
        thumbnail,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

fn artist_from_parts(browse_id: &str, name: &str, thumbnail: Option<String>) -> Artist {
    Artist {
        id: ArtistId::new(browse_id),
        name: name.to_owned(),
        subscribers: None,
        thumbnail,
        description: None,
    }
}

fn subtitle_artists(renderer: &serde_json::Value) -> Vec<String> {
    let Ok(runs) = ptr(renderer, "/subtitle/runs") else {
        return Vec::new();
    };
    let Some(list) = runs.as_array() else {
        return Vec::new();
    };
    runs_artists(list)
}

fn subtitle_linked_artists(renderer: &serde_json::Value) -> Vec<String> {
    let Ok(runs) = ptr(renderer, "/subtitle/runs") else {
        return Vec::new();
    };
    let Some(list) = runs.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter(|run| run.get("navigationEndpoint").is_some())
        .filter_map(|run| run.pointer("/text").and_then(|v| v.as_str()))
        .map(str::to_owned)
        .collect()
}

fn runs_artists(runs: &[serde_json::Value]) -> Vec<String> {
    runs.iter()
        .take_while(|run| run.get("text").and_then(|v| v.as_str()) != Some(" • "))
        .step_by(2)
        .filter_map(|run| run.get("text").and_then(|v| v.as_str()))
        .map(str::to_owned)
        .collect()
}

fn list_artists(renderer: &serde_json::Value) -> Vec<String> {
    let Ok(column) = ptr(
        renderer,
        "/flexColumns/1/musicResponsiveListItemFlexColumnRenderer/text/runs",
    ) else {
        return Vec::new();
    };
    let Some(list) = column.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter(|run| run.get("navigationEndpoint").is_some())
        .filter_map(|run| run.pointer("/text").and_then(|v| v.as_str()))
        .map(str::to_owned)
        .collect()
}

fn subtitle_year(renderer: &serde_json::Value) -> Option<String> {
    let runs = ptr(renderer, "/subtitle/runs").ok()?.as_array()?;
    let first = runs.first()?.get("text")?.as_str()?;
    if first.chars().all(|c| c.is_ascii_digit()) && first.len() == 4 {
        return Some(first.to_owned());
    }
    let second = runs.get(2)?.get("text")?.as_str()?;
    if second.chars().all(|c| c.is_ascii_digit()) && second.len() == 4 {
        return Some(second.to_owned());
    }
    None
}

fn overlay_playlist_id(renderer: &serde_json::Value) -> Option<String> {
    ptr(
        renderer,
        "/thumbnailOverlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchPlaylistEndpoint/playlistId",
    )
    .ok()?
    .as_str()
    .map(str::to_owned)
}

fn thumbs_two_row(renderer: &serde_json::Value) -> Option<String> {
    ptr(
        renderer,
        "/thumbnailRenderer/musicThumbnailRenderer/thumbnail/thumbnails",
    )
    .ok()
    .and_then(|v| v.as_array())
    .and_then(|v| v.last())
    .and_then(|v| v.get("url"))
    .and_then(|v| v.as_str())
    .map(str::to_owned)
}

fn thumbs_list(renderer: &serde_json::Value) -> Option<String> {
    ptr(
        renderer,
        "/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails",
    )
    .ok()
    .and_then(|v| v.as_array())
    .and_then(|v| v.last())
    .and_then(|v| v.get("url"))
    .and_then(|v| v.as_str())
    .map(str::to_owned)
}

pub(crate) fn ptr<'a>(
    value: &'a serde_json::Value,
    pointer: &str,
) -> Result<&'a serde_json::Value, Error> {
    value
        .pointer(pointer)
        .ok_or_else(|| Error::Fixture(format!("missing {pointer}")))
}

pub(crate) fn text(value: &serde_json::Value) -> Result<&str, Error> {
    value
        .pointer("/runs/0/text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Fixture("missing runs/0/text".to_owned()))
}

fn io_error(e: Error) -> ytmapi_rs::Error {
    ytmapi_rs::Error::from(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        e.to_string(),
    ))
}
