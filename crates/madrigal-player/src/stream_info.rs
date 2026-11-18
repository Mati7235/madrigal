use crate::engine::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Playing,
    Paused,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamMeta {
    pub media_id: String,
    pub loudness_hint: Option<f64>,
}

impl StreamMeta {
    pub fn new(media_id: impl Into<String>) -> Self {
        Self {
            media_id: media_id.into(),
            loudness_hint: None,
        }
    }

    pub fn with_loudness(media_id: impl Into<String>, loudness_hint: f64) -> Self {
        Self {
            media_id: media_id.into(),
            loudness_hint: Some(loudness_hint),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamInfo {
    pub codec: Option<String>,
    pub abr_kbps: Option<f64>,
    pub sample_rate: Option<u32>,
    pub media_id: String,
    pub loudness_hint: Option<f64>,
    pub buffer_health: Option<f64>,
    pub speed: f64,
    pub state: PlaybackState,
}

impl StreamInfo {
    pub fn snapshot(engine: &Engine, meta: &StreamMeta) -> Option<Self> {
        engine.prop_str("path")?;
        let paused = engine.paused().unwrap_or(true);
        Some(Self {
            codec: engine.prop_str("audio-codec-name"),
            abr_kbps: engine
                .prop_f64("audio-bitrate")
                .map(|bps| bps / 1000.0)
                .filter(|kbps| *kbps > 0.0),
            sample_rate: engine
                .prop_i64("audio-params/samplerate")
                .and_then(|rate| u32::try_from(rate).ok())
                .filter(|rate| *rate > 0),
            media_id: meta.media_id.clone(),
            loudness_hint: meta.loudness_hint,
            buffer_health: engine.prop_i64("cache-buffering-state").map(|v| v as f64),
            speed: engine.speed().unwrap_or(1.0),
            state: if paused {
                PlaybackState::Paused
            } else {
                PlaybackState::Playing
            },
        })
    }
}
