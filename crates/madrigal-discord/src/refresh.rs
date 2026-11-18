use madrigal_core::Track;

pub const DEFAULT_DEBOUNCE_MS: u64 = 2000;

#[derive(Debug, Clone, Copy)]
pub struct PlaybackContext<'a> {
    pub track: Option<&'a Track>,
    pub is_playing: bool,
    pub now_ms: u64,
}

pub struct RefreshGate {
    interval_ms: u64,
    last_track_id: Option<Option<String>>,
    last_playing: bool,
    last_emit_ms: u64,
}

impl RefreshGate {
    pub fn new(interval_ms: u64) -> Self {
        Self {
            interval_ms,
            last_track_id: None,
            last_playing: false,
            last_emit_ms: 0,
        }
    }

    pub fn decide(&mut self, context: PlaybackContext<'_>) -> bool {
        let track_id = context.track.map(|track| track.identity.id.clone());
        let changed = self.last_track_id.as_ref().is_none_or(|previous| {
            *previous != track_id || self.last_playing != context.is_playing
        });
        if changed {
            self.last_track_id = Some(track_id);
            self.last_playing = context.is_playing;
            self.last_emit_ms = context.now_ms;
            return true;
        }
        if context.now_ms.saturating_sub(self.last_emit_ms) >= self.interval_ms {
            self.last_emit_ms = context.now_ms;
            return true;
        }
        false
    }
}
