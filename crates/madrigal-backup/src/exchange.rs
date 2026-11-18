use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD};

pub const JSON_FORMAT: &str = "madrigal-playlist";
pub const JSON_VERSION: u32 = 1;
pub const CSV_COLUMNS: [&str; 5] = ["position", "title", "artists", "videoId", "localPath"];
pub const ARTIST_SEPARATOR: &str = "; ";

pub const MAX_TRACKS: usize = 5000;
pub const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_COVER_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_NAME_LENGTH: usize = 512;
pub const MAX_DESCRIPTION_LENGTH: usize = 4096;
pub const MAX_TITLE_LENGTH: usize = 1024;
pub const MAX_ALBUM_LENGTH: usize = 1024;
pub const MAX_ARTIST_LENGTH: usize = 512;
pub const MAX_VIDEO_ID_LENGTH: usize = 128;
pub const MAX_PATH_LENGTH: usize = 4096;

const ALLOWED_COVER_MIME: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/gif",
    "image/bmp",
];

#[derive(Debug)]
pub enum ExchangeError {
    Format(String),
    Limit(String),
    Io(io::Error),
}

impl std::fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExchangeError::Format(msg) => write!(f, "playlist format error: {msg}"),
            ExchangeError::Limit(msg) => write!(f, "playlist limit exceeded: {msg}"),
            ExchangeError::Io(err) => write!(f, "playlist io error: {err}"),
        }
    }
}

impl std::error::Error for ExchangeError {}

impl From<io::Error> for ExchangeError {
    fn from(err: io::Error) -> Self {
        ExchangeError::Io(err)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExchangeIssue {
    pub code: String,
    pub message: String,
    pub severity: Severity,
    pub position: Option<usize>,
}

impl ExchangeIssue {
    fn error(code: &str, message: String, position: Option<usize>) -> Self {
        Self {
            code: code.to_owned(),
            message,
            severity: Severity::Error,
            position,
        }
    }

    fn warning(code: &str, message: String, position: Option<usize>) -> Self {
        Self {
            code: code.to_owned(),
            message,
            severity: Severity::Warning,
            position,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TrackEntry {
    pub position: usize,
    pub title: String,
    pub artists: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
}

impl TrackEntry {
    pub fn artist_text(&self) -> String {
        self.artists.join(ARTIST_SEPARATOR)
    }

    pub fn identity(&self) -> Option<String> {
        self.video_id
            .as_ref()
            .map(|id| format!("video:{id}"))
            .or_else(|| self.local_path.as_ref().map(|path| format!("local:{path}")))
    }

    pub fn as_dict(&self) -> serde_json::Value {
        serde_json::json!({
            "position": self.position,
            "videoId": self.video_id,
            "title": self.title,
            "artists": self.artists,
            "album": self.album,
            "duration": self.duration,
            "thumbnail": self.thumbnail,
            "localPath": self.local_path,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoverSummary {
    pub name: Option<String>,
    pub mime: Option<String>,
    pub bytes: Option<u64>,
    pub has_data: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PlaylistMeta {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub cover: Option<CoverSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExchangeReport {
    pub kind: String,
    pub valid: bool,
    pub playlist: PlaylistMeta,
    pub track_count: usize,
    pub issues: Vec<ExchangeIssue>,
    pub entries: Vec<TrackEntry>,
}

impl ExchangeReport {
    pub fn errors(&self) -> Vec<&ExchangeIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect()
    }

    pub fn warnings(&self) -> Vec<&ExchangeIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportDiff {
    pub adds: Vec<TrackEntry>,
    pub updates: Vec<TrackEntry>,
    pub skipped: Vec<TrackEntry>,
}

pub fn diff_against(entries: &[TrackEntry], existing: &HashSet<String>) -> ImportDiff {
    let mut diff = ImportDiff::default();
    for entry in entries {
        match entry.identity() {
            None => diff.skipped.push(entry.clone()),
            Some(key) if existing.contains(&key) => diff.updates.push(entry.clone()),
            Some(_) => diff.adds.push(entry.clone()),
        }
    }
    diff
}

fn lookup<'a>(value: &'a serde_json::Value, keys: &[&str]) -> Option<&'a serde_json::Value> {
    let map = value.as_object()?;
    keys.iter().find_map(|key| {
        map.get(*key).filter(|v| !v.is_null())
    })
}

fn clean_text(value: Option<&serde_json::Value>) -> String {
    match value {
        None | Some(serde_json::Value::Null) => String::new(),
        Some(serde_json::Value::String(s)) => s.trim().to_owned(),
        Some(other) => other
            .as_str()
            .unwrap_or(&other.to_string())
            .trim()
            .to_owned(),
    }
}

fn opt_text(value: Option<&serde_json::Value>) -> Option<String> {
    let text = clean_text(value);
    if text.is_empty() { None } else { Some(text) }
}

fn as_int(value: Option<&serde_json::Value>) -> Option<i64> {
    match value {
        Some(serde_json::Value::Number(n)) => n.as_i64(),
        Some(serde_json::Value::String(s)) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn as_artists(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(serde_json::Value::String(s)) => {
            let text = s.trim();
            if text.is_empty() {
                return Vec::new();
            }
            let sep = if text.contains(ARTIST_SEPARATOR) {
                ARTIST_SEPARATOR
            } else {
                ", "
            };
            if text.contains(sep) {
                text.split(sep)
                    .map(|p| p.trim().to_owned())
                    .filter(|p| !p.is_empty())
                    .collect()
            } else {
                vec![text.to_owned()]
            }
        }
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect(),
        Some(other) => {
            let text = other.as_str().unwrap_or("").trim().to_owned();
            if text.is_empty() { Vec::new() } else { vec![text] }
        }
    }
}

fn clean_local_path(value: Option<&serde_json::Value>) -> Option<String> {
    let text = clean_text(value);
    if text.is_empty() {
        return None;
    }
    let path = Path::new(&text);
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(text)
}

pub fn entry_from_value(source: &serde_json::Value, position: usize) -> TrackEntry {
    let declared = as_int(lookup(source, &["position", "index", "order", "orden"]));
    TrackEntry {
        position: declared
            .filter(|v| *v >= 0)
            .map(|v| v as usize)
            .unwrap_or(position),
        title: clean_text(lookup(source, &["title", "name", "titulo"])),
        artists: as_artists(lookup(source, &["artists", "artist", "artist_text", "artistas"])),
        video_id: opt_text(lookup(
            source,
            &["video_id", "videoId", "videoid", "track_id", "trackId"],
        )),
        local_path: clean_local_path(lookup(
            source,
            &["local_path", "localPath", "localpath", "path", "ruta"],
        )),
        album: opt_text(lookup(source, &["album", "album_name", "albumName"])),
        duration: as_int(lookup(source, &["duration", "length", "duracion"]))
            .filter(|v| *v >= 0)
            .map(|v| v as u32),
        thumbnail: opt_text(lookup(source, &["thumbnail", "art", "artwork"])),
    }
}

pub fn normalize_entries(entries: Vec<TrackEntry>) -> Vec<TrackEntry> {
    let mut sorted = entries;
    sorted.sort_by_key(|e| e.position);
    sorted
}

pub fn renumber_entries(entries: Vec<TrackEntry>) -> Vec<TrackEntry> {
    let mut sorted = normalize_entries(entries);
    for (index, entry) in sorted.iter_mut().enumerate() {
        entry.position = index;
    }
    sorted
}

fn playlist_block(value: &serde_json::Value) -> &serde_json::Value {
    if let Some(inner) = value.get("playlist") {
        if inner.is_object() {
            return inner;
        }
    }
    value
}

pub fn playlist_metadata(value: Option<&serde_json::Value>) -> PlaylistMeta {
    match value {
        None => PlaylistMeta {
            id: None,
            name: String::new(),
            description: None,
            cover: None,
        },
        Some(raw) => {
            let block = playlist_block(raw);
            let name = clean_text(lookup(block, &["name", "title", "nombre"]));
            let description = opt_text(lookup(block, &["description", "descripcion", "desc"]));
            PlaylistMeta {
                id: lookup(block, &["id", "playlistId", "playlist_id", "identity"])
                    .and_then(|v| v.as_str().map(str::to_owned)),
                name,
                description,
                cover: None,
            }
        }
    }
}

fn cover_mime_for_name(name: &str) -> String {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
    .to_owned()
}

fn coerce_cover_bytes(value: &serde_json::Value) -> Result<Vec<u8>, ExchangeError> {
    match value {
        serde_json::Value::String(s) => STANDARD
            .decode(s.as_bytes())
            .map_err(|err| ExchangeError::Format(format!("cover data invalid: {err}"))),
        _ => Err(ExchangeError::Format("cover data must be base64".to_owned())),
    }
}

fn encode_cover_bytes(data: Vec<u8>, name: String, mime: String) -> Result<serde_json::Value, ExchangeError> {
    if data.len() > MAX_COVER_BYTES {
        return Err(ExchangeError::Limit(format!(
            "cover exceeds {MAX_COVER_BYTES} bytes"
        )));
    }
    Ok(serde_json::json!({
        "name": name,
        "mime": mime,
        "bytes": data.len(),
        "data": STANDARD.encode(&data),
    }))
}

pub fn encode_cover_value(cover: &serde_json::Value) -> Result<Option<serde_json::Value>, ExchangeError> {
    if cover.is_null() {
        return Ok(None);
    }
    if let Some(path_raw) = lookup(cover, &["path", "file", "cover_path", "coverPath"]) {
        if let Some(path_str) = path_raw.as_str() {
            return encode_cover_file(Path::new(path_str)).map(Some);
        }
    }
    if let Some(data) = lookup(cover, &["data", "bytes", "content"]) {
        let raw = coerce_cover_bytes(data)?;
        let name = clean_text(lookup(cover, &["name", "filename"]));
        let name = if name.is_empty() { "cover".to_owned() } else { name };
        let mime = clean_text(lookup(cover, &["mime", "mimeType", "type"]));
        let mime = if mime.is_empty() {
            cover_mime_for_name(&name)
        } else {
            mime
        };
        return encode_cover_bytes(raw, name, mime).map(Some);
    }
    if let Some(path_str) = cover.as_str() {
        return encode_cover_file(Path::new(path_str)).map(Some);
    }
    Ok(None)
}

pub fn encode_cover_file(path: &Path) -> Result<serde_json::Value, ExchangeError> {
    let bytes = std::fs::read(path)?;
    if bytes.len() > MAX_COVER_BYTES {
        return Err(ExchangeError::Limit(format!(
            "cover exceeds {MAX_COVER_BYTES} bytes"
        )));
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "cover".to_owned());
    encode_cover_bytes(bytes, name.clone(), cover_mime_for_name(&name))
}

fn guard_track_count(count: usize) -> Result<(), ExchangeError> {
    if count > MAX_TRACKS {
        return Err(ExchangeError::Limit(format!("more than {MAX_TRACKS} tracks")));
    }
    Ok(())
}

fn cover_source(playlist: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    let raw = playlist?;
    let block = playlist_block(raw);
    lookup(block, &["cover", "coverPath", "cover_path", "coverUrl", "portada"]).cloned()
}

pub fn build_json_payload(
    playlist: Option<&serde_json::Value>,
    entries: &[TrackEntry],
    cover: Option<&serde_json::Value>,
) -> Result<serde_json::Value, ExchangeError> {
    let meta = playlist_metadata(playlist);
    let ordered = renumber_entries(entries.to_vec());
    guard_track_count(ordered.len())?;
    if meta.name.len() > MAX_NAME_LENGTH {
        return Err(ExchangeError::Limit("name too long".to_owned()));
    }
    if meta.description.as_ref().is_some_and(|d| d.len() > MAX_DESCRIPTION_LENGTH) {
        return Err(ExchangeError::Limit("description too long".to_owned()));
    }
    let source = cover.cloned().or_else(|| cover_source(playlist));
    let encoded = source
        .as_ref()
        .map(encode_cover_value)
        .transpose()?
        .flatten();
    Ok(serde_json::json!({
        "format": JSON_FORMAT,
        "version": JSON_VERSION,
        "playlist": {
            "id": meta.id,
            "name": meta.name,
            "description": meta.description,
            "trackCount": ordered.len(),
            "cover": encoded,
        },
        "tracks": ordered.iter().map(TrackEntry::as_dict).collect::<Vec<_>>(),
    }))
}

pub fn serialize_json(
    playlist: Option<&serde_json::Value>,
    entries: &[TrackEntry],
    cover: Option<&serde_json::Value>,
) -> Result<String, ExchangeError> {
    let mut text = serde_json::to_string_pretty(&build_json_payload(playlist, entries, cover)?)
        .map_err(|err| ExchangeError::Format(err.to_string()))?;
    text.push('\n');
    Ok(text)
}

fn csv_needs_quote(cell: &str) -> bool {
    cell.contains([',', '"', '\n', '\r'])
}

fn csv_escape(cell: &str) -> String {
    if csv_needs_quote(cell) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_owned()
    }
}

fn csv_cell(value: &str) -> String {
    let guarded = match value.chars().next() {
        Some('=' | '+' | '-' | '@' | '\t' | '\r') => format!("'{value}"),
        _ => value.to_owned(),
    };
    csv_escape(&guarded)
}

fn csv_uncell(value: &str) -> String {
    if value.len() >= 2
        && value.starts_with('\'')
        && matches!(
            value.chars().nth(1),
            Some('=' | '+' | '-' | '@' | '\t' | '\r')
        ) {
        return value[1..].to_owned();
    }
    value.to_owned()
}

pub fn serialize_csv(entries: &[TrackEntry]) -> Result<String, ExchangeError> {
    let ordered = renumber_entries(entries.to_vec());
    guard_track_count(ordered.len())?;
    let mut out = String::new();
    out.push_str(&CSV_COLUMNS.join(","));
    out.push('\n');
    for entry in &ordered {
        let row = [
            entry.position.to_string(),
            csv_cell(&entry.title),
            csv_cell(&entry.artist_text()),
            csv_cell(entry.video_id.as_deref().unwrap_or("")),
            csv_cell(entry.local_path.as_deref().unwrap_or("")),
        ];
        out.push_str(&row.join(","));
        out.push('\n');
    }
    Ok(out)
}

pub fn export_json(
    playlist: Option<&serde_json::Value>,
    entries: &[TrackEntry],
    path: &Path,
    cover: Option<&serde_json::Value>,
) -> Result<PathBuf, ExchangeError> {
    let text = serialize_json(playlist, entries, cover)?;
    Ok(madrigal_files::atomic_write_text(path, &text)?)
}

pub fn export_csv(entries: &[TrackEntry], path: &Path) -> Result<PathBuf, ExchangeError> {
    Ok(madrigal_files::atomic_write_text(
        path,
        &serialize_csv(entries)?,
    )?)
}

pub fn export_playlist(
    playlist: Option<&serde_json::Value>,
    entries: &[TrackEntry],
    path: &Path,
    cover: Option<&serde_json::Value>,
) -> Result<PathBuf, ExchangeError> {
    let is_csv = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("csv"));
    if is_csv {
        export_csv(entries, path)
    } else {
        export_json(playlist, entries, path, cover)
    }
}

pub fn parse_json(text: &str) -> Result<serde_json::Value, ExchangeError> {
    let data: serde_json::Value = serde_json::from_str(text)
        .map_err(|err| ExchangeError::Format(format!("invalid JSON: {err}")))?;
    if !data.is_object() {
        return Err(ExchangeError::Format("JSON must be an object".to_owned()));
    }
    Ok(data)
}

fn parse_csv_rows(text: &str) -> Result<Vec<Vec<String>>, ExchangeError> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    let mut has_data = false;
    while let Some(c) = chars.next() {
        has_data = true;
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                cell.push(c);
            }
        } else if c == '"' && cell.is_empty() {
            in_quotes = true;
        } else if c == ',' {
            row.push(std::mem::take(&mut cell));
        } else if c == '\n' {
            row.push(std::mem::take(&mut cell));
            rows.push(std::mem::take(&mut row));
        } else if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut cell));
            rows.push(std::mem::take(&mut row));
        } else {
            cell.push(c);
        }
    }
    if in_quotes {
        return Err(ExchangeError::Format("unterminated CSV quote".to_owned()));
    }
    if has_data {
        row.push(cell);
        rows.push(row);
    }
    Ok(rows)
}

fn csv_key(value: &str) -> String {
    value
        .replace('\u{feff}', "")
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '-', '_'], "")
}

fn csv_header(row: &[String]) -> Option<HashMap<String, usize>> {
    let mut columns: HashMap<String, usize> = HashMap::new();
    for (index, cell) in row.iter().enumerate() {
        let key = csv_key(cell);
        if !key.is_empty() {
            columns.entry(key).or_insert(index);
        }
    }
    let aliases: &[(&str, &[&str])] = &[
        ("position", &["position", "posicion", "pos", "orden", "order", "index"]),
        ("title", &["title", "titulo", "nombre", "name"]),
        ("artists", &["artists", "artistas", "artist", "artista"]),
        ("videoid", &["videoid", "video", "id"]),
        ("localpath", &["localpath", "path", "ruta", "ruta-local"]),
    ];
    let mut resolved = HashMap::new();
    for (canonical, names) in aliases {
        for name in *names {
            if let Some(index) = columns.get(*name) {
                resolved.insert((*canonical).to_owned(), *index);
                break;
            }
        }
    }
    if resolved.is_empty() { None } else { Some(resolved) }
}

pub fn parse_csv(text: &str) -> Result<Vec<TrackEntry>, ExchangeError> {
    let rows = parse_csv_rows(text)?;
    let rows: Vec<Vec<String>> = rows
        .into_iter()
        .filter(|row| row.iter().any(|cell| !cell.trim().is_empty()))
        .collect();
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let header = csv_header(&rows[0])
        .ok_or_else(|| ExchangeError::Format("unrecognized CSV header".to_owned()))?;
    let mut entries = Vec::new();
    for (index, row) in rows.iter().skip(1).enumerate() {
        let get = |key: &str| {
            header
                .get(key)
                .map(|i| row.get(*i).map(|s| s.trim()).unwrap_or(""))
                .unwrap_or("")
        };
        let declared = get("position").parse::<i64>().ok();
        entries.push(TrackEntry {
            position: declared
                .filter(|v| *v >= 0)
                .map(|v| v as usize)
                .unwrap_or(index),
            title: csv_uncell(get("title")).trim().to_owned(),
            artists: as_artists(Some(&serde_json::Value::String(csv_uncell(get("artists"))))),
            video_id: {
                let text = csv_uncell(get("videoid")).trim().to_owned();
                if text.is_empty() { None } else { Some(text) }
            },
            local_path: {
                let text = csv_uncell(get("localpath")).trim().to_owned();
                clean_local_path(Some(&serde_json::Value::String(text)))
            },
            album: None,
            duration: None,
            thumbnail: None,
        });
    }
    Ok(normalize_entries(entries))
}

fn read_limited_text(path: &Path) -> Result<String, ExchangeError> {
    if path.is_symlink() {
        return Err(ExchangeError::Limit("symlink rejected".to_owned()));
    }
    let meta = std::fs::metadata(path)?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(ExchangeError::Limit(format!(
            "file exceeds {MAX_FILE_BYTES} bytes"
        )));
    }
    let bytes = std::fs::read(path)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(ExchangeError::Limit(format!(
            "file exceeds {MAX_FILE_BYTES} bytes"
        )));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| ExchangeError::Format("file is not valid UTF-8".to_owned()))?;
    Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned())
}

pub fn load_json(path: &Path) -> Result<serde_json::Value, ExchangeError> {
    parse_json(&read_limited_text(path)?)
}

pub fn load_csv(path: &Path) -> Result<Vec<TrackEntry>, ExchangeError> {
    parse_csv(&read_limited_text(path)?)
}

fn validate_entries(
    entries: &[TrackEntry],
    issues: &mut Vec<ExchangeIssue>,
    check_local_paths: bool,
) {
    let mut seen_video: HashMap<&str, usize> = HashMap::new();
    let mut seen_local: HashMap<&str, usize> = HashMap::new();
    let mut seen_position: HashSet<usize> = HashSet::new();
    for entry in entries {
        if entry.title.is_empty() {
            issues.push(ExchangeIssue::error(
                "missing_title",
                "track without title".to_owned(),
                Some(entry.position),
            ));
        } else if entry.title.len() > MAX_TITLE_LENGTH {
            issues.push(ExchangeIssue::error(
                "title_too_long",
                "title too long".to_owned(),
                Some(entry.position),
            ));
        }
        if entry.album.as_ref().is_some_and(|a| a.len() > MAX_ALBUM_LENGTH) {
            issues.push(ExchangeIssue::error(
                "album_too_long",
                "album too long".to_owned(),
                Some(entry.position),
            ));
        }
        if entry.artists.len() > 64
            || entry.artists.iter().any(|a| a.len() > MAX_ARTIST_LENGTH)
        {
            issues.push(ExchangeIssue::error(
                "artist_too_long",
                "artist too long".to_owned(),
                Some(entry.position),
            ));
        }
        if entry.video_id.as_ref().is_some_and(|v| v.len() > MAX_VIDEO_ID_LENGTH) {
            issues.push(ExchangeIssue::error(
                "video_id_too_long",
                "videoId too long".to_owned(),
                Some(entry.position),
            ));
        }
        if entry.local_path.as_ref().is_some_and(|p| p.len() > MAX_PATH_LENGTH) {
            issues.push(ExchangeIssue::error(
                "local_path_too_long",
                "local path too long".to_owned(),
                Some(entry.position),
            ));
        }
        if entry.video_id.is_none() && entry.local_path.is_none() {
            issues.push(ExchangeIssue::error(
                "missing_identity",
                "no videoId or local path; cannot resolve by title".to_owned(),
                Some(entry.position),
            ));
        }
        if let Some(video) = &entry.video_id {
            if seen_video.insert(video.as_str(), entry.position).is_some() {
                issues.push(ExchangeIssue::error(
                    "duplicate_video",
                    format!("duplicate videoId: {video}"),
                    Some(entry.position),
                ));
            }
        }
        if let Some(local) = &entry.local_path {
            if seen_local.insert(local.as_str(), entry.position).is_some() {
                issues.push(ExchangeIssue::error(
                    "duplicate_local_path",
                    format!("duplicate local path: {local}"),
                    Some(entry.position),
                ));
            } else if check_local_paths && !Path::new(local).exists() {
                issues.push(ExchangeIssue::warning(
                    "missing_local_path",
                    format!("missing local path: {local}"),
                    Some(entry.position),
                ));
            }
        }
        if !seen_position.insert(entry.position) {
            issues.push(ExchangeIssue::warning(
                "duplicate_position",
                format!("duplicate position: {}", entry.position),
                Some(entry.position),
            ));
        }
    }
}

fn validate_cover_value(cover: &serde_json::Value, issues: &mut Vec<ExchangeIssue>) {
    if cover.is_null() {
        return;
    }
    let map = match cover.as_object() {
        Some(map) => map,
        None => {
            issues.push(ExchangeIssue::error(
                "invalid_cover",
                "cover with invalid shape".to_owned(),
                None,
            ));
            return;
        }
    };
    let data = match map.get("data").and_then(|v| v.as_str()) {
        Some(data) if !data.is_empty() => data,
        _ => {
            issues.push(ExchangeIssue::error(
                "invalid_cover",
                "cover without data".to_owned(),
                None,
            ));
            return;
        }
    };
    let raw = match STANDARD.decode(data.as_bytes()) {
        Ok(raw) => raw,
        Err(_) => {
            issues.push(ExchangeIssue::error(
                "invalid_cover",
                "cover data not decodable".to_owned(),
                None,
            ));
            return;
        }
    };
    if raw.len() > MAX_COVER_BYTES {
        issues.push(ExchangeIssue::error(
            "cover_too_large",
            format!("cover exceeds {MAX_COVER_BYTES} bytes"),
            None,
        ));
    }
    if let Some(declared) = map.get("bytes").and_then(|v| as_int(Some(v))) {
        if declared != raw.len() as i64 {
            issues.push(ExchangeIssue::warning(
                "cover_size_mismatch",
                "declared cover size mismatch".to_owned(),
                None,
            ));
        }
    }
    if let Some(mime) = map.get("mime").and_then(|v| v.as_str()) {
        let mime = mime.trim();
        if !mime.is_empty() && !ALLOWED_COVER_MIME.contains(&mime) {
            issues.push(ExchangeIssue::warning(
                "cover_mime_unknown",
                format!("unrecognized cover type: {mime}"),
                None,
            ));
        }
    }
}

fn payload_metadata(block: &serde_json::Value, issues: &mut Vec<ExchangeIssue>) -> PlaylistMeta {
    let name = clean_text(lookup(block, &["name", "title"]));
    let description = opt_text(lookup(block, &["description"]));
    if name.is_empty() {
        issues.push(ExchangeIssue::error(
            "missing_name",
            "playlist without name".to_owned(),
            None,
        ));
    } else if name.len() > MAX_NAME_LENGTH {
        issues.push(ExchangeIssue::error(
            "name_too_long",
            "name too long".to_owned(),
            None,
        ));
    }
    if description.as_ref().is_some_and(|d| d.len() > MAX_DESCRIPTION_LENGTH) {
        issues.push(ExchangeIssue::error(
            "description_too_long",
            "description too long".to_owned(),
            None,
        ));
    }
    let cover = block.get("cover").and_then(|c| {
        c.as_object().map(|_| CoverSummary {
            name: c.get("name").and_then(|v| v.as_str()).map(str::to_owned),
            mime: c.get("mime").and_then(|v| v.as_str()).map(str::to_owned),
            bytes: c.get("bytes").and_then(|v| as_int(Some(v))).map(|v| v as u64),
            has_data: c
                .get("data")
                .and_then(|v| v.as_str())
                .is_some_and(|s| !s.is_empty()),
        })
    });
    PlaylistMeta {
        id: block
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        name,
        description,
        cover,
    }
}

pub fn validate_json(
    data: &serde_json::Value,
    check_local_paths: bool,
) -> Result<ExchangeReport, ExchangeError> {
    let map = data
        .as_object()
        .ok_or_else(|| ExchangeError::Format("JSON must be an object".to_owned()))?;
    let mut issues = Vec::new();
    let version = map.get("version").and_then(|v| as_int(Some(v)));
    let format = map
        .get("format")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or(JSON_FORMAT)
        .to_owned();
    if format != JSON_FORMAT {
        issues.push(ExchangeIssue::error(
            "format_mismatch",
            format!("unknown format: {format}"),
            None,
        ));
    }
    match version {
        None => issues.push(ExchangeIssue::error(
            "missing_version",
            "missing format version".to_owned(),
            None,
        )),
        Some(v) if v > JSON_VERSION as i64 => issues.push(ExchangeIssue::error(
            "unsupported_version",
            format!("unsupported version {v}"),
            None,
        )),
        _ => {}
    }
    let block = map
        .get("playlist")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let meta = payload_metadata(&block, &mut issues);
    let raw_tracks = map.get("tracks");
    let mut entries = Vec::new();
    match raw_tracks {
        Some(serde_json::Value::Array(items)) => {
            if items.len() > MAX_TRACKS {
                issues.push(ExchangeIssue::error(
                    "too_many_tracks",
                    format!("more than {MAX_TRACKS} tracks"),
                    None,
                ));
            }
            for (index, item) in items.iter().enumerate() {
                if !item.is_object() {
                    issues.push(ExchangeIssue::error(
                        "invalid_entry",
                        "track is not an object".to_owned(),
                        Some(index),
                    ));
                    continue;
                }
                entries.push(entry_from_value(item, index));
            }
        }
        _ => issues.push(ExchangeIssue::error(
            "missing_tracks",
            "missing track list".to_owned(),
            None,
        )),
    }
    entries = normalize_entries(entries);
    validate_entries(&entries, &mut issues, check_local_paths);
    if let Some(cover) = block.get("cover") {
        validate_cover_value(cover, &mut issues);
    }
    let valid = issues
        .iter()
        .all(|i| i.severity != Severity::Error);
    Ok(ExchangeReport {
        kind: "json".to_owned(),
        valid,
        playlist: meta,
        track_count: entries.len(),
        issues,
        entries,
    })
}

pub fn validate_csv(
    entries: Vec<TrackEntry>,
    check_local_paths: bool,
) -> ExchangeReport {
    let mut issues = Vec::new();
    let normalized = normalize_entries(entries);
    if normalized.len() > MAX_TRACKS {
        issues.push(ExchangeIssue::error(
            "too_many_tracks",
            format!("more than {MAX_TRACKS} tracks"),
            None,
        ));
    }
    validate_entries(&normalized, &mut issues, check_local_paths);
    let valid = issues
        .iter()
        .all(|i| i.severity != Severity::Error);
    ExchangeReport {
        kind: "csv".to_owned(),
        valid,
        playlist: PlaylistMeta {
            id: None,
            name: String::new(),
            description: None,
            cover: None,
        },
        track_count: normalized.len(),
        issues,
        entries: normalized,
    }
}

pub fn preview_json(text: &str, check_local_paths: bool) -> Result<ExchangeReport, ExchangeError> {
    validate_json(&parse_json(text)?, check_local_paths)
}

pub fn preview_csv(text: &str, check_local_paths: bool) -> Result<ExchangeReport, ExchangeError> {
    Ok(validate_csv(parse_csv(text)?, check_local_paths))
}

pub fn preview_file(path: &Path, check_local_paths: bool) -> Result<ExchangeReport, ExchangeError> {
    let is_csv = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("csv"));
    if is_csv {
        Ok(validate_csv(load_csv(path)?, check_local_paths))
    } else {
        validate_json(&load_json(path)?, check_local_paths)
    }
}
