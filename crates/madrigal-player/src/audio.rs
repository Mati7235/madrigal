use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use madrigal_settings::{sanitize_gains, EQ_BANDS};
use madrigal_settings::Player as PlayerConfig;

use crate::engine::Engine;
use crate::Error;

pub const LOUDNORM_AF: &str = "loudnorm=I=-16:TP=-1.5:LRA=11";

pub const SKIP_SILENCE_AF: &str = "silenceremove=start_periods=1:start_duration=0.2:start_threshold=-60dB:stop_periods=1:stop_duration=0.2:stop_threshold=-60dB";

pub const PRESET_FLAT: [f32; 10] = [0.0; 10];
pub const PRESET_BASS: [f32; 10] = [5.0, 5.0, 4.0, 3.0, 1.0, 0.0, 0.0, -1.0, -1.0, -1.0];
pub const PRESET_TREBLE: [f32; 10] = [-2.0, -2.0, -1.0, -1.0, 0.0, 1.0, 2.0, 4.0, 5.0, 5.0];
pub const PRESET_VOCAL: [f32; 10] = [-3.0, -3.0, -2.0, -1.0, 2.0, 4.0, 3.0, 2.0, 0.0, -1.0];

pub type PresetMap = BTreeMap<String, [f32; 10]>;

pub fn preset_gains(name: &str) -> Option<[f32; 10]> {
    match name {
        "flat" => Some(PRESET_FLAT),
        "bass" => Some(PRESET_BASS),
        "treble" => Some(PRESET_TREBLE),
        "vocal" => Some(PRESET_VOCAL),
        _ => None,
    }
}

pub fn custom_presets(config: &PlayerConfig) -> PresetMap {
    let raw: BTreeMap<String, Vec<f32>> =
        serde_json::from_str(&config.equalizer_custom_profiles_json).unwrap_or_default();
    raw.into_iter()
        .map(|(name, gains)| {
            let fixed = sanitize_gains(&gains);
            let mut out = [0.0; 10];
            out.copy_from_slice(&fixed);
            (name, out)
        })
        .collect()
}

pub fn resolve_gains(config: &PlayerConfig, name: &str) -> Option<[f32; 10]> {
    preset_gains(name).or_else(|| custom_presets(config).get(name).copied())
}

pub fn export_presets(path: &Path, presets: &PresetMap) -> io::Result<()> {
    let text = serde_json::to_string_pretty(presets)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    madrigal_files::atomic_write_text(path, &text)?;
    Ok(())
}

pub fn import_presets(path: &Path) -> io::Result<PresetMap> {
    let text = std::fs::read_to_string(path)?;
    let raw: BTreeMap<String, Vec<f32>> = serde_json::from_str(&text)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    Ok(raw
        .into_iter()
        .map(|(name, gains)| {
            let fixed = sanitize_gains(&gains);
            let mut out = [0.0; 10];
            out.copy_from_slice(&fixed);
            (name, out)
        })
        .collect())
}

pub fn headroom_db(gains: &[f32]) -> f32 {
    gains.iter().fold(0.0_f32, |max, &gain| max.max(gain)).max(0.0)
}

pub fn equalizer_segments(gains: &[f32]) -> Vec<String> {
    EQ_BANDS
        .iter()
        .zip(gains.iter())
        .filter(|(_, gain)| **gain != 0.0)
        .map(|(freq, gain)| format!("equalizer=f={freq}:width_type=o:width=1:g={gain}"))
        .collect()
}

pub fn equalizer_af(gains: &[f32]) -> Option<String> {
    let segments = equalizer_segments(gains);
    if segments.is_empty() {
        return None;
    }
    let headroom = headroom_db(gains);
    if headroom > 0.0 {
        let mut chain = vec![format!("volume=volume=-{headroom}dB")];
        chain.extend(segments);
        Some(chain.join(","))
    } else {
        Some(segments.join(","))
    }
}

pub fn pitch_multiplier(semitones: f32) -> f64 {
    2.0_f64.powf(f64::from(semitones) / 12.0)
}

pub fn tempo_af(speed: f32, semitones: f32) -> Option<String> {
    let mut parts = Vec::new();
    if (speed - 1.0).abs() > f32::EPSILON {
        parts.push("scaletempo2".to_owned());
    }
    if semitones.abs() > f32::EPSILON {
        parts.push(format!("rubberband=pitch-scale={:.6}", pitch_multiplier(semitones)));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(","))
    }
}

pub fn build_af(config: &PlayerConfig) -> String {
    let mut parts = Vec::new();
    if config.equalizer_enabled {
        if let Some(chain) = equalizer_af(&config.equalizer_gains) {
            parts.push(chain);
        }
    }
    if config.normalization == madrigal_settings::Normalization::Dynamic {
        parts.push(LOUDNORM_AF.to_owned());
    }
    if config.skip_silence {
        parts.push(SKIP_SILENCE_AF.to_owned());
    }
    if let Some(chain) = tempo_af(config.playback_speed, config.pitch_semitones) {
        parts.push(chain);
    }
    parts.join(",")
}

pub fn apply_audio(engine: &Engine, config: &PlayerConfig) -> Result<(), Error> {
    let speed = config.playback_speed.clamp(0.5, 2.0);
    engine.set_speed(f64::from(speed))?;
    engine.set_audio_pitch_correction(true)?;
    engine.apply_af(&build_af(config))
}

pub const MAX_AUTO_SKIPS: u32 = 5;

pub struct ErrorSkipper {
    consecutive: u32,
    max: u32,
}

impl ErrorSkipper {
    pub fn new(max: u32) -> Self {
        Self {
            consecutive: 0,
            max,
        }
    }

    pub fn on_error(&mut self) -> bool {
        if self.consecutive < self.max {
            self.consecutive += 1;
            true
        } else {
            false
        }
    }

    pub fn on_success(&mut self) {
        self.consecutive = 0;
    }

    pub fn consecutive(&self) -> u32 {
        self.consecutive
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeAction {
    Keep,
    Pause,
    Resume,
}

pub struct ZeroVolumePause {
    stashed: Option<i32>,
    engaged: bool,
}

impl Default for ZeroVolumePause {
    fn default() -> Self {
        Self::new()
    }
}

impl ZeroVolumePause {
    pub fn new() -> Self {
        Self {
            stashed: None,
            engaged: false,
        }
    }

    pub fn set_volume(&mut self, current: i32, requested: i32) -> VolumeAction {
        if requested == 0 {
            if self.engaged {
                return VolumeAction::Keep;
            }
            self.engaged = true;
            if current != 0 {
                self.stashed = Some(current);
            }
            VolumeAction::Pause
        } else if self.engaged {
            self.engaged = false;
            self.stashed = None;
            VolumeAction::Resume
        } else {
            VolumeAction::Keep
        }
    }

    pub fn resume_target(&mut self, current: i32, fallback: i32) -> Option<i32> {
        if !self.engaged {
            return None;
        }
        self.engaged = false;
        if current == 0 {
            Some(self.stashed.take().unwrap_or(fallback))
        } else {
            self.stashed = None;
            None
        }
    }

    pub fn is_engaged(&self) -> bool {
        self.engaged
    }
}
