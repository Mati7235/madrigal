use serde::{Deserialize, Deserializer, Serialize};

use crate::de::or_default;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UpdateChannel {
    #[default]
    Stable,
    Artifact,
}

impl<'de> Deserialize<'de> for UpdateChannel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "STABLE" => Self::Stable,
            "ARTIFACT" => Self::Artifact,
            "NIGHTLY" | "DAILY_NIGHTLY" | "CANARY" => Self::Artifact,
            _ => Self::Stable,
        })
    }
}

impl Serialize for UpdateChannel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            Self::Stable => "STABLE",
            Self::Artifact => "ARTIFACT",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Updates {
    #[serde(deserialize_with = "or_default")]
    pub automatic_update_check: bool,
    #[serde(deserialize_with = "or_default")]
    pub enable_update_notification: bool,
    #[serde(deserialize_with = "or_default")]
    pub update_channel: UpdateChannel,
    #[serde(deserialize_with = "or_default")]
    pub last_update_check: i64,
    #[serde(deserialize_with = "or_default")]
    pub last_notified_version: String,
    #[serde(deserialize_with = "or_default")]
    pub redownload_on_restore: bool,
    #[serde(deserialize_with = "or_default")]
    pub launch_count: i32,
    #[serde(deserialize_with = "or_default")]
    pub onboarding_completed: bool,
    #[serde(deserialize_with = "or_default")]
    pub has_pressed_star: bool,
    #[serde(deserialize_with = "or_default")]
    pub remind_after: i32,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            automatic_update_check: true,
            enable_update_notification: true,
            update_channel: UpdateChannel::Stable,
            last_update_check: 0,
            last_notified_version: String::new(),
            redownload_on_restore: true,
            launch_count: 0,
            onboarding_completed: false,
            has_pressed_star: false,
            remind_after: 0,
        }
    }
}

impl Updates {
    pub fn sanitize(&mut self) {
        if self.last_update_check < 0 {
            self.last_update_check = 0;
        }
        if self.launch_count < 0 {
            self.launch_count = 0;
        }
        if self.remind_after < 0 {
            self.remind_after = 0;
        }
        self.last_notified_version = self.last_notified_version.trim().to_owned();
    }
}
