use std::time::{Duration, Instant};

pub const MAX_OFFSET_MS: i64 = 5_000;

#[derive(Clone, Copy, Debug)]
pub struct LyricsClock {
    position_ms: i64,
    anchor: Instant,
    playing: bool,
    offset_ms: i64,
}

impl LyricsClock {
    pub fn new(now: Instant) -> Self {
        Self { position_ms: 0, anchor: now, playing: false, offset_ms: 0 }
    }

    pub fn anchor(&mut self, position_ms: i64, now: Instant) {
        self.position_ms = position_ms.max(0);
        self.anchor = now;
    }

    pub fn set_playing(&mut self, playing: bool, now: Instant) {
        self.position_ms = self.position_at(now);
        self.anchor = now;
        self.playing = playing;
    }

    pub fn position_at(&self, now: Instant) -> i64 {
        let elapsed = now.saturating_duration_since(self.anchor).as_millis() as i64;
        let position = if self.playing { self.position_ms + elapsed } else { self.position_ms };
        position.max(0)
    }

    pub fn lyric_position_at(&self, now: Instant) -> i64 {
        (self.position_at(now) + self.offset_ms).max(0)
    }

    pub fn set_offset(&mut self, ms: i64) {
        self.offset_ms = ms.clamp(-MAX_OFFSET_MS, MAX_OFFSET_MS);
    }

    pub fn offset_ms(&self) -> i64 {
        self.offset_ms
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn pump(&mut self, by: Duration, now: Instant) {
        if self.playing {
            self.position_ms = (self.position_ms + by.as_millis() as i64).max(0);
        }
        self.anchor = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(ms: u64) -> Instant {
        Instant::now() + Duration::from_millis(ms)
    }

    fn clock_at(start: Instant, position_ms: i64) -> LyricsClock {
        let mut clock = LyricsClock::new(start);
        clock.anchor(position_ms, start);
        clock
    }

    #[test]
    fn paused_freezes() {
        let start = t(0);
        let clock = clock_at(start, 1_000);
        assert_eq!(clock.position_at(start + Duration::from_secs(30)), 1_000);
    }

    #[test]
    fn resume_continues_from_the_frozen_position() {
        let start = t(0);
        let mut clock = clock_at(start, 1_000);
        clock.set_playing(true, start);
        assert_eq!(clock.position_at(t(500)), 1_500);
        clock.set_playing(false, t(500));
        assert_eq!(clock.position_at(t(5_000)), 1_500);
        clock.set_playing(true, t(5_000));
        assert_eq!(clock.position_at(t(5_500)), 2_000);
    }

    #[test]
    fn re_anchor_corrects_drift() {
        let start = t(0);
        let mut clock = clock_at(start, 1_000);
        clock.set_playing(true, start);
        clock.anchor(3_000, t(500));
        assert_eq!(clock.position_at(t(1_000)), 3_500);
    }

    #[test]
    fn offset_applies_and_clamps() {
        let start = t(0);
        let mut clock = clock_at(start, 1_000);
        clock.set_offset(500);
        assert_eq!(clock.lyric_position_at(start), 1_500);
        assert_eq!(clock.position_at(start), 1_000);
        clock.set_offset(99_000);
        assert_eq!(clock.offset_ms(), MAX_OFFSET_MS);
        clock.set_offset(-99_000);
        assert_eq!(clock.offset_ms(), -MAX_OFFSET_MS);
    }

    #[test]
    fn seek_jumps() {
        let start = t(0);
        let mut clock = clock_at(start, 1_000);
        clock.set_playing(true, start);
        assert_eq!(clock.position_at(t(100)), 1_100);
        clock.anchor(60_000, t(100));
        assert_eq!(clock.position_at(t(200)), 60_100);
    }

    #[test]
    fn position_never_goes_negative() {
        let start = t(0);
        let mut clock = clock_at(start, 0);
        clock.set_offset(-MAX_OFFSET_MS);
        assert_eq!(clock.lyric_position_at(start), 0);
        clock.anchor(-2_000, start);
        assert_eq!(clock.position_at(start), 0);
    }

    #[test]
    fn pump_advances_only_while_playing() {
        let start = t(0);
        let mut clock = clock_at(start, 0);
        clock.pump(Duration::from_millis(16), start);
        assert_eq!(clock.position_at(start), 0);
        clock.set_playing(true, start);
        for step in 1..=125 {
            clock.pump(Duration::from_millis(16), start + Duration::from_millis(16 * step));
        }
        assert_eq!(clock.position_at(t(2_000)), 2_000);
    }
}
