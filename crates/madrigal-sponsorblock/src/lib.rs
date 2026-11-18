use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::client::Client;

pub mod client;

pub const HASH_PREFIX_LENGTH: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Category {
    #[serde(rename = "sponsor")]
    Sponsor,
    #[serde(rename = "selfpromo")]
    SelfPromo,
    #[serde(rename = "interaction")]
    Interaction,
    #[serde(rename = "intro")]
    Intro,
    #[serde(rename = "outro")]
    Outro,
    #[serde(rename = "preview")]
    Preview,
    #[serde(rename = "music_offtopic")]
    MusicOfftopic,
    #[serde(rename = "filler")]
    Filler,
    #[serde(rename = "hook")]
    Hook,
}

impl Category {
    pub const ALL: [Category; 9] = [
        Category::Sponsor,
        Category::SelfPromo,
        Category::Interaction,
        Category::Intro,
        Category::Outro,
        Category::Preview,
        Category::MusicOfftopic,
        Category::Filler,
        Category::Hook,
    ];

    pub const DEFAULT: [Category; 5] = [
        Category::Sponsor,
        Category::SelfPromo,
        Category::Intro,
        Category::Outro,
        Category::MusicOfftopic,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Category::Sponsor => "sponsor",
            Category::SelfPromo => "selfpromo",
            Category::Interaction => "interaction",
            Category::Intro => "intro",
            Category::Outro => "outro",
            Category::Preview => "preview",
            Category::MusicOfftopic => "music_offtopic",
            Category::Filler => "filler",
            Category::Hook => "hook",
        }
    }

    pub fn parse(value: &str) -> Option<Category> {
        Category::ALL
            .iter()
            .copied()
            .find(|category| category.as_str() == value)
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::Sponsor => "Sponsor",
            Category::SelfPromo => "Self-promotion",
            Category::Interaction => "Interaction reminder",
            Category::Intro => "Intro",
            Category::Outro => "Outro",
            Category::Preview => "Preview",
            Category::MusicOfftopic => "Non-music section",
            Category::Filler => "Filler",
            Category::Hook => "Hook",
        }
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<madrigal_settings::SponsorCategory> for Category {
    fn from(value: madrigal_settings::SponsorCategory) -> Category {
        use madrigal_settings::SponsorCategory as Stored;
        match value {
            Stored::Sponsor => Category::Sponsor,
            Stored::SelfPromo => Category::SelfPromo,
            Stored::Interaction => Category::Interaction,
            Stored::Intro => Category::Intro,
            Stored::Outro => Category::Outro,
            Stored::Preview => Category::Preview,
            Stored::MusicOfftopic => Category::MusicOfftopic,
            Stored::Filler => Category::Filler,
            Stored::Hook => Category::Hook,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Skip,
    Mute,
    Full,
}

impl Action {
    pub fn parse(value: &str) -> Option<Action> {
        match value {
            "skip" => Some(Action::Skip),
            "mute" => Some(Action::Mute),
            "full" => Some(Action::Full),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Action::Skip => "skip",
            Action::Mute => "mute",
            Action::Full => "full",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub category: Category,
    pub action: Action,
    pub uuid: String,
    pub votes: i64,
}

impl Segment {
    pub fn duration(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }

    pub fn label(&self) -> &'static str {
        self.category.label()
    }

    pub fn mutes(&self) -> bool {
        self.action == Action::Mute
    }
}

fn parse_segment(raw: &serde_json::Value) -> Option<Segment> {
    let span = raw.get("segment")?.as_array()?;
    if span.len() != 2 {
        return None;
    }
    let start = span.first()?.as_f64()?;
    let end = span.get(1)?.as_f64()?;
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    if !(0.0 <= start && start < end) {
        return None;
    }
    let category = Category::parse(raw.get("category")?.as_str()?)?;
    let action = Action::parse(raw.get("actionType")?.as_str()?)?;
    let uuid = raw
        .get("UUID")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_owned();
    let votes = raw
        .get("votes")
        .and_then(|value| value.as_i64())
        .unwrap_or(0);
    Some(Segment {
        start,
        end,
        category,
        action,
        uuid,
        votes,
    })
}

pub fn parse(payload: &serde_json::Value, video_id: &str) -> Vec<Segment> {
    let entries = match payload.as_array() {
        Some(entries) => entries,
        None => return Vec::new(),
    };
    for entry in entries {
        let matches = entry
            .get("videoID")
            .and_then(|value| value.as_str())
            .is_some_and(|id| id == video_id);
        if !matches {
            continue;
        }
        let raws = entry
            .get("segments")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let mut segments: Vec<Segment> =
            raws.iter().filter_map(parse_segment).collect();
        segments.sort_by(|a, b| {
            a.start
                .partial_cmp(&b.start)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        return segments;
    }
    Vec::new()
}

pub fn active_segment(segments: &[Segment], t_ms: i64) -> Option<&Segment> {
    let position = t_ms.max(0) as f64 / 1000.0;
    segments
        .iter()
        .find(|segment| segment.start <= position && position < segment.end)
}

#[derive(Debug, Default)]
pub struct SkipTracker {
    skipped: HashSet<String>,
}

impl SkipTracker {
    pub fn new() -> SkipTracker {
        SkipTracker::default()
    }

    pub fn reset(&mut self) {
        self.skipped.clear();
    }

    pub fn check(&mut self, segments: &[Segment], t_ms: i64) -> Option<Segment> {
        let segment = active_segment(segments, t_ms)?.clone();
        if self.skipped.contains(&segment.uuid) {
            return None;
        }
        self.skipped.insert(segment.uuid.clone());
        Some(segment)
    }
}

pub fn client_for(
    integrations: &madrigal_settings::Integrations,
) -> Client {
    Client::from_integrations(integrations)
}
