use std::fs;
use std::path::{Path, PathBuf};

use madrigal_core::{Track, TrackIdentity};

use crate::artwork;
use crate::tags;

pub const SUPPORTED_EXTENSIONS: [&str; 5] = ["mp3", "flac", "m4a", "ogg", "wav"];

pub struct ScanConfig {
    pub include_dirs: Vec<PathBuf>,
    pub exclude_dirs: Vec<PathBuf>,
    pub min_duration_secs: u32,
    pub artwork_cache_limit_bytes: u64,
    pub artist_separators: String,
}

impl ScanConfig {
    pub fn from_settings(settings: &madrigal_settings::Settings) -> Self {
        let includes = settings
            .storage
            .local_songs_included_folders
            .iter()
            .map(PathBuf::from)
            .collect();
        let excludes = settings
            .storage
            .local_songs_excluded_folders
            .iter()
            .map(PathBuf::from)
            .collect();
        Self {
            include_dirs: includes,
            exclude_dirs: excludes,
            min_duration_secs: settings.content.local_songs_min_duration_secs.max(0) as u32,
            artwork_cache_limit_bytes: (settings.storage.max_image_cache_mb.max(0) as u64)
                * 1024
                * 1024,
            artist_separators: settings.content.artist_separators.clone(),
        }
    }
}

pub struct ScanResult {
    pub track: Track,
    pub artwork_path: Option<PathBuf>,
    pub lyrics: Option<String>,
}

pub struct ScanError {
    pub path: PathBuf,
    pub message: String,
}

pub struct ScanOutcome {
    pub results: Vec<ScanResult>,
    pub errors: Vec<ScanError>,
    pub skipped_excluded: usize,
    pub skipped_short: usize,
}

fn absolutize(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    }
}

fn canonical_or_absolute(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| absolutize(path))
}

fn is_under(path: &Path, prefix: &Path) -> bool {
    path.starts_with(prefix)
}

fn split_artists(raw: &str, separators: &str) -> Vec<String> {
    if separators.is_empty() {
        let value = raw.trim().to_owned();
        return if value.is_empty() {
            Vec::new()
        } else {
            vec![value]
        };
    }
    raw.split(|c: char| separators.contains(c))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn stem_title(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_owned()
}

fn track_for(path: &Path, tags: &tags::FileTags, separators: &str) -> Track {
    let id = path.to_string_lossy().into_owned();
    let artists = tags
        .artists
        .iter()
        .flat_map(|raw| split_artists(raw, separators))
        .collect::<Vec<_>>();
    Track {
        identity: TrackIdentity::local(&id),
        title: tags.title.clone().unwrap_or_else(|| stem_title(path)),
        artists,
        album: tags.album.clone(),
        duration_secs: tags.duration_secs,
        thumbnail: None,
        track_number: tags.track_number,
        local_path: Some(id),
        set_video_id: None,
    }
}

fn collect_files(root: &Path, out: &mut Vec<PathBuf>, errors: &mut Vec<ScanError>) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                errors.push(ScanError {
                    path: dir,
                    message: e.to_string(),
                });
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    errors.push(ScanError {
                        path: dir.clone(),
                        message: e.to_string(),
                    });
                    continue;
                }
            };
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(e) => {
                    errors.push(ScanError {
                        path: entry.path(),
                        message: e.to_string(),
                    });
                    continue;
                }
            };
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() {
                out.push(entry.path());
            }
        }
    }
}

fn supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SUPPORTED_EXTENSIONS.contains(&e.to_lowercase().as_str()))
}

pub fn scan(config: &ScanConfig) -> ScanOutcome {
    let mut outcome = ScanOutcome {
        results: Vec::new(),
        errors: Vec::new(),
        skipped_excluded: 0,
        skipped_short: 0,
    };
    let excludes: Vec<PathBuf> = config
        .exclude_dirs
        .iter()
        .map(|raw| canonical_or_absolute(&absolutize(raw)))
        .collect();
    for include in &config.include_dirs {
        let root = canonical_or_absolute(&absolutize(include));
        if !root.is_dir() {
            outcome.errors.push(ScanError {
                path: include.clone(),
                message: "include dir is missing or not a directory".to_owned(),
            });
            continue;
        }
        let mut files = Vec::new();
        collect_files(&root, &mut files, &mut outcome.errors);
        files.sort();
        for file in files {
            if !supported(&file) {
                continue;
            }
            let canonical = canonical_or_absolute(&file);
            if excludes.iter().any(|x| is_under(&canonical, x)) {
                outcome.skipped_excluded += 1;
                continue;
            }
            let parsed = match tags::parse_file(&file) {
                Ok(parsed) => parsed,
                Err(message) => {
                    outcome.errors.push(ScanError {
                        path: file,
                        message,
                    });
                    continue;
                }
            };
            if parsed.duration_secs.unwrap_or(0) < config.min_duration_secs {
                outcome.skipped_short += 1;
                continue;
            }
            let mut result = ScanResult {
                track: track_for(&canonical, &parsed, &config.artist_separators),
                artwork_path: None,
                lyrics: parsed.lyrics,
            };
            if let Some(art) = &parsed.artwork {
                match artwork::store_artwork(
                    &art.data,
                    &art.mime,
                    config.artwork_cache_limit_bytes,
                ) {
                    Ok(stored) => {
                        result.artwork_path = Some(stored.clone());
                        result.track.thumbnail =
                            Some(stored.to_string_lossy().into_owned());
                    }
                    Err(e) => outcome.errors.push(ScanError {
                        path: file,
                        message: format!("artwork store failed: {e}"),
                    }),
                }
            }
            outcome.results.push(result);
        }
    }
    outcome
}

pub fn sync_to_db(
    conn: &rusqlite::Connection,
    results: &[ScanResult],
) -> Result<usize, madrigal_db::Error> {
    for result in results {
        madrigal_db::upsert_song(conn, &result.track)?;
    }
    Ok(results.len())
}
