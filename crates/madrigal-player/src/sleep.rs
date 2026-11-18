use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepMode {
    Minutes(u64),
    EndOfSong,
}

pub struct SleepTimer {
    mode: SleepMode,
    armed: bool,
    deadline: Option<Instant>,
}

impl SleepTimer {
    pub fn minutes(mins: u64) -> Self {
        Self {
            mode: SleepMode::Minutes(mins),
            armed: false,
            deadline: None,
        }
    }

    pub fn end_of_song() -> Self {
        Self {
            mode: SleepMode::EndOfSong,
            armed: false,
            deadline: None,
        }
    }

    pub fn mode(&self) -> SleepMode {
        self.mode
    }

    pub fn arm(&mut self, now: Instant) {
        self.armed = true;
        self.deadline = match self.mode {
            SleepMode::Minutes(mins) => now.checked_add(Duration::from_secs(mins * 60)),
            SleepMode::EndOfSong => None,
        };
    }

    pub fn cancel(&mut self) {
        self.armed = false;
        self.deadline = None;
    }

    pub fn is_armed(&self) -> bool {
        self.armed
    }

    pub fn poll(&mut self, now: Instant) -> bool {
        if !self.armed {
            return false;
        }
        match self.deadline {
            Some(deadline) if now >= deadline => {
                self.armed = false;
                self.deadline = None;
                true
            }
            _ => false,
        }
    }

    pub fn on_track_end(&mut self) -> bool {
        if self.armed && self.mode == SleepMode::EndOfSong {
            self.armed = false;
            true
        } else {
            false
        }
    }

    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        if !self.armed {
            return None;
        }
        self.deadline.map(|deadline| deadline.saturating_duration_since(now))
    }
}
