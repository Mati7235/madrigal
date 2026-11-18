use ytmapi_rs::auth::AuthToken;
use ytmapi_rs::parse::ParseFrom;
use ytmapi_rs::query::{PostMethod, PostQuery, Query};

use madrigal_core::{Album, Artist, Playlist, Track};

use crate::error::Error;
use crate::CatalogClient;

#[derive(Clone)]
pub struct GetHomeQuery;

impl<A: AuthToken> Query<A> for GetHomeQuery {
    type Output = HomeFeed;
    type Method = PostMethod;
}

impl PostQuery for GetHomeQuery {
    fn header(&self) -> serde_json::Map<String, serde_json::Value> {
        serde_json::Map::from_iter([(
            "browseId".to_string(),
            serde_json::json!("FEmusic_home"),
        )])
    }

    fn params(&self) -> Vec<(&str, std::borrow::Cow<'_, str>)> {
        Vec::new()
    }

    fn path(&self) -> &str {
        "browse"
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HomeItem {
    Track(Track),
    Album(Album),
    Playlist(Playlist),
    Artist(Artist),
}

#[derive(Debug, Clone, PartialEq)]
pub struct HomeSection {
    pub title: String,
    pub items: Vec<HomeItem>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct HomeFeed {
    pub sections: Vec<HomeSection>,
}

impl HomeFeed {
    pub fn from_value(value: &serde_json::Value) -> Result<Self, Error> {
        let contents = pointer(
            value,
            "/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents",
        )?;
        let items = contents
            .as_array()
            .ok_or_else(|| Error::Fixture("home contents is not an array".to_owned()))?;
        let mut sections = Vec::with_capacity(items.len());
        for item in items {
            if let Some(section) = parse_section(item)? {
                if !section.items.is_empty() {
                    sections.push(section);
                }
            }
        }
        Ok(Self { sections })
    }
}

impl ParseFrom<GetHomeQuery> for HomeFeed {
    fn parse_from(
        p: ytmapi_rs::ProcessedResult<GetHomeQuery>,
    ) -> ytmapi_rs::Result<Self> {
        Self::from_value(&p.json.into_inner()).map_err(io_error)
    }
}

fn parse_section(item: &serde_json::Value) -> Result<Option<HomeSection>, Error> {
    if let Some(shelf) = item.pointer("/musicCarouselShelfRenderer") {
        let title = shelf_text(shelf)?;
        let mut section = HomeSection {
            title,
            items: Vec::new(),
        };
        let contents = shelf
            .pointer("/contents")
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::Fixture("home shelf contents is not an array".to_owned()))?;
        for entry in contents {
            if let Some(home_item) = parse_entry(entry)? {
                section.items.push(home_item);
            }
        }
        return Ok(Some(section));
    }
    if let Some(shelf) = item.pointer("/musicCardShelfRenderer") {
        let title = shelf_text(shelf)?;
        let mut section = HomeSection {
            title,
            items: Vec::new(),
        };
        let contents = shelf
            .pointer("/contents")
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::Fixture("home card shelf contents is not an array".to_owned()))?;
        for entry in contents {
            if let Some(home_item) = parse_card(entry)? {
                section.items.push(home_item);
            }
        }
        return Ok(Some(section));
    }
    Ok(None)
}

fn shelf_text(shelf: &serde_json::Value) -> Result<String, Error> {
    run_text(shelf.pointer("/header/musicCarouselShelfBasicHeaderRenderer/title").or_else(|| {
        shelf.pointer("/header/musicCardShelfHeaderBasicRenderer/title")
    }).ok_or_else(|| Error::Fixture("home shelf has no title".to_owned()))?)
    .map(str::to_owned)
}

fn parse_entry(entry: &serde_json::Value) -> Result<Option<HomeItem>, Error> {
    if let Some(renderer) = entry.pointer("/musicTwoRowItemRenderer") {
        return parse_two_row(renderer).map(Some);
    }
    if let Some(renderer) = entry.pointer("/musicResponsiveListItemRenderer") {
        return parse_list_item(renderer).map(Some);
    }
    Ok(None)
}

fn parse_card(entry: &serde_json::Value) -> Result<Option<HomeItem>, Error> {
    if let Some(renderer) = entry.pointer("/musicResponsiveListItemRenderer") {
        return parse_list_item(renderer).map(Some);
    }
    if let Some(renderer) = entry.pointer("/musicTwoRowItemRenderer") {
        return parse_two_row(renderer).map(Some);
    }
    Ok(None)
}

fn parse_two_row(renderer: &serde_json::Value) -> Result<HomeItem, Error> {
    let title = run_text(pointer(renderer, "/title")?).map(str::to_owned)?;
    let subtitle = runs_text(renderer.pointer("/subtitle").unwrap_or(&serde_json::Value::Null));
    let thumbnail = renderer
        .pointer("/thumbnailRenderer/musicThumbnailRenderer")
        .and_then(thumb);
    let endpoint = renderer.pointer("/navigationEndpoint");
    if let Some(video) = endpoint
        .and_then(|e| e.pointer("/watchEndpoint/videoId"))
        .and_then(|v| v.as_str())
    {
        return Ok(HomeItem::Track(track_item(video, &title, &subtitle, thumbnail)));
    }
    let page = endpoint
        .and_then(|e| {
            e.pointer(
                "/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType",
            )
        })
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let browse_id = endpoint
        .and_then(|e| e.pointer("/browseEndpoint/browseId"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Fixture("home two-row item has no endpoint".to_owned()))?;
    Ok(match page {
        "MUSIC_PAGE_TYPE_ALBUM" => HomeItem::Album(album_item(browse_id, &title, &subtitle, thumbnail)),
        "MUSIC_PAGE_TYPE_ARTIST" => HomeItem::Artist(artist_item(browse_id, &title, thumbnail)),
        _ => HomeItem::Playlist(playlist_item(browse_id, &title, &subtitle, thumbnail)),
    })
}

fn runs_text(value: &serde_json::Value) -> String {
    value
        .pointer("/runs")
        .and_then(|v| v.as_array())
        .map(|runs| {
            runs.iter()
                .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
                .collect::<String>()
        })
        .unwrap_or_default()
}

fn parse_list_item(renderer: &serde_json::Value) -> Result<HomeItem, Error> {
    let columns = renderer
        .pointer("/flexColumns")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Fixture("home list item has no flexColumns".to_owned()))?;
    let title = columns
        .first()
        .and_then(|c| {
            c.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs/0/text")
                .and_then(|v| v.as_str())
        })
        .ok_or_else(|| Error::Fixture("home list item has no title".to_owned()))?;
    let video_id = renderer
        .pointer("/playlistItemData/videoId")
        .and_then(|v| v.as_str())
        .or_else(|| {
            columns.first().and_then(|c| {
                c.pointer(
                    "/musicResponsiveListItemFlexColumnRenderer/text/runs/0/navigationEndpoint/watchEndpoint/videoId",
                )
                .and_then(|v| v.as_str())
            })
        })
        .or_else(|| {
            renderer
                .pointer("/navigationEndpoint/watchEndpoint/videoId")
                .and_then(|v| v.as_str())
        });
    let subtitle = columns
        .get(1)
        .map(|c| {
            runs_text(
                c.pointer("/musicResponsiveListItemFlexColumnRenderer/text")
                    .unwrap_or(&serde_json::Value::Null),
            )
        })
        .unwrap_or_default();
    let album = columns.get(2).and_then(|c| {
        c.pointer(
            "/musicResponsiveListItemFlexColumnRenderer/text/runs/0/text",
        )
        .and_then(|v| v.as_str())
    });
    let thumbnail = renderer
        .pointer("/thumbnail/musicThumbnailRenderer")
        .and_then(thumb);
    match video_id {
        Some(video) => Ok(HomeItem::Track(track_item_album(
            video,
            title,
            &subtitle,
            album,
            thumbnail,
        ))),
        None => Err(Error::Fixture(
            "home list item has no videoId".to_owned(),
        )),
    }
}

fn track_item(
    video_id: &str,
    title: &str,
    subtitle: &str,
    thumbnail: Option<String>,
) -> Track {
    track_item_album(video_id, title, subtitle, None, thumbnail)
}

fn track_item_album(
    video_id: &str,
    title: &str,
    subtitle: &str,
    album: Option<&str>,
    thumbnail: Option<String>,
) -> Track {
    use madrigal_core::TrackIdentity;
    let artists: Vec<String> = subtitle
        .split(" • ")
        .next()
        .unwrap_or(subtitle)
        .split(", ")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    Track {
        identity: TrackIdentity::youtube(video_id),
        title: title.to_owned(),
        artists,
        album: album.map(str::to_owned),
        duration_secs: None,
        thumbnail,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

fn album_item(id: &str, title: &str, subtitle: &str, thumbnail: Option<String>) -> Album {
    use madrigal_core::AlbumId;
    Album {
        id: AlbumId::new(id),
        title: title.to_owned(),
        artists: vec![subtitle.to_owned()],
        artist_id: None,
        year: None,
        thumbnail,
        track_count: None,
        audio_playlist_id: None,
    }
}

fn playlist_item(id: &str, title: &str, subtitle: &str, thumbnail: Option<String>) -> Playlist {
    use madrigal_core::{PlaylistId, PlaylistKind};
    let author = subtitle
        .split(" • ")
        .next()
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    Playlist {
        id: PlaylistId::new(id),
        kind: PlaylistKind::Remote,
        title: title.to_owned(),
        author,
        description: None,
        thumbnail,
        track_count: None,
        owned: false,
        privacy: None,
        author_id: None,
    }
}

fn artist_item(id: &str, name: &str, thumbnail: Option<String>) -> Artist {
    use madrigal_core::ArtistId;
    Artist {
        id: ArtistId::new(id),
        name: name.to_owned(),
        subscribers: None,
        thumbnail,
        description: None,
    }
}

fn thumb(renderer: &serde_json::Value) -> Option<String> {
    renderer
        .pointer("/thumbnail/thumbnails")
        .and_then(|v| v.as_array())
        .and_then(|t| t.last())
        .and_then(|t| t.get("url"))
        .and_then(|u| u.as_str())
        .map(str::to_owned)
}

fn pointer<'a>(value: &'a serde_json::Value, ptr: &str) -> Result<&'a serde_json::Value, Error> {
    value
        .pointer(ptr)
        .ok_or_else(|| Error::Fixture(format!("missing {ptr}")))
}

fn run_text(value: &serde_json::Value) -> Result<&str, Error> {
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

impl CatalogClient {
    pub async fn get_home(&self) -> Result<HomeFeed, Error> {
        let value = self.raw_json(GetHomeQuery).await?;
        HomeFeed::from_value(&value)
    }

    pub fn parse_home(source: &str) -> Result<HomeFeed, Error> {
        let value: serde_json::Value = serde_json::from_str(source)?;
        HomeFeed::from_value(&value)
    }
}
