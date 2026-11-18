use std::collections::HashSet;
use std::sync::OnceLock;

use madrigal_core::Track;
use regex::Regex;

use crate::models::{SpotifyImage, SpotifyTrack};

pub const MIN_MATCH_THRESHOLD: f64 = 0.35;
pub const EARLY_EXIT_THRESHOLD: f64 = 0.95;

fn feat_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\(feat.*?\)").unwrap_or_else(|_| Regex::new("$^").unwrap()))
}

fn ft_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\(ft.*?\)").unwrap_or_else(|_| Regex::new("$^").unwrap()))
}

fn bracket_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\[.*?]").unwrap_or_else(|_| Regex::new("$^").unwrap()))
}

fn remaster_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\(.*?remaster.*?\)").unwrap_or_else(|_| Regex::new("$^").unwrap())
    })
}

fn remix_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\(.*?remix.*?\)").unwrap_or_else(|_| Regex::new("$^").unwrap())
    })
}

fn non_alnum_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"[^a-z0-9\s]").unwrap_or_else(|_| Regex::new("$^").unwrap()))
}

fn multi_space_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\s+").unwrap_or_else(|_| Regex::new("$^").unwrap()))
}

pub fn normalize_title(title: &str) -> String {
    let lowered = title.to_lowercase();
    let stripped = feat_pattern().replace_all(&lowered, "");
    let stripped = ft_pattern().replace_all(&stripped, "");
    let stripped = bracket_pattern().replace_all(&stripped, "");
    let stripped = remaster_pattern().replace_all(&stripped, "");
    let stripped = remix_pattern().replace_all(&stripped, "");
    let stripped = non_alnum_pattern().replace_all(&stripped, "");
    multi_space_pattern()
        .replace_all(&stripped, " ")
        .trim()
        .to_owned()
}

fn bigrams(text: &str) -> HashSet<String> {
    if text.len() < 2 {
        return HashSet::new();
    }
    text.as_bytes()
        .windows(2)
        .map(|pair| String::from_utf8_lossy(pair).into_owned())
        .collect()
}

fn dice(a: &str, bigrams_a: &HashSet<String>, b: &str, bigrams_b: &HashSet<String>) -> f64 {
    if a == b {
        return 1.0;
    }
    if bigrams_a.is_empty() || bigrams_b.is_empty() {
        return 0.0;
    }
    let intersection = bigrams_a.iter().filter(|gram| bigrams_b.contains(*gram)).count();
    2.0 * intersection as f64 / (bigrams_a.len() + bigrams_b.len()) as f64
}

pub fn duration_score(spotify_duration_ms: i64, candidate_duration_secs: Option<u32>) -> f64 {
    let candidate = match candidate_duration_secs {
        Some(seconds) => seconds as i64,
        None => return 0.5,
    };
    if spotify_duration_ms <= 0 {
        return 0.5;
    }
    let diff = (spotify_duration_ms / 1000 - candidate).abs();
    match diff {
        0..=2 => 1.0,
        3..=5 => 0.8,
        6..=10 => 0.5,
        11..=30 => 0.2,
        _ => 0.0,
    }
}

pub fn match_score(
    spotify_title: &str,
    spotify_artist: &str,
    spotify_duration_ms: i64,
    candidate_title: &str,
    candidate_artist: &str,
    candidate_duration_secs: Option<u32>,
) -> f64 {
    let norm_spotify_title = normalize_title(spotify_title);
    let norm_candidate_title = normalize_title(candidate_title);
    let title = dice(
        &norm_spotify_title,
        &bigrams(&norm_spotify_title),
        &norm_candidate_title,
        &bigrams(&norm_candidate_title),
    );
    let norm_spotify_artist = normalize_title(spotify_artist);
    let norm_candidate_artist = normalize_title(candidate_artist);
    let artist = dice(
        &norm_spotify_artist,
        &bigrams(&norm_spotify_artist),
        &norm_candidate_artist,
        &bigrams(&norm_candidate_artist),
    );
    title * 0.45 + artist * 0.35 + duration_score(spotify_duration_ms, candidate_duration_secs) * 0.20
}

pub fn build_query(track: &SpotifyTrack) -> String {
    match track.artists.first() {
        Some(artist) if !artist.name.is_empty() => format!("{} {}", artist.name, track.name),
        _ => track.name.clone(),
    }
}

pub fn thumbnail_url(images: &[SpotifyImage]) -> Option<String> {
    images
        .iter()
        .find(|image| matches!(image.width, Some(width) if (200..=400).contains(&width)))
        .or_else(|| images.first())
        .map(|image| image.url.clone())
}

pub fn playlist_thumbnail_url(images: &[SpotifyImage]) -> Option<String> {
    thumbnail_url(images)
}

pub fn pick_best(track: &SpotifyTrack, candidates: &[Track]) -> Option<(usize, f64)> {
    let (best, _) = score_candidates(track, candidates);
    best.filter(|(_, score)| *score >= MIN_MATCH_THRESHOLD)
}

pub fn top_score(track: &SpotifyTrack, candidates: &[Track]) -> f64 {
    score_candidates(track, candidates).1
}

fn score_candidates(track: &SpotifyTrack, candidates: &[Track]) -> (Option<(usize, f64)>, f64) {
    let artist = track.artist_text();
    let mut best: Option<(usize, f64)> = None;
    let mut seen = 0.0;
    for (index, candidate) in candidates.iter().enumerate() {
        if candidate.identity.id.is_empty() {
            continue;
        }
        let score = match_score(
            &track.name,
            &artist,
            track.duration_ms,
            &candidate.title,
            &candidate.artist_text(),
            candidate.duration_secs,
        );
        if score > seen {
            seen = score;
        }
        if best.is_none_or(|(_, current)| score > current) {
            best = Some((index, score));
        }
        if score >= EARLY_EXIT_THRESHOLD {
            break;
        }
    }
    (best, seen)
}

pub fn apply_resolution(spotify: &SpotifyTrack, candidate: &Track) -> Track {
    Track {
        identity: candidate.identity.clone(),
        title: candidate.title.clone(),
        artists: candidate.artists.clone(),
        album: spotify
            .album
            .as_ref()
            .map(|album| album.name.clone())
            .or_else(|| candidate.album.clone()),
        duration_secs: if spotify.duration_ms > 0 {
            u32::try_from(spotify.duration_ms / 1000).ok()
        } else {
            candidate.duration_secs
        },
        thumbnail: spotify
            .album
            .as_ref()
            .and_then(|album| thumbnail_url(&album.images))
            .or_else(|| candidate.thumbnail.clone()),
        track_number: candidate.track_number,
        local_path: None,
        set_video_id: candidate.set_video_id.clone(),
    }
}
