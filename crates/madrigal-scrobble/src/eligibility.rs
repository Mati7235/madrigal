use madrigal_core::Event;
use madrigal_settings::Integrations;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Eligibility {
    pub delay_fraction: f32,
    pub delay_secs: u64,
    pub min_duration_secs: u64,
}

impl Default for Eligibility {
    fn default() -> Self {
        Self {
            delay_fraction: 0.5,
            delay_secs: 180,
            min_duration_secs: 30,
        }
    }
}

impl Eligibility {
    pub fn from_integrations(integrations: &Integrations) -> Self {
        Self {
            delay_fraction: integrations.scrobble_delay_percent / 100.0,
            delay_secs: integrations.scrobble_delay_secs.max(0) as u64,
            min_duration_secs: integrations.scrobble_min_song_duration_secs.max(0) as u64,
        }
    }

    fn threshold_float(&self, duration_secs: u64) -> Option<f64> {
        if duration_secs <= self.min_duration_secs {
            return None;
        }
        let scaled = duration_secs as f64 * self.delay_fraction.max(0.0) as f64;
        Some(scaled.min(self.delay_secs as f64))
    }

    pub fn threshold_secs(&self, duration_secs: u64) -> Option<u64> {
        self.threshold_float(duration_secs)
            .map(|limit| limit as u64)
    }

    pub fn eligible(&self, duration_secs: u64, played_secs: u64) -> bool {
        match self.threshold_float(duration_secs) {
            Some(limit) => played_secs as f64 >= limit,
            None => false,
        }
    }

    pub fn event_threshold(&self, event: &Event) -> Option<u64> {
        event
            .duration_secs
            .and_then(|duration| self.threshold_secs(duration as u64))
    }
}
