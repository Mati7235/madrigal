use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::signature::SAMPLE_RATE_HZ;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTool {
    PwCat,
    Parec,
    Ffmpeg,
}

impl CaptureTool {
    pub fn binary(&self) -> &'static str {
        match self {
            CaptureTool::PwCat => "pw-cat",
            CaptureTool::Parec => "parec",
            CaptureTool::Ffmpeg => "ffmpeg",
        }
    }

    pub fn ordered() -> [CaptureTool; 3] {
        [CaptureTool::PwCat, CaptureTool::Parec, CaptureTool::Ffmpeg]
    }
}

impl fmt::Display for CaptureTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.binary())
    }
}

#[derive(Debug)]
pub enum Error {
    ToolMissing(&'static str),
    Spawn(String),
    Wav(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ToolMissing(tool) => write!(f, "capture tool missing: {tool}"),
            Error::Spawn(detail) => write!(f, "capture failed: {detail}"),
            Error::Wav(detail) => write!(f, "unreadable wav: {detail}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub fn search_path(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn available_tool() -> Option<CaptureTool> {
    CaptureTool::ordered()
        .into_iter()
        .find(|tool| search_path(tool.binary()).is_some())
}

pub fn default_capture_path() -> PathBuf {
    madrigal_paths::cache_dir().join("shazam-capture.wav")
}

pub fn capture_seconds(tool: CaptureTool, seconds: u32, dest: &Path) -> Result<()> {
    let seconds = seconds.clamp(1, 30);
    let dest_arg = dest.to_string_lossy().into_owned();
    let status = match tool {
        CaptureTool::PwCat => {
            if search_path("timeout").is_none() {
                return Err(Error::ToolMissing("timeout"));
            }
            let limit = seconds + 2;
            Command::new("timeout")
                .args([
                    "-s",
                    "INT",
                    &limit.to_string(),
                    "pw-cat",
                    "--record",
                    "--rate",
                    "16000",
                    "--channels",
                    "1",
                    &dest_arg,
                ])
                .status()
        }
        CaptureTool::Parec => {
            if search_path("timeout").is_none() {
                return Err(Error::ToolMissing("timeout"));
            }
            let limit = seconds + 2;
            Command::new("timeout")
                .args([
                    "-s",
                    "INT",
                    &limit.to_string(),
                    "parec",
                    "--record",
                    "--rate=16000",
                    "--channels=1",
                    "--format=s16le",
                    "--file-format=wav",
                    &dest_arg,
                ])
                .status()
        }
        CaptureTool::Ffmpeg => Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "pulse",
                "-i",
                "default",
                "-ac",
                "1",
                "-ar",
                "16000",
                "-t",
                &seconds.to_string(),
                &dest_arg,
            ])
            .status(),
    }
    .map_err(|err| Error::Spawn(format!("{tool}: {err}")))?;
    if !status.success() && tool == CaptureTool::Ffmpeg {
        return Err(Error::Spawn(format!("{tool} exited with {status}")));
    }
    let size = std::fs::metadata(dest)
        .map(|m| m.len())
        .map_err(|err| Error::Spawn(format!("{tool}: {err}")))?;
    if size == 0 {
        return Err(Error::Spawn(format!("{tool} wrote an empty file")));
    }
    Ok(())
}

pub fn read_wav_mono16k(path: &Path) -> Result<Vec<i16>> {
    let bytes = std::fs::read(path).map_err(|err| Error::Wav(err.to_string()))?;
    parse_wav_mono16k(&bytes)
}

fn parse_wav_mono16k(bytes: &[u8]) -> Result<Vec<i16>> {
    if bytes.len() > 256 * 1024 * 1024 {
        return Err(Error::Wav("file too large".to_owned()));
    }
    let header = bytes.get(..12).ok_or_else(|| Error::Wav("too short".to_owned()))?;
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return Err(Error::Wav("not a wav file".to_owned()));
    }
    let mut format: Option<WavFormat> = None;
    let mut data: Option<&[u8]> = None;
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let len = u32::from_le_bytes(
            bytes[offset + 4..offset + 8]
                .try_into()
                .map_err(|_| Error::Wav("bad chunk header".to_owned()))?,
        ) as usize;
        let body_start = offset + 8;
        let body_end = body_start.saturating_add(len).min(bytes.len());
        let body = bytes.get(body_start..body_end).unwrap_or_default();
        if id == b"fmt " {
            format = parse_format(body)?;
        } else if id == b"data" {
            data = Some(body);
        }
        offset = body_end + (body_end - body_start) % 2;
        if body_end == bytes.len() {
            break;
        }
    }
    let format = format.ok_or_else(|| Error::Wav("missing fmt chunk".to_owned()))?;
    let data = data.ok_or_else(|| Error::Wav("missing data chunk".to_owned()))?;
    if data.is_empty() {
        return Err(Error::Wav("no samples".to_owned()));
    }
    let mono = decode_mono(data, &format)?;
    if mono.is_empty() {
        return Err(Error::Wav("no samples".to_owned()));
    }
    Ok(resample_to_16k(&mono, format.rate))
}

struct WavFormat {
    kind: u16,
    channels: usize,
    rate: u32,
    bits: u16,
}

fn parse_format(body: &[u8]) -> Result<Option<WavFormat>> {
    if body.len() < 16 {
        return Err(Error::Wav("bad fmt chunk".to_owned()));
    }
    let kind = u16::from_le_bytes([body[0], body[1]]);
    let channels = u16::from_le_bytes([body[2], body[3]]) as usize;
    let rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
    let bits = u16::from_le_bytes([body[14], body[15]]);
    if channels == 0 || channels > 64 {
        return Err(Error::Wav("bad channel count".to_owned()));
    }
    if !(1000..=192_000).contains(&rate) {
        return Err(Error::Wav("bad sample rate".to_owned()));
    }
    if kind != 1 && kind != 3 {
        return Err(Error::Wav(format!("unsupported encoding {kind}")));
    }
    if ![8, 16, 24, 32].contains(&bits) {
        return Err(Error::Wav(format!("unsupported depth {bits}")));
    }
    if kind == 3 && bits != 32 {
        return Err(Error::Wav("bad float depth".to_owned()));
    }
    Ok(Some(WavFormat {
        kind,
        channels,
        rate,
        bits,
    }))
}

fn decode_mono(data: &[u8], format: &WavFormat) -> Result<Vec<i32>> {
    let depth = (format.bits / 8) as usize;
    let frame = depth * format.channels;
    if frame == 0 || !data.len().is_multiple_of(frame) {
        return Err(Error::Wav("ragged data chunk".to_owned()));
    }
    let frames = data.len() / frame;
    if frames > 64 * 1024 * 1024 {
        return Err(Error::Wav("too many frames".to_owned()));
    }
    let mut mono = Vec::with_capacity(frames);
    for f in 0..frames {
        let mut sum: i64 = 0;
        for c in 0..format.channels {
            let at = f * frame + c * depth;
            let sample = data
                .get(at..at + depth)
                .ok_or_else(|| Error::Wav("short frame".to_owned()))?;
            sum += i64::from(decode_sample(sample, format)?);
        }
        mono.push((sum / format.channels as i64) as i32);
    }
    Ok(mono)
}

fn decode_sample(raw: &[u8], format: &WavFormat) -> Result<i16> {
    match (format.kind, format.bits) {
        (1, 8) => Ok((((raw[0] as i32) - 128) << 8) as i16),
        (1, 16) => Ok(i16::from_le_bytes([raw[0], raw[1]])),
        (1, 24) => {
            let word = i32::from_le_bytes([raw[0], raw[1], raw[2], 0]) >> 8;
            Ok((word >> 8) as i16)
        }
        (1, 32) => Ok((i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) >> 16) as i16),
        (3, 32) => {
            let v = f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
            if !v.is_finite() {
                return Err(Error::Wav("non-finite float sample".to_owned()));
            }
            Ok((v.clamp(-1.0, 1.0) * 32767.0) as i16)
        }
        _ => Err(Error::Wav("unsupported sample".to_owned())),
    }
}

fn resample_to_16k(mono: &[i32], rate: u32) -> Vec<i16> {
    if rate == SAMPLE_RATE_HZ {
        return mono.iter().map(|s| (*s).clamp(-32768, 32767) as i16).collect();
    }
    let target = (mono.len() as u64 * u64::from(SAMPLE_RATE_HZ) / u64::from(rate)) as usize;
    let mut out = Vec::with_capacity(target);
    for i in 0..target {
        let pos = i as f64 * f64::from(rate) / f64::from(SAMPLE_RATE_HZ);
        let lo = pos.floor() as usize;
        let frac = pos - pos.floor();
        let a = i64::from(mono.get(lo).copied().unwrap_or(0)) as f64;
        let b = i64::from(mono.get(lo + 1).copied().unwrap_or(a as i32)) as f64;
        out.push((a + (b - a) * frac).clamp(-32768.0, 32767.0) as i16);
    }
    out
}
