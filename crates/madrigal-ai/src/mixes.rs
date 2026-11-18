use serde::{Deserialize, Serialize};

use madrigal_library::Library;
use madrigal_settings::TopRange;

use crate::providers::{ChatModel, ChatRequest};
use crate::Error;

pub const TOP_MIX_COUNT: usize = 5;
pub const TOP_MIX_TRACK_LIMIT: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestedTrack {
    pub title: String,
    pub artist: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedTrack {
    pub title: String,
    pub artists: Vec<String>,
    pub plays: u64,
}

#[derive(Debug, Clone, Default)]
pub struct MixSeeds {
    pub top_tracks: Vec<SeedTrack>,
    pub top_artists: Vec<(String, u64)>,
    pub top_albums: Vec<(String, u64)>,
}

pub fn collect_mix_seeds(
    library: &Library<'_>,
    range: TopRange,
    limit: usize,
) -> Result<MixSeeds, madrigal_library::Error> {
    let top_tracks = library
        .top_tracks(range, limit)?
        .into_iter()
        .map(|entry| SeedTrack {
            title: entry.track.title,
            artists: entry.track.artists,
            plays: entry.plays,
        })
        .collect();
    let top_artists = library
        .top_artists(range, limit)?
        .into_iter()
        .map(|entry| (entry.name, entry.plays))
        .collect();
    let top_albums = library
        .top_albums(range, limit)?
        .into_iter()
        .map(|entry| (entry.name, entry.plays))
        .collect();
    Ok(MixSeeds {
        top_tracks,
        top_artists,
        top_albums,
    })
}

fn seed_block(seeds: &MixSeeds, limit: usize) -> String {
    let mut block = String::from("Top tracks (title - artists, plays):\n");
    for seed in seeds.top_tracks.iter().take(limit) {
        block.push_str(&format!(
            "- {} - {}, {}\n",
            seed.title,
            seed.artists.join(", "),
            seed.plays
        ));
    }
    block.push_str("Top artists (name, plays):\n");
    for (name, plays) in seeds.top_artists.iter().take(limit) {
        block.push_str(&format!("- {name}, {plays}\n"));
    }
    block.push_str("Top albums (name, plays):\n");
    for (name, plays) in seeds.top_albums.iter().take(limit) {
        block.push_str(&format!("- {name}, {plays}\n"));
    }
    block
}

pub fn top_mix_prompt(seeds: &MixSeeds, count: usize) -> ChatRequest {
    let system = format!(
        "You are a music curator. Analyze the provided listening stats, inferring genres, \
         styles, eras, and moods from artists, titles, and album names. Recommend up to \
         {count} tracks that fit this taste, favoring transition flow and genre coherence \
         over covering every seed. You may include tracks from the stats and tracks by \
         similar artists of the same genres. Return only a numbered list with one track \
         per line in the form 'Song Title - Artist Name'. No commentary, no markdown, \
         no extra lines."
    );
    let user = format!(
        "Listening stats:\n{}Recommend up to {count} tracks.",
        seed_block(seeds, 20)
    );
    ChatRequest::new(&system, &user, 0.35, 4096)
}

pub fn suggestion_prompt(title: &str, seeds: &[SeedTrack], count: usize) -> ChatRequest {
    let system = format!(
        "You are a music recommender. Given a playlist title and a few of its tracks, \
         suggest up to {count} tracks that belong on the playlist. Return only a numbered \
         list with one track per line in the form 'Song Title - Artist Name'. No \
         commentary, no markdown, no extra lines."
    );
    let mut user = format!("Playlist title: {}\nPlaylist tracks:\n", title.trim());
    for seed in seeds.iter().take(10) {
        user.push_str(&format!("- {} - {}\n", seed.title, seed.artists.join(", ")));
    }
    user.push_str(&format!("Suggest up to {count} tracks."));
    ChatRequest::new(&system, &user, 0.35, 2048)
}

fn strip_prefix(line: &str) -> &str {
    let mut rest = line;
    if rest.starts_with(|b: char| b.is_ascii_digit()) {
        rest = rest.trim_start_matches(|b: char| b.is_ascii_digit());
        rest = rest.strip_prefix(['.', ')', ':']).unwrap_or(rest).trim_start();
    } else if let Some(stripped) = rest.strip_prefix(['-', '*', '>', '+']) {
        rest = stripped.trim_start();
    } else if let Some(stripped) = rest.strip_prefix('•') {
        rest = stripped.trim_start();
    }
    rest
}

fn split_title_artist(line: &str) -> Option<(String, String)> {
    for separator in [" - ", " – ", " — ", " | "] {
        if let Some((title, artist)) = line.split_once(separator) {
            return Some((title.to_owned(), artist.to_owned()));
        }
    }
    None
}

fn clean_field(value: &str) -> String {
    value
        .trim()
        .trim_matches(['"', '\'', '`', '*'])
        .trim()
        .to_owned()
}

pub fn parse_track_list(raw: &str) -> Result<Vec<SuggestedTrack>, Error> {
    let mut tracks = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("```") {
            continue;
        }
        let entry = strip_prefix(line);
        let Some((title, artist)) = split_title_artist(entry) else {
            continue;
        };
        let title = clean_field(&title);
        let artist = clean_field(&artist);
        if title.is_empty() || artist.is_empty() {
            continue;
        }
        if tracks.iter().any(|known: &SuggestedTrack| {
            known.title.eq_ignore_ascii_case(&title) && known.artist.eq_ignore_ascii_case(&artist)
        }) {
            continue;
        }
        tracks.push(SuggestedTrack { title, artist });
    }
    if tracks.is_empty() {
        return Err(Error::Unparseable(
            "no track names parsed from ai response",
        ));
    }
    Ok(tracks)
}

pub fn request_top_mix(
    model: &dyn ChatModel,
    seeds: &MixSeeds,
    count: usize,
) -> Result<Vec<SuggestedTrack>, Error> {
    if seeds.top_tracks.is_empty() && seeds.top_artists.is_empty() {
        return Err(Error::NotConfigured("no listening history for top mix"));
    }
    let request = top_mix_prompt(seeds, count);
    let raw = model.complete(&request)?;
    let mut tracks = parse_track_list(&raw)?;
    tracks.truncate(count.max(1));
    Ok(tracks)
}

pub fn request_suggestions(
    model: &dyn ChatModel,
    title: &str,
    seeds: &[SeedTrack],
    count: usize,
) -> Result<Vec<SuggestedTrack>, Error> {
    if title.trim().is_empty() && seeds.is_empty() {
        return Err(Error::NotConfigured("no playlist context for suggestions"));
    }
    let request = suggestion_prompt(title, seeds, count);
    let raw = model.complete(&request)?;
    let mut tracks = parse_track_list(&raw)?;
    tracks.truncate(count.max(1));
    Ok(tracks)
}
