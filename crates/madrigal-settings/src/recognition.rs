use serde::{Deserialize, Serialize};

use crate::de::or_default;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Recognition {
    #[serde(deserialize_with = "or_default")]
    pub enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub capture_seconds: u32,
    #[serde(deserialize_with = "or_default")]
    pub min_interval_ms: u64,
    #[serde(deserialize_with = "or_default")]
    pub max_attempts: u32,
}

impl Default for Recognition {
    fn default() -> Self {
        Self {
            enabled: false,
            capture_seconds: 8,
            min_interval_ms: 1000,
            max_attempts: 3,
        }
    }
}

impl Recognition {
    pub fn sanitize(&mut self) {
        self.capture_seconds = self.capture_seconds.clamp(3, 30);
        self.min_interval_ms = self.min_interval_ms.clamp(250, 10_000);
        self.max_attempts = self.max_attempts.clamp(1, 5);
    }
}
