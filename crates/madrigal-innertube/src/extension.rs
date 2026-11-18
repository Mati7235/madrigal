use ytmapi_rs::parse::ParseFrom;
use ytmapi_rs::query::GetMoodCategoriesQuery;

use crate::error::Error;
use crate::pages::{MoodEntry, MoodListing, MoodSection};

impl ParseFrom<GetMoodCategoriesQuery> for MoodListing {
    fn parse_from(
        p: ytmapi_rs::ProcessedResult<GetMoodCategoriesQuery>,
    ) -> ytmapi_rs::Result<Self> {
        let value = p.json.into_inner();
        listing_from_value(&value).map_err(io_error)
    }
}

fn listing_from_value(value: &serde_json::Value) -> Result<MoodListing, Error> {
    let contents = pointer(
        value,
        "/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents",
    )?;
    let items = contents
        .as_array()
        .ok_or_else(|| Error::Fixture("mood contents is not an array".to_owned()))?;
    let mut sections = Vec::with_capacity(items.len());
    for item in items {
        let grid = pointer(item, "/gridRenderer")?;
        let name = run_text(pointer(grid, "/header/gridHeaderRenderer/title")?)?.to_owned();
        let entries_raw = pointer(grid, "/items")?;
        let entries_list = entries_raw
            .as_array()
            .ok_or_else(|| Error::Fixture("mood items is not an array".to_owned()))?;
        let mut entries = Vec::with_capacity(entries_list.len());
        for entry in entries_list {
            let button = pointer(entry, "/musicNavigationButtonRenderer")?;
            let title = run_text(pointer(button, "/buttonText")?)?.to_owned();
            let params = pointer(button, "/clickCommand/browseEndpoint/params")?
                .as_str()
                .ok_or_else(|| Error::Fixture("mood params is not a string".to_owned()))?
                .to_owned();
            let thumbnail = pointer(button, "/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails")
                .ok()
                .and_then(|t| t.as_array())
                .and_then(|t| t.last())
                .and_then(|t| t.get("url"))
                .and_then(|u| u.as_str())
                .map(str::to_owned);
            entries.push(MoodEntry {
                title,
                params,
                thumbnail,
            });
        }
        sections.push(MoodSection { name, entries });
    }
    Ok(MoodListing { sections })
}

fn io_error(e: Error) -> ytmapi_rs::Error {
    ytmapi_rs::Error::from(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        e.to_string(),
    ))
}

fn pointer<'a>(value: &'a serde_json::Value, ptr: &str) -> Result<&'a serde_json::Value, Error> {    value
        .pointer(ptr)
        .ok_or_else(|| Error::Fixture(format!("missing {ptr}")))
}

fn run_text(value: &serde_json::Value) -> Result<&str, Error> {    value
        .pointer("/runs/0/text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Fixture("missing runs/0/text".to_owned()))
}
