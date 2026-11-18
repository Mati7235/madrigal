use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    pub title: String,
    pub start_secs: f64,
    pub end_secs: f64,
}

#[derive(Debug)]
pub enum PostprocessError {
    Probe(String),
    Ffmpeg(String),
    Empty,
    Io(io::Error),
}

impl std::fmt::Display for PostprocessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Probe(message) | Self::Ffmpeg(message) => f.write_str(message),
            Self::Empty => f.write_str("no audio left after cutting segments"),
            Self::Io(err) => write!(f, "io: {err}"),
        }
    }
}

impl From<io::Error> for PostprocessError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn probe_duration(ffprobe_bin: &Path, input: &Path) -> Result<f64, PostprocessError> {
    let out = Command::new(ffprobe_bin)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(input)
        .output()
        .map_err(|err| PostprocessError::Probe(format!("ffprobe failed: {err}")))?;
    if !out.status.success() {
        return Err(PostprocessError::Probe(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .map_err(|_| PostprocessError::Probe("unparseable duration".to_string()))
}

pub fn read_chapters(ffprobe_bin: &Path, input: &Path) -> Result<Vec<Chapter>, PostprocessError> {
    let out = Command::new(ffprobe_bin)
        .args(["-v", "error", "-show_chapters", "-of", "json"])
        .arg(input)
        .output()
        .map_err(|err| PostprocessError::Probe(format!("ffprobe failed: {err}")))?;
    if !out.status.success() {
        return Err(PostprocessError::Probe(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    let raw: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|_| PostprocessError::Probe("unparseable chapters".to_string()))?;
    let mut chapters = Vec::new();
    if let Some(list) = raw.get("chapters").and_then(|v| v.as_array()) {
        for entry in list {
            let start = entry
                .get("start_time")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            let end = entry
                .get("end_time")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            let title = entry
                .get("tags")
                .and_then(|t| t.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            chapters.push(Chapter {
                title,
                start_secs: start,
                end_secs: end,
            });
        }
    }
    Ok(chapters)
}

fn normalize_segments(segments: &[(f64, f64)], duration: f64) -> Vec<(f64, f64)> {
    let mut kept: Vec<(f64, f64)> = segments
        .iter()
        .map(|(a, b)| (a.max(0.0), b.min(duration)))
        .filter(|(a, b)| b - a > 1e-3)
        .collect();
    kept.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (start, end) in kept {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    merged
}

fn removed_before(segments: &[(f64, f64)], point: f64) -> f64 {
    segments
        .iter()
        .map(|(start, end)| {
            if point <= *start {
                0.0
            } else {
                (end.min(point) - start).max(0.0)
            }
        })
        .sum()
}

pub fn shift_chapters(chapters: &[Chapter], segments: &[(f64, f64)]) -> Vec<Chapter> {
    chapters
        .iter()
        .map(|chapter| Chapter {
            title: chapter.title.clone(),
            start_secs: chapter.start_secs - removed_before(segments, chapter.start_secs),
            end_secs: chapter.end_secs - removed_before(segments, chapter.end_secs),
        })
        .filter(|chapter| chapter.end_secs - chapter.start_secs > 1e-3)
        .collect()
}

fn audio_codec_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "opus" => "libopus",
        "mp3" => "libmp3lame",
        "ogg" | "oga" => "libvorbis",
        _ => "aac",
    }
}

fn write_metadata(chapters: &[Chapter], dest: &Path) -> io::Result<()> {
    let mut text = String::from(";FFMETADATA1\n");
    for chapter in chapters {
        let start = (chapter.start_secs.max(0.0) * 1000.0).round() as i64;
        let end = (chapter.end_secs.max(0.0) * 1000.0).round() as i64;
        text.push_str("[CHAPTER]\nTIMEBASE=1/1000\n");
        text.push_str(&format!("START={start}\nEND={end}\n"));
        text.push_str(&format!("title={}\n", chapter.title.replace('\n', " ")));
    }
    fs::write(dest, text)
}

fn run_ffmpeg(ffmpeg_bin: &Path, args: &[String]) -> Result<(), PostprocessError> {
    let out = Command::new(ffmpeg_bin)
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .map_err(|err| PostprocessError::Ffmpeg(format!("ffmpeg failed: {err}")))?;
    if out.status.success() {
        return Ok(());
    }
    Err(PostprocessError::Ffmpeg(
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    ))
}

pub fn cut_segments(
    input: &Path,
    segments: &[(f64, f64)],
    chapters: &[Chapter],
    ffmpeg_bin: &Path,
    ffprobe_bin: &Path,
) -> Result<PathBuf, PostprocessError> {
    let duration = probe_duration(ffprobe_bin, input)?;
    let removed = normalize_segments(segments, duration);
    if removed.is_empty() {
        return Err(PostprocessError::Empty);
    }
    let mut bounds = vec![0.0];
    for (start, end) in &removed {
        bounds.push(*start);
        bounds.push(*end);
    }
    bounds.push(duration);
    let mut keep: Vec<(f64, f64)> = Vec::new();
    let mut index = 0;
    while index + 1 < bounds.len() {
        if bounds[index + 1] - bounds[index] > 1e-3 {
            keep.push((bounds[index], bounds[index + 1]));
        }
        index += 2;
    }
    if keep.is_empty() {
        return Err(PostprocessError::Empty);
    }
    let mut filter = String::new();
    let mut labels = Vec::new();
    for (index, (start, end)) in keep.iter().enumerate() {
        filter.push_str(&format!(
            "[0:a]atrim=start={start:.3}:end={end:.3},asetpts=PTS-STARTPTS[a{index}];"
        ));
        labels.push(format!("[a{index}]"));
    }
    filter.push_str(&format!("{}concat=n={}:v=0:a=1[out]", labels.concat(), keep.len()));
    let shifted = shift_chapters(chapters, &removed);
    if shifted.is_empty() && !chapters.is_empty() {
        return Err(PostprocessError::Empty);
    }
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("audio");
    let extension = input
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("m4a");
    let cut = input.with_file_name(format!("{stem}.madrigal-cut.{extension}"));
    let meta = input.with_file_name(format!("{stem}.madrigal-meta.txt"));
    write_metadata(&shifted, &meta)?;
    let mut args = vec![
        "-i".to_string(),
        input.to_string_lossy().into_owned(),
        "-i".to_string(),
        meta.to_string_lossy().into_owned(),
        "-filter_complex".to_string(),
        filter,
        "-map".to_string(),
        "[out]".to_string(),
        "-vn".to_string(),
        "-c:a".to_string(),
        audio_codec_for(input).to_string(),
    ];
    if extension.eq_ignore_ascii_case("opus") {
        args.push("-b:a".to_string());
        args.push("128k".to_string());
    }
    args.extend(
        [
            "-map_metadata",
            "1",
            "-map_chapters",
            "1",
            &cut.to_string_lossy(),
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    let result = run_ffmpeg(ffmpeg_bin, &args);
    let _ = fs::remove_file(&meta);
    result?;
    let produced = probe_duration(ffprobe_bin, &cut)?;
    let expected = duration - removed.iter().map(|(a, b)| b - a).sum::<f64>();
    if produced <= 0.0 || (produced - expected).abs() > 1.0 {
        let _ = fs::remove_file(&cut);
        return Err(PostprocessError::Ffmpeg(format!(
            "cut produced {produced:.1}s, expected {expected:.1}s"
        )));
    }
    Ok(cut)
}
