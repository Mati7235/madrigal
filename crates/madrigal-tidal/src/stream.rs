use std::sync::{Mutex, OnceLock};

use base64::Engine as _;
use regex::Regex;

use crate::auth::{Client, TokenSet};
use crate::catalog::track_id_param;
use crate::Error;

pub const DEFAULT_QUALITY: &str = "HI_RES_LOSSLESS";
pub const MPD_MIME: &str = "application/dash+xml";
pub const BTS_MIME: &str = "application/vnd.tidal.bts";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestKind {
    Bts,
    HlsFile,
}

#[derive(Debug, Clone)]
pub struct StreamResolution {
    pub track_id: String,
    pub url: String,
    pub kind: ManifestKind,
    pub quality: String,
    pub sample_rate: u32,
    pub bit_depth: u32,
    pub codec: Option<String>,
}

pub fn url_ok(url: &str) -> bool {
    if url.is_empty() {
        return false;
    }
    if url.bytes().any(|byte| matches!(byte, b'\r' | b'\n' | b'\t' | b'#')) {
        return false;
    }
    if let Some(rest) = url.strip_prefix("https://") {
        return !rest.is_empty() && !rest.starts_with('/');
    }
    if url.contains("://") {
        return false;
    }
    if url.contains('\\') {
        return false;
    }
    !url.split('/').any(|part| part == "..")
}

fn attr_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new("\\b([A-Za-z_:][A-Za-z0-9_.:-]*)=\"([^\"]*)\"").expect("attr")
    })
}

fn attr_of(tag: &str, name: &str) -> Option<String> {
    attr_re()
        .captures_iter(tag)
        .find(|caps| caps.get(1).is_some_and(|key| key.as_str() == name))
        .and_then(|caps| caps.get(2))
        .map(|value| value.as_str().to_owned())
}

fn tag(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}");
    let start = xml.find(open.as_str())?;
    let stop = xml[start..].find('>')?;
    Some(xml[start..start + stop].to_owned())
}

fn timeline(xml: &str) -> Vec<(u64, u64)> {
    let entry = Regex::new("<S\\b[^>]*>").expect("timeline entry");
    let mut spans = Vec::new();
    for hit in entry.find_iter(xml) {
        let text = hit.as_str();
        let length: u64 = attr_of(text, "d").and_then(|v| v.parse().ok()).unwrap_or(0);
        if length == 0 {
            continue;
        }
        let repeat: u64 = attr_of(text, "r").and_then(|v| v.parse().ok()).unwrap_or(0);
        spans.push((length, repeat));
    }
    spans
}

fn parse_duration(value: &str) -> Option<f64> {
    let body = value.strip_prefix("PT")?;
    let part = Regex::new("(?:([0-9]+)H)?(?:([0-9]+)M)?(?:([0-9]+(?:[.][0-9]+)?)S)?")
        .expect("iso duration");
    let caps = part.captures(body)?;
    if caps.get(0).is_some_and(|whole| whole.as_str().is_empty()) {
        return None;
    }
    let hours: f64 = caps.get(1).map(|v| v.as_str().parse().unwrap_or(0.0)).unwrap_or(0.0);
    let minutes: f64 = caps.get(2).map(|v| v.as_str().parse().unwrap_or(0.0)).unwrap_or(0.0);
    let seconds: f64 = caps.get(3).map(|v| v.as_str().parse().unwrap_or(0.0)).unwrap_or(0.0);
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

struct Mpd {
    duration_secs: f64,
    codecs: Option<String>,
    sample_rate: u32,
    media_template: String,
    timescale: u64,
    spans: Vec<(u64, u64)>,
}

fn parse_mpd(xml: &str) -> Result<Mpd, Error> {
    if xml.contains("<ContentProtection") {
        return Err(Error::Drm);
    }
    let root = tag(xml, "MPD").ok_or_else(|| Error::Protocol("mpd lacks MPD".to_owned()))?;
    let duration_secs = root
        .find("mediaPresentationDuration")
        .and_then(|_| attr_of(&root, "mediaPresentationDuration"))
        .and_then(|value| parse_duration(&value))
        .unwrap_or(0.0);
    let template = tag(xml, "SegmentTemplate")
        .ok_or_else(|| Error::Protocol("mpd lacks SegmentTemplate".to_owned()))?;
    let media_template = attr_of(&template, "media")
        .filter(|value| value.contains("$Number$"))
        .ok_or_else(|| Error::Protocol("mpd lacks media template".to_owned()))?;
    let timescale = template
        .find("timescale")
        .and_then(|_| attr_of(&template, "timescale"))
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let representation = tag(xml, "Representation").unwrap_or_default();
    let codecs = attr_of(&representation, "codecs");
    let sample_rate = representation
        .find("audioSamplingRate")
        .and_then(|_| attr_of(&representation, "audioSamplingRate"))
        .and_then(|value| value.parse().ok())
        .unwrap_or(44100);
    let spans = timeline(xml);
    if spans.is_empty() {
        return Err(Error::Protocol("mpd lacks timeline".to_owned()));
    }
    Ok(Mpd {
        duration_secs,
        codecs,
        sample_rate,
        media_template,
        timescale: timescale.max(1),
        spans,
    })
}

fn dash_urls(mpd: &Mpd) -> Vec<String> {
    let mut count: u64 = 2;
    for (_, repeat) in &mpd.spans {
        count += (*repeat).max(1);
    }
    (0..count)
        .map(|index| mpd.media_template.replace("$Number$", &index.to_string()))
        .collect()
}

fn chunk_secs(length: u64, timescale: u64) -> f64 {
    length as f64 / timescale.max(1) as f64
}

pub fn dash_hls(mpd_xml: &str) -> Result<(String, Option<String>, u32), Error> {
    let mpd = parse_mpd(mpd_xml)?;
    let urls = dash_urls(&mpd);
    let first = mpd.spans.first().map(|span| span.0).unwrap_or(0);
    let last = mpd.spans.last().map(|span| span.0).unwrap_or(first);
    let mut hls = format!(
        "#EXTM3U\n#EXT-X-TARGETDURATION:{}\n#EXT-X-VERSION:3\n",
        mpd.duration_secs as u64
    );
    let body: Vec<String> = urls
        .iter()
        .enumerate()
        .map(|(index, url)| {
            let length = if index + 1 == urls.len() { last } else { first };
            format!("#EXTINF:{:.3},\n{url}", chunk_secs(length, mpd.timescale))
        })
        .collect();
    hls.push_str(&body.join("\n"));
    hls.push_str("\n#EXT-X-ENDLIST\n");
    Ok((upgrade_hls(&hls), mpd.codecs, mpd.sample_rate))
}

pub fn upgrade_hls(playlist: &str) -> String {
    let lines: Vec<&str> = playlist.lines().collect();
    if lines.iter().any(|line| line.starts_with("#EXT-X-MAP")) {
        return playlist.to_owned();
    }
    let target = lines
        .iter()
        .find(|line| line.starts_with("#EXT-X-TARGETDURATION"))
        .copied()
        .unwrap_or("#EXT-X-TARGETDURATION:10");
    let mut duration = "#EXTINF:10.000,";
    let mut segments: Vec<(&str, &str)> = Vec::new();
    for line in &lines {
        if line.starts_with("#EXTINF") {
            duration = line;
        } else if !line.is_empty() && !line.starts_with('#') && url_ok(line) {
            segments.push((duration, line));
        }
    }
    if segments.len() < 2 {
        return playlist.to_owned();
    }
    let mut output = vec![
        "#EXTM3U".to_owned(),
        "#EXT-X-VERSION:7".to_owned(),
        target.to_owned(),
        "#EXT-X-PLAYLIST-TYPE:VOD".to_owned(),
        format!("#EXT-X-MAP:URI=\"{}\"", segments[0].1),
    ];
    for (extinf, url) in segments.iter().skip(1) {
        output.push((*extinf).to_owned());
        output.push((*url).to_owned());
    }
    output.push("#EXT-X-ENDLIST".to_owned());
    output.join("\n") + "\n"
}

fn created() -> &'static Mutex<Vec<std::path::PathBuf>> {
    static CELL: OnceLock<Mutex<Vec<std::path::PathBuf>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(Vec::new()))
}

fn write_playlist(track_id: &str, playlist: &str) -> Result<String, Error> {
    let dir = madrigal_paths::cache_dir().join("tidal");
    std::fs::create_dir_all(&dir).map_err(|err| Error::Io(err.to_string()))?;
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let tag = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = format!("tidal-{track_id}-{}.m3u8", std::process::id());
    let path = dir.join(format!("{name}.{tag}"));
    madrigal_files::atomic_write_text(&path, playlist)
        .map_err(|err| Error::Io(err.to_string()))?;
    let final_path = dir.join(name);
    let _ = std::fs::remove_file(&final_path);
    std::fs::rename(&path, &final_path).map_err(|err| Error::Io(err.to_string()))?;
    created()
        .lock()
        .map(|mut owned| owned.push(final_path.clone()))
        .map_err(|_| Error::Io("playlist registry poisoned".to_owned()))?;
    Ok(final_path.to_string_lossy().into_owned())
}

pub fn cleanup_streams() {
    let owned = created().lock().map(|mut list| std::mem::take(&mut *list));
    for path in owned.unwrap_or_default() {
        if !path.is_symlink() && path.is_file() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn bts_urls(manifest_b64: &str) -> Result<(Vec<String>, Option<String>), Error> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(manifest_b64)
        .map_err(|err| Error::Protocol(err.to_string()))?;
    let body: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|_| Error::Protocol("bts is not json".to_owned()))?;
    let encryption = body
        .get("encryptionType")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    if !encryption.is_empty() && !encryption.eq_ignore_ascii_case("none") {
        return Err(Error::Drm);
    }
    let urls = body
        .get("urls")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|value| value.as_str())
        .filter(|url| url_ok(url))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let codec = body
        .get("codecs")
        .and_then(|value| value.as_str())
        .map(|value| value.split('.').next().unwrap_or(value).to_uppercase());
    Ok((urls, codec))
}

pub fn resolve_stream(
    client: &Client,
    token: &TokenSet,
    track_id: &str,
    quality: Option<&str>,
) -> Result<StreamResolution, Error> {
    let raw = track_id_param(track_id)?.to_owned();
    let response = client.get(
        &format!("/v1/tracks/{raw}/playbackinfopostpaywall"),
        token,
        &[
            ("playbackmode", "STREAM".to_owned()),
            ("audioquality", quality.unwrap_or(DEFAULT_QUALITY).to_owned()),
            ("assetpresentation", "FULL".to_owned()),
        ],
    )?;
    if response.status() == reqwest::StatusCode::NOT_FOUND
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        return Err(Error::Unavailable("stream not available".to_owned()));
    }
    if !response.status().is_success() {
        return Err(Error::Status(response.status().as_u16()));
    }
    let body = crate::body_json(response)?;
    let mime = body
        .get("manifestMimeType")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned();
    let manifest = body
        .get("manifest")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::Protocol("stream lacks manifest".to_owned()))?;
    let quality_name = body
        .get("audioQuality")
        .and_then(|value| value.as_str())
        .unwrap_or(DEFAULT_QUALITY)
        .to_owned();
    let sample_rate = body
        .get("sampleRate")
        .and_then(|value| value.as_u64())
        .unwrap_or(44100) as u32;
    let bit_depth = body
        .get("bitDepth")
        .and_then(|value| value.as_u64())
        .unwrap_or(16) as u32;
    if mime.contains(BTS_MIME) {
        let (urls, codec) = bts_urls(manifest)?;
        let url = urls
            .into_iter()
            .next()
            .ok_or_else(|| Error::Unavailable("manifest is empty".to_owned()))?;
        return Ok(StreamResolution {
            track_id: format!("tidal:{raw}"),
            url,
            kind: ManifestKind::Bts,
            quality: quality_name,
            sample_rate,
            bit_depth,
            codec,
        });
    }
    if mime.contains(MPD_MIME) {
        let raw_bytes = base64::engine::general_purpose::STANDARD
            .decode(manifest)
            .map_err(|err| Error::Protocol(err.to_string()))?;
        let xml = String::from_utf8(raw_bytes)
            .map_err(|err| Error::Protocol(err.to_string()))?;
        let (playlist, codec, rate) = dash_hls(&xml)?;
        let path = write_playlist(&raw, &playlist)?;
        return Ok(StreamResolution {
            track_id: format!("tidal:{raw}"),
            url: path,
            kind: ManifestKind::HlsFile,
            quality: quality_name,
            sample_rate: rate,
            bit_depth,
            codec,
        });
    }
    Err(Error::Protocol(format!("unknown manifest: {mime}")))
}
