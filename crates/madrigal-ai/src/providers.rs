use std::time::Duration;

use madrigal_settings::{Ai, AiProvider};
use serde::{Deserialize, Serialize};

use crate::{api_error, transport_error, Error, HTTP_TIMEOUT};
use crate::{DEFAULT_CHATGPT_MODEL, DEFAULT_GEMINI_MODEL, DEFAULT_OPENROUTER_MODEL};
use crate::{GEMINI_BASE, OPENAI_BASE, OPENROUTER_BASE};

#[derive(Clone)]
pub struct ChatRequest {
    pub system: String,
    pub user: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

impl ChatRequest {
    pub fn new(system: &str, user: &str, temperature: f32, max_tokens: u32) -> Self {
        Self {
            system: system.to_owned(),
            user: user.to_owned(),
            temperature,
            max_tokens,
        }
    }
}

pub trait ChatModel {
    fn complete(&self, request: &ChatRequest) -> Result<String, Error>;
}

#[derive(Clone)]
pub struct AiConfig {
    pub provider: AiProvider,
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
}

impl AiConfig {
    pub fn from_settings(ai: &Ai) -> Result<Self, Error> {
        if ai.provider == AiProvider::None {
            return Err(Error::NotConfigured("ai provider is disabled"));
        }
        if ai.api_key.trim().is_empty() {
            return Err(Error::NotConfigured("ai api key is missing"));
        }
        let selected = ai.selected_model.trim();
        let custom = ai.custom_model.trim();
        let (provider, endpoint, model) = match ai.provider {
            AiProvider::Chatgpt => (
                AiProvider::Chatgpt,
                format!("{OPENAI_BASE}/chat/completions"),
                nonempty_or(selected, DEFAULT_CHATGPT_MODEL),
            ),
            AiProvider::Openrouter => (
                AiProvider::Openrouter,
                format!("{OPENROUTER_BASE}/chat/completions"),
                nonempty_or(selected, DEFAULT_OPENROUTER_MODEL),
            ),
            AiProvider::Gemini => (
                AiProvider::Gemini,
                GEMINI_BASE.to_owned(),
                nonempty_or(selected, DEFAULT_GEMINI_MODEL),
            ),
            AiProvider::Custom => {
                if ai.custom_endpoint.trim().is_empty() {
                    return Err(Error::NotConfigured("custom endpoint is missing"));
                }
                if custom.is_empty() {
                    return Err(Error::NotConfigured("no ai model configured"));
                }
                (
                    AiProvider::Custom,
                    ai.custom_endpoint.trim().to_owned(),
                    custom.to_owned(),
                )
            }
            AiProvider::None => return Err(Error::NotConfigured("ai provider is disabled")),
        };
        Ok(Self {
            provider,
            endpoint,
            api_key: ai.api_key.trim().to_owned(),
            model,
        })
    }
}

fn nonempty_or(value: &str, fallback: &str) -> String {
    if value.is_empty() {
        fallback.to_owned()
    } else {
        value.to_owned()
    }
}

fn http_client(timeout: Duration) -> Result<reqwest::blocking::Client, Error> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(transport_error)
}

#[derive(Clone)]
pub struct OpenAiChat {
    endpoint: String,
    api_key: String,
    model: String,
    timeout: Duration,
}

impl OpenAiChat {
    pub fn new(endpoint: &str, api_key: &str, model: &str) -> Self {
        Self {
            endpoint: endpoint.to_owned(),
            api_key: api_key.to_owned(),
            model: model.to_owned(),
            timeout: HTTP_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl ChatModel for OpenAiChat {
    fn complete(&self, request: &ChatRequest) -> Result<String, Error> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": request.system},
                {"role": "user", "content": request.user},
            ],
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
        });
        let response = http_client(self.timeout)?
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .map_err(transport_error)?;
        let status = response.status().as_u16();
        let raw = response.text().map_err(transport_error)?;
        if !(200..300).contains(&status) {
            return Err(api_error(status, &raw));
        }
        let value: serde_json::Value =
            serde_json::from_str(&raw).map_err(|_| Error::EmptyResponse)?;
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|content| !content.is_empty())
            .map(str::to_owned)
            .ok_or(Error::EmptyResponse)
    }
}

#[derive(Clone)]
pub struct GeminiChat {
    base: String,
    api_key: String,
    model: String,
    timeout: Duration,
}

impl GeminiChat {
    pub fn new(base: &str, api_key: &str, model: &str) -> Self {
        Self {
            base: base.to_owned(),
            api_key: api_key.to_owned(),
            model: model.to_owned(),
            timeout: HTTP_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl ChatModel for GeminiChat {
    fn complete(&self, request: &ChatRequest) -> Result<String, Error> {
        let url = format!("{}/models/{}:generateContent", self.base, self.model);
        let body = serde_json::json!({
            "contents": [{"parts": [{"text": format!("{}\n\n{}", request.system, request.user)}]}],
            "generationConfig": {
                "temperature": request.temperature,
                "maxOutputTokens": request.max_tokens,
            },
        });
        let response = http_client(self.timeout)?
            .post(&url)
            .query(&[("key", &self.api_key)])
            .json(&body)
            .send()
            .map_err(transport_error)?;
        let status = response.status().as_u16();
        let raw = response.text().map_err(transport_error)?;
        if !(200..300).contains(&status) {
            return Err(api_error(status, &raw));
        }
        let value: serde_json::Value =
            serde_json::from_str(&raw).map_err(|_| Error::EmptyResponse)?;
        value
            .get("candidates")
            .and_then(|candidates| candidates.get(0))
            .and_then(|candidate| candidate.get("content"))
            .and_then(|content| content.get("parts"))
            .and_then(|parts| parts.get(0))
            .and_then(|part| part.get("text"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|content| !content.is_empty())
            .map(str::to_owned)
            .ok_or(Error::EmptyResponse)
    }
}

pub fn chat_model(config: &AiConfig) -> Box<dyn ChatModel> {
    match config.provider {
        AiProvider::Gemini => Box::new(GeminiChat::new(
            &config.endpoint,
            &config.api_key,
            &config.model,
        )),
        _ => Box::new(OpenAiChat::new(
            &config.endpoint,
            &config.api_key,
            &config.model,
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelOption {
    pub id: String,
    pub display: String,
}

pub fn list_models(config: &AiConfig) -> Result<Vec<ModelOption>, Error> {
    match config.provider {
        AiProvider::Chatgpt | AiProvider::Openrouter => {
            let base = config
                .endpoint
                .strip_suffix("/chat/completions")
                .unwrap_or(&config.endpoint);
            let url = format!("{base}/models");
            let response = http_client(HTTP_TIMEOUT)?
                .get(&url)
                .bearer_auth(&config.api_key)
                .send()
                .map_err(transport_error)?;
            let status = response.status().as_u16();
            let raw = response.text().map_err(transport_error)?;
            if !(200..300).contains(&status) {
                return Err(api_error(status, &raw));
            }
            let value: serde_json::Value =
                serde_json::from_str(&raw).map_err(|_| Error::EmptyResponse)?;
            let mut models = value
                .get("data")
                .and_then(serde_json::Value::as_array)
                .map(|data| {
                    data.iter()
                        .filter_map(|entry| entry.get("id")?.as_str())
                        .filter(|id| !id.trim().is_empty())
                        .map(|id| ModelOption {
                            id: id.to_owned(),
                            display: id.to_owned(),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            models.sort_by(|a, b| a.id.cmp(&b.id));
            Ok(models)
        }
        AiProvider::Gemini => {
            let url = format!("{}/models", config.endpoint);
            let response = http_client(HTTP_TIMEOUT)?
                .get(&url)
                .query(&[("key", &config.api_key)])
                .send()
                .map_err(transport_error)?;
            let status = response.status().as_u16();
            let raw = response.text().map_err(transport_error)?;
            if !(200..300).contains(&status) {
                return Err(api_error(status, &raw));
            }
            let value: serde_json::Value =
                serde_json::from_str(&raw).map_err(|_| Error::EmptyResponse)?;
            let mut models = value
                .get("models")
                .and_then(serde_json::Value::as_array)
                .map(|data| {
                    data.iter()
                        .filter(|entry| supports_generate(entry))
                        .filter_map(|entry| {
                            entry
                                .get("name")?
                                .as_str()?
                                .strip_prefix("models/")
                                .filter(|id| !id.trim().is_empty())
                                .map(|id| ModelOption {
                                    id: id.to_owned(),
                                    display: entry
                                        .get("displayName")
                                        .and_then(serde_json::Value::as_str)
                                        .filter(|name| !name.trim().is_empty())
                                        .unwrap_or(id)
                                        .to_owned(),
                                })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            models.sort_by(|a, b| a.id.cmp(&b.id));
            Ok(models)
        }
        AiProvider::Custom | AiProvider::None => Ok(Vec::new()),
    }
}

fn supports_generate(entry: &serde_json::Value) -> bool {
    entry
        .get("supportedGenerationMethods")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|methods| {
            methods
                .iter()
                .any(|method| method.as_str() == Some("generateContent"))
        })
}
