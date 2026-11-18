pub mod mixes;
pub mod providers;
pub mod translation;
pub mod validate;

use std::fmt;
use std::time::Duration;

pub const OPENAI_BASE: &str = "https://api.openai.com/v1";
pub const OPENROUTER_BASE: &str = "https://openrouter.ai/api/v1";
pub const GEMINI_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";
pub const DEFAULT_CHATGPT_MODEL: &str = "gpt-4o";
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-3.5-flash";
pub const DEFAULT_OPENROUTER_MODEL: &str = "~openai/gpt-latest";
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NotConfigured(&'static str),
    Transport(String),
    Api(u16, String),
    EmptyResponse,
    MismatchedLines { expected: usize, got: usize },
    Unparseable(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConfigured(reason) => write!(formatter, "ai provider is not configured: {reason}"),
            Self::Transport(reason) => write!(formatter, "ai transport error: {reason}"),
            Self::Api(status, reason) => write!(formatter, "ai api failed ({status}): {reason}"),
            Self::EmptyResponse => formatter.write_str("ai api returned an empty response"),
            Self::MismatchedLines { expected, got } => {
                write!(formatter, "translated {got} lines, expected {expected}")
            }
            Self::Unparseable(reason) => write!(formatter, "{reason}"),
        }
    }
}

impl std::error::Error for Error {}

pub fn transport_error(error: reqwest::Error) -> Error {
    Error::Transport(error.without_url().to_string())
}

pub fn api_error(status: u16, raw: &str) -> Error {
    let message = read_error_message(raw).unwrap_or_else(|| format!("http {status}"));
    let trimmed: String = message.chars().take(200).collect();
    Error::Api(status, trimmed)
}

fn read_error_message(raw: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    if let Some(message) = value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(serde_json::Value::as_str)
    {
        if !message.trim().is_empty() {
            return Some(message.trim().to_owned());
        }
    }
    value
        .get("error")
        .and_then(serde_json::Value::as_str)
        .filter(|message| !message.trim().is_empty())
        .map(|message| message.trim().to_owned())
}
