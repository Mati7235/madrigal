use serde::{Deserialize, Serialize};

use crate::de::or_default;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Privacy {
    #[serde(deserialize_with = "or_default")]
    pub pause_listen_history: bool,
    #[serde(deserialize_with = "or_default")]
    pub pause_search_history: bool,
    #[serde(deserialize_with = "or_default")]
    pub disable_screenshot: bool,
    #[serde(deserialize_with = "or_default")]
    pub history_duration_days: i32,
    #[serde(deserialize_with = "or_default")]
    pub ai_content_filter_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub ai_content_filter_include_moderate: bool,
    #[serde(deserialize_with = "or_default")]
    pub ai_content_filter_last_updated: i64,
    #[serde(deserialize_with = "or_default")]
    pub ai_content_filter_source_url: String,
}

impl Default for Privacy {
    fn default() -> Self {
        Self {
            pause_listen_history: false,
            pause_search_history: false,
            disable_screenshot: false,
            history_duration_days: 30,
            ai_content_filter_enabled: false,
            ai_content_filter_include_moderate: false,
            ai_content_filter_last_updated: 0,
            ai_content_filter_source_url: String::new(),
        }
    }
}

impl Privacy {
    pub fn sanitize(&mut self) {
        self.history_duration_days =
            crate::sanitize::clamp_i32(self.history_duration_days, 5, 60);
        if self.ai_content_filter_last_updated < 0 {
            self.ai_content_filter_last_updated = 0;
        }
        self.ai_content_filter_source_url = self.ai_content_filter_source_url.trim().to_owned();
    }
}
