use std::borrow::Cow;

use madrigal_core::{Album, Artist, ArtistId, Playlist, Track};
use ytmapi_rs::auth::AuthToken;
use ytmapi_rs::json::Json;
use ytmapi_rs::parse::{ParseFrom, ProcessedResult};
use ytmapi_rs::query::{PostMethod, PostQuery, Query};

use crate::browse::{
    mtrir_album, mtrir_playlist, mtrir_track, ptr, section_from_value, BrowsePage,
};
use crate::error::Error;
use crate::CatalogClient;

pub struct GetChartsQuery {
    country: String,
}

impl GetChartsQuery {
    pub fn new(country: &str) -> Self {
        Self {
            country: country.to_owned(),
        }
    }
}

impl<A: AuthToken> Query<A> for GetChartsQuery {
    type Output = ChartsPage;
    type Method = PostMethod;
}

impl PostQuery for GetChartsQuery {
    fn header(&self) -> serde_json::Map<String, serde_json::Value> {
        serde_json::Map::from_iter([(
            "browseId".to_owned(),
            serde_json::Value::String("FEmusic_charts".to_owned()),
        ), (
            "formData".to_owned(),
            serde_json::json!({"selectedValues": [self.country]}),
        )])
    }

    fn params(&self) -> Vec<(&str, Cow<'_, str>)> {
        vec![]
    }

    fn path(&self) -> &str {
        "browse"
    }
}

pub struct GetNewReleasesQuery;

impl<A: AuthToken> Query<A> for GetNewReleasesQuery {
    type Output = NewReleases;
    type Method = PostMethod;
}

impl PostQuery for GetNewReleasesQuery {
    fn header(&self) -> serde_json::Map<String, serde_json::Value> {
        serde_json::Map::from_iter([(
            "browseId".to_owned(),
            serde_json::Value::String("FEmusic_explore".to_owned()),
        )])
    }

    fn params(&self) -> Vec<(&str, Cow<'_, str>)> {
        vec![]
    }

    fn path(&self) -> &str {
        "browse"
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChartTrend {
    Up,
    Down,
    Neutral,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChartArtist {
    pub artist: Artist,
    pub rank: Option<String>,
    pub trend: Option<ChartTrend>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChartsPage {
    pub country: String,
    pub videos: Vec<Playlist>,
    pub artists: Vec<ChartArtist>,
    pub genres: Vec<Playlist>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NewReleases {
    pub albums: Vec<Album>,
    pub videos: Vec<Track>,
}

impl ParseFrom<GetChartsQuery> for ChartsPage {
    fn parse_from(p: ProcessedResult<GetChartsQuery>) -> ytmapi_rs::Result<Self> {
        let country = p.query.country.clone();
        let value = p.json.into_inner();
        let mut page = page_from_value(&value).map_err(io_error)?;
        page.country = country;
        Ok(page)
    }
}

impl ParseFrom<GetNewReleasesQuery> for NewReleases {
    fn parse_from(p: ProcessedResult<GetNewReleasesQuery>) -> ytmapi_rs::Result<Self> {
        let value = p.json.into_inner();
        releases_from_value(&value).map_err(io_error)
    }
}

impl CatalogClient {
    pub async fn charts(&self, country: &str) -> Result<ChartsPage, Error> {
        let value = self.raw_json(GetChartsQuery::new(country)).await?;
        Self::parse_charts(&value.to_string())
    }

    pub async fn charts_page(&self, country: &str) -> Result<BrowsePage, Error> {
        let value = self.raw_json(GetChartsQuery::new(country)).await?;
        Self::parse_charts_page(&value.to_string())
    }

    pub fn parse_charts(source: &str) -> Result<ChartsPage, Error> {
        let query = GetChartsQuery::new("");
        offline_parse(&query, source)
    }

    pub fn parse_charts_page(source: &str) -> Result<BrowsePage, Error> {
        let json: Json = serde_json::from_str(source)?;
        let value = json.into_inner();
        let list = ptr(
            &value,
            "/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents",
        )?
        .as_array()
        .ok_or_else(|| Error::Fixture("charts contents is not an array".to_owned()))?;
        let mut sections = Vec::new();
        for section in list {
            if section.get("musicShelfRenderer").is_some() {
                continue;
            }
            if let Some(parsed) = section_from_value(section)? {
                sections.push(parsed);
            }
        }
        Ok(BrowsePage { sections })
    }

    pub async fn new_releases(&self) -> Result<NewReleases, Error> {
        let value = self.raw_json(GetNewReleasesQuery).await?;
        Self::parse_new_releases(&value.to_string())
    }

    pub fn parse_new_releases(source: &str) -> Result<NewReleases, Error> {
        offline_parse(&GetNewReleasesQuery, source)
    }
}

fn offline_parse<Q, O>(query: &Q, source: &str) -> Result<O, Error>
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

fn page_from_value(value: &serde_json::Value) -> Result<ChartsPage, Error> {
    let sections = ptr(
        value,
        "/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents",
    )?;
    let list = sections
        .as_array()
        .ok_or_else(|| Error::Fixture("charts contents is not an array".to_owned()))?;
    let mut buckets: Vec<Vec<Playlist>> = Vec::new();
    let mut artists: Vec<ChartArtist> = Vec::new();
    for section in list.iter().skip(1) {
        let Ok(carousel) = ptr(section, "/musicCarouselShelfRenderer") else {
            continue;
        };
        let Ok(contents) = ptr(carousel, "/contents") else {
            continue;
        };
        let Some(entries) = contents.as_array() else {
            continue;
        };
        let Some(first) = entries.first() else {
            continue;
        };
        if is_chart_playlist(first) {
            let mut bucket = Vec::with_capacity(entries.len());
            for entry in entries {
                bucket.push(mtrir_playlist(ptr(entry, "/musicTwoRowItemRenderer")?)?);
            }
            buckets.push(bucket);
        } else if first.get("musicResponsiveListItemRenderer").is_some() && artists.is_empty() {
            for entry in entries {
                artists.push(chart_artist(ptr(
                    entry,
                    "/musicResponsiveListItemRenderer",
                )?)?);
            }
        }
    }
    let mut buckets = buckets.into_iter();
    let videos = buckets.next().unwrap_or_default();
    let genres: Vec<Playlist> = buckets.flatten().collect();
    Ok(ChartsPage {
        country: String::new(),
        videos,
        artists,
        genres,
    })
}

fn is_chart_playlist(first: &serde_json::Value) -> bool {
    let Ok(renderer) = ptr(first, "/musicTwoRowItemRenderer") else {
        return false;
    };
    let Ok(browse_id) = ptr(
        renderer,
        "/title/runs/0/navigationEndpoint/browseEndpoint/browseId",
    ) else {
        return false;
    };
    browse_id.as_str().is_some_and(|id| id.starts_with("VL"))
}

fn chart_artist(renderer: &serde_json::Value) -> Result<ChartArtist, Error> {
    let head = ptr(
        renderer,
        "/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text/runs/0",
    )?;
    let name = head
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Fixture("missing chart artist title".to_owned()))?;
    let browse_id = ptr(renderer, "/navigationEndpoint/browseEndpoint/browseId")?
        .as_str()
        .ok_or_else(|| Error::Fixture("chart artist id is not a string".to_owned()))?;
    let subscribers = ptr(
        renderer,
        "/flexColumns/1/musicResponsiveListItemFlexColumnRenderer/text/runs/0/text",
    )
    .ok()
    .and_then(|v| v.as_str())
    .and_then(|s| s.split_whitespace().next())
    .map(str::to_owned);
    let thumbnail = ptr(
        renderer,
        "/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails",
    )
    .ok()
    .and_then(|v| v.as_array())
    .and_then(|v| v.last())
    .and_then(|v| v.get("url"))
    .and_then(|v| v.as_str())
    .map(str::to_owned);
    let rank = ptr(
        renderer,
        "/customIndexColumn/musicCustomIndexColumnRenderer/text/runs/0/text",
    )
    .ok()
    .and_then(|v| v.as_str())
    .map(str::to_owned);
    let trend = ptr(
        renderer,
        "/customIndexColumn/musicCustomIndexColumnRenderer/icon/iconType",
    )
    .ok()
    .and_then(|v| v.as_str())
    .and_then(|icon| match icon {
        "ARROW_DROP_UP" => Some(ChartTrend::Up),
        "ARROW_DROP_DOWN" => Some(ChartTrend::Down),
        "ARROW_CHART_NEUTRAL" => Some(ChartTrend::Neutral),
        _ => None,
    });
    Ok(ChartArtist {
        artist: Artist {
            id: ArtistId::new(browse_id),
            name: name.to_owned(),
            subscribers,
            thumbnail,
            description: None,
        },
        rank,
        trend,
    })
}

fn releases_from_value(value: &serde_json::Value) -> Result<NewReleases, Error> {
    let sections = ptr(
        value,
        "/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents",
    )?;
    let list = sections
        .as_array()
        .ok_or_else(|| Error::Fixture("explore contents is not an array".to_owned()))?;
    let mut albums = Vec::new();
    let mut videos = Vec::new();
    for section in list {
        let Ok(carousel) = ptr(section, "/musicCarouselShelfRenderer") else {
            continue;
        };
        let tab = ptr(
            carousel,
            "/header/musicCarouselShelfBasicHeaderRenderer/title/runs/0/navigationEndpoint/browseEndpoint/browseId",
        )
        .ok()
        .and_then(|v| v.as_str());
        let Ok(contents) = ptr(carousel, "/contents") else {
            continue;
        };
        let Some(entries) = contents.as_array() else {
            continue;
        };
        match tab {
            Some("FEmusic_new_releases_albums") => {
                for entry in entries {
                    if entry.get("musicTwoRowItemRenderer").is_some() {
                        albums.push(mtrir_album(ptr(entry, "/musicTwoRowItemRenderer")?)?);
                    }
                }
            }
            Some("FEmusic_new_releases_videos") => {
                for entry in entries {
                    if entry.get("musicTwoRowItemRenderer").is_some() {
                        videos.push(mtrir_track(ptr(entry, "/musicTwoRowItemRenderer")?)?);
                    }
                }
            }
            _ => continue,
        }
    }
    if albums.is_empty() && videos.is_empty() {
        return Err(Error::Fixture("new releases tabs missing".to_owned()));
    }
    Ok(NewReleases { albums, videos })
}

fn io_error(e: Error) -> ytmapi_rs::Error {
    ytmapi_rs::Error::from(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        e.to_string(),
    ))
}
