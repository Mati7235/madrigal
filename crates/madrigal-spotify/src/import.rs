use madrigal_core::Track;

use crate::cache::{lookup_mapping, store_mapping};
use crate::mapper::{apply_resolution, build_query, pick_best, top_score};
use crate::models::{SpotifyPlaylist, SpotifyTrack};
use crate::Error;

pub trait SpotifySource {
    fn playlist_detail(&self, playlist_id: &str) -> Result<SpotifyPlaylist, Error>;
    fn playlist_tracks(&self, playlist_id: &str) -> Result<Vec<SpotifyTrack>, Error>;
}

pub trait Search {
    fn search_songs(&self, query: &str) -> Result<Vec<Track>, Error>;
}

#[derive(Debug, Clone)]
pub struct UnresolvedTrack {
    pub track: SpotifyTrack,
    pub query: String,
    pub best_score: f64,
}

#[derive(Debug, Clone)]
pub struct ImportOutcome {
    pub playlist: SpotifyPlaylist,
    pub resolved: Vec<Track>,
    pub unresolved: Vec<UnresolvedTrack>,
}

pub fn import_playlist<S: SpotifySource, Q: Search>(
    source: &S,
    search: &Q,
    conn: &rusqlite::Connection,
    playlist_id: &str,
    show_in_library: bool,
) -> Result<ImportOutcome, Error> {
    if !show_in_library {
        return Err(Error::Disabled);
    }
    if playlist_id.trim().is_empty() {
        return Err(Error::Protocol("playlist id required".to_owned()));
    }
    let playlist = source.playlist_detail(playlist_id)?;
    let mut resolved = Vec::new();
    let mut unresolved = Vec::new();
    for track in source
        .playlist_tracks(playlist_id)?
        .into_iter()
        .filter(|track| track.is_valid())
    {
        if let Some(cached) = lookup_mapping(conn, &track.id)? {
            resolved.push(cached);
            continue;
        }
        let query = build_query(&track);
        let candidates = match search.search_songs(&query) {
            Ok(candidates) => candidates,
            Err(_) => {
                unresolved.push(UnresolvedTrack {
                    query: query.clone(),
                    best_score: 0.0,
                    track,
                });
                continue;
            }
        };
        match pick_best(&track, &candidates) {
            Some((index, _)) => {
                let merged = apply_resolution(&track, &candidates[index]);
                store_mapping(conn, &track.id, &merged)?;
                resolved.push(merged);
            }
            None => unresolved.push(UnresolvedTrack {
                query: query.clone(),
                best_score: top_score(&track, &candidates),
                track,
            }),
        }
    }
    Ok(ImportOutcome {
        playlist,
        resolved,
        unresolved,
    })
}
