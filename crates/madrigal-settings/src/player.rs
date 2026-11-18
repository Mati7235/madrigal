use serde::{Deserialize, Serialize};

use crate::de::{or_default, vec_f32_or_zero};

pub const EQ_BANDS: [f64; 10] = [
    31.5, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];
const LEGACY_EQ_BANDS: [f64; 5] = [60.0, 230.0, 910.0, 3600.0, 14000.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioQuality {
    #[default]
    Best,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Normalization {
    #[default]
    Off,
    ReplaygainTrack,
    ReplaygainAlbum,
    Dynamic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TransitionMode {
    #[default]
    Normal,
    Crossfade,
    Gapless,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    #[default]
    Off,
    One,
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Player {
    #[serde(deserialize_with = "or_default")]
    pub volume: i32,
    #[serde(deserialize_with = "or_default")]
    pub audio_quality: AudioQuality,
    #[serde(deserialize_with = "or_default")]
    pub playback_speed: f32,
    #[serde(deserialize_with = "or_default")]
    pub pitch_semitones: f32,
    #[serde(deserialize_with = "or_default")]
    pub equalizer_enabled: bool,
    #[serde(default, deserialize_with = "vec_f32_or_zero")]
    pub equalizer_gains: Vec<f32>,
    #[serde(deserialize_with = "or_default")]
    pub equalizer_control_mode: String,
    #[serde(deserialize_with = "or_default")]
    pub equalizer_auto_headroom_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub equalizer_output_gain_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub equalizer_output_gain_mb: i32,
    #[serde(deserialize_with = "or_default")]
    pub equalizer_bass_boost_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub equalizer_bass_boost_strength: i32,
    #[serde(deserialize_with = "or_default")]
    pub equalizer_virtualizer_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub equalizer_virtualizer_strength: i32,
    #[serde(deserialize_with = "or_default")]
    pub equalizer_selected_profile_id: String,
    #[serde(deserialize_with = "or_default")]
    pub equalizer_custom_profiles_json: String,
    #[serde(deserialize_with = "or_default")]
    pub normalization: Normalization,
    #[serde(deserialize_with = "or_default")]
    pub transition_mode: TransitionMode,
    #[serde(deserialize_with = "or_default")]
    pub crossfade_seconds: f32,
    #[serde(deserialize_with = "or_default")]
    pub resume_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub resume_autoplay: bool,
    #[serde(deserialize_with = "or_default")]
    pub persistent_queue: bool,
    #[serde(deserialize_with = "or_default")]
    pub preload_next_song: bool,
    #[serde(deserialize_with = "or_default")]
    pub permanent_shuffle: bool,
    #[serde(deserialize_with = "or_default")]
    pub skip_silence: bool,
    #[serde(deserialize_with = "or_default")]
    pub audio_offload: bool,
    #[serde(deserialize_with = "or_default")]
    pub auto_download_on_like: bool,
    #[serde(deserialize_with = "or_default")]
    pub auto_skip_next_on_error: bool,
    #[serde(deserialize_with = "or_default")]
    pub pause_on_device_mute: bool,
    #[serde(deserialize_with = "or_default")]
    pub device_mute_recovery_volume: i32,
    #[serde(deserialize_with = "or_default")]
    pub repeat_mode: RepeatMode,
    #[serde(deserialize_with = "or_default")]
    pub queue_edit_lock: bool,
    #[serde(deserialize_with = "or_default")]
    pub seek_extra_seconds: bool,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            volume: 70,
            audio_quality: AudioQuality::Best,
            playback_speed: 1.0,
            pitch_semitones: 0.0,
            equalizer_enabled: false,
            equalizer_gains: vec![0.0; EQ_BANDS.len()],
            equalizer_control_mode: String::new(),
            equalizer_auto_headroom_enabled: true,
            equalizer_output_gain_enabled: false,
            equalizer_output_gain_mb: 0,
            equalizer_bass_boost_enabled: false,
            equalizer_bass_boost_strength: 0,
            equalizer_virtualizer_enabled: false,
            equalizer_virtualizer_strength: 0,
            equalizer_selected_profile_id: String::new(),
            equalizer_custom_profiles_json: String::new(),
            normalization: Normalization::Off,
            transition_mode: TransitionMode::Normal,
            crossfade_seconds: 3.0,
            resume_enabled: true,
            resume_autoplay: false,
            persistent_queue: true,
            preload_next_song: true,
            permanent_shuffle: false,
            skip_silence: false,
            audio_offload: false,
            auto_download_on_like: false,
            auto_skip_next_on_error: true,
            pause_on_device_mute: true,
            device_mute_recovery_volume: 50,
            repeat_mode: RepeatMode::Off,
            queue_edit_lock: false,
            seek_extra_seconds: false,
        }
    }
}

fn bisect_right(sorted: &[f64], value: f64) -> usize {
    let mut low = 0;
    let mut high = sorted.len();
    while low < high {
        let mid = (low + high) / 2;
        if sorted[mid] <= value {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

fn clamp_gain(value: f32) -> f32 {
    crate::sanitize::clamp_f32(value, -12.0, 12.0, 0.0)
}

pub fn sanitize_gains(raw: &[f32]) -> Vec<f32> {
    if raw.len() == EQ_BANDS.len() {
        return raw.iter().map(|gain| clamp_gain(*gain)).collect();
    }
    if raw.len() == LEGACY_EQ_BANDS.len() {
        let mut migrated = Vec::with_capacity(EQ_BANDS.len());
        for frequency in EQ_BANDS {
            let upper = bisect_right(&LEGACY_EQ_BANDS, frequency)
                .clamp(1, LEGACY_EQ_BANDS.len() - 1);
            let lower = upper - 1;
            let span = (LEGACY_EQ_BANDS[upper] / LEGACY_EQ_BANDS[lower]).log2();
            let mut ratio = if span == 0.0 {
                0.0
            } else {
                (frequency / LEGACY_EQ_BANDS[lower]).log2() / span
            };
            ratio = ratio.clamp(0.0, 1.0);
            let gain = raw[lower] + ratio as f32 * (raw[upper] - raw[lower]);
            migrated.push(clamp_gain(gain));
        }
        return migrated;
    }
    vec![0.0; EQ_BANDS.len()]
}

impl Player {
    pub fn sanitize(&mut self) {
        self.volume = crate::sanitize::clamp_i32(self.volume, 0, 130);
        self.playback_speed =
            crate::sanitize::clamp_f32(self.playback_speed, 0.5, 2.0, 1.0);
        self.pitch_semitones =
            crate::sanitize::clamp_f32(self.pitch_semitones, -12.0, 12.0, 0.0);
        self.equalizer_gains = sanitize_gains(&self.equalizer_gains);
        self.crossfade_seconds =
            crate::sanitize::clamp_f32(self.crossfade_seconds, 0.0, 12.0, 3.0);
        self.equalizer_bass_boost_strength =
            crate::sanitize::clamp_i32(self.equalizer_bass_boost_strength, 0, 100);
        self.equalizer_virtualizer_strength =
            crate::sanitize::clamp_i32(self.equalizer_virtualizer_strength, 0, 100);
        self.equalizer_output_gain_mb =
            crate::sanitize::clamp_i32(self.equalizer_output_gain_mb, -1500, 1500);
        self.device_mute_recovery_volume =
            crate::sanitize::clamp_i32(self.device_mute_recovery_volume, 0, 130);
        self.equalizer_control_mode = self.equalizer_control_mode.trim().to_owned();
        self.equalizer_selected_profile_id =
            self.equalizer_selected_profile_id.trim().to_owned();
    }
}
