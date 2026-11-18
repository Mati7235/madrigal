use madrigal_lyrics::translation::TranslationError;
use madrigal_lyrics::translation::TranslationProvider;

use crate::providers::{ChatModel, ChatRequest};

pub fn system_prompt(target: &str, count: usize, custom: &str) -> String {
    let base = format!(
        "You are an expert song lyrics translator.\n\
         Translate each input string into {target} with natural, accurate lyric phrasing.\n\
         Preserve meaning, tone, profanity level, names, repeated hooks, and line-level intent.\n\
         Do not add timestamps, IDs, XML, markdown, explanations, or extra lines.\n\
         Return only a JSON array of strings with exactly {count} items in the same order."
    );
    let extra = custom.trim();
    if extra.is_empty() {
        base
    } else {
        format!("{base}\n\n{extra}")
    }
}

pub fn extract_json_array(raw: &str) -> Result<Vec<String>, TranslationError> {
    let trimmed = raw
        .trim()
        .strip_prefix("```json")
        .or_else(|| raw.trim().strip_prefix("```"))
        .unwrap_or(raw.trim());
    let trimmed = trimmed.strip_suffix("```").unwrap_or(trimmed).trim();
    let (start, end) = match (trimmed.find('['), trimmed.rfind(']')) {
        (Some(start), Some(end)) if end > start => (start, end),
        _ => {
            return Err(TranslationError::Provider(
                "ai response did not contain a json array".to_owned(),
            ));
        }
    };
    let value: serde_json::Value = serde_json::from_str(&trimmed[start..=end])
        .map_err(|_| {
            TranslationError::Provider("ai response did not contain a json array".to_owned())
        })?;
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .ok_or_else(|| {
            TranslationError::Provider("ai response did not contain a json array".to_owned())
        })
}

pub struct AiTranslationProvider<'a> {
    model: &'a dyn ChatModel,
    custom_prompt: String,
}

impl<'a> AiTranslationProvider<'a> {
    pub fn new(model: &'a dyn ChatModel, custom_prompt: &str) -> Self {
        Self {
            model,
            custom_prompt: custom_prompt.to_owned(),
        }
    }
}

impl TranslationProvider for AiTranslationProvider<'_> {
    fn name(&self) -> &str {
        "ai"
    }

    fn translate_lines(
        &self,
        lines: &[String],
        target: &str,
    ) -> Result<Vec<String>, TranslationError> {
        if target.trim().is_empty() {
            return Err(TranslationError::EmptyTarget);
        }
        if lines.is_empty() {
            return Err(TranslationError::EmptyInput);
        }
        let target = target.trim();
        let user = serde_json::to_string(lines)
            .map_err(|error| TranslationError::Provider(error.to_string()))?;
        let request = ChatRequest::new(
            &system_prompt(target, lines.len(), &self.custom_prompt),
            &user,
            0.15,
            8192,
        );
        let raw = self
            .model
            .complete(&request)
            .map_err(|error| TranslationError::Provider(error.to_string()))?;
        let translated = extract_json_array(&raw)?;
        if translated.len() != lines.len() {
            return Err(TranslationError::MismatchedLines {
                expected: lines.len(),
                got: translated.len(),
            });
        }
        Ok(translated)
    }
}
