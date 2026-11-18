use serde::{Deserialize, Serialize};

use crate::de::or_default;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AiProvider {
    Chatgpt,
    Gemini,
    Openrouter,
    Custom,
    #[default]
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AiValidationStatus {
    #[default]
    Unknown,
    Success,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ai {
    #[serde(deserialize_with = "or_default")]
    pub provider: AiProvider,
    #[serde(deserialize_with = "or_default")]
    pub custom_endpoint: String,
    #[serde(deserialize_with = "or_default")]
    pub api_key: String,
    #[serde(deserialize_with = "or_default")]
    pub validation_status: AiValidationStatus,
    #[serde(deserialize_with = "or_default")]
    pub selected_model: String,
    #[serde(deserialize_with = "or_default")]
    pub custom_model: String,
    #[serde(deserialize_with = "or_default")]
    pub custom_prompt: String,
}

impl Default for Ai {
    fn default() -> Self {
        Self {
            provider: AiProvider::None,
            custom_endpoint: String::new(),
            api_key: String::new(),
            validation_status: AiValidationStatus::Unknown,
            selected_model: String::new(),
            custom_model: String::new(),
            custom_prompt: String::new(),
        }
    }
}

impl Ai {
    pub fn sanitize(&mut self) {
        self.custom_endpoint = self.custom_endpoint.trim().to_owned();
        self.api_key = self.api_key.trim().to_owned();
        self.selected_model = self.selected_model.trim().to_owned();
        self.custom_model = self.custom_model.trim().to_owned();
    }
}
