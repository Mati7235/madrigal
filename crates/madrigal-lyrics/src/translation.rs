use std::fmt;

use crate::Lyrics;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationError {
    EmptyTarget,
    EmptyInput,
    NoProvider,
    MismatchedLines { expected: usize, got: usize },
    Provider(String),
}

impl fmt::Display for TranslationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTarget => formatter.write_str("missing target language"),
            Self::EmptyInput => formatter.write_str("no lines to translate"),
            Self::NoProvider => formatter.write_str("no translation provider"),
            Self::MismatchedLines { expected, got } => {
                write!(formatter, "translated {got} lines, expected {expected}")
            }
            Self::Provider(message) => write!(formatter, "translation failed: {message}"),
        }
    }
}

impl std::error::Error for TranslationError {}

pub trait TranslationProvider {
    fn name(&self) -> &str;
    fn translate_lines(
        &self,
        lines: &[String],
        target: &str,
    ) -> Result<Vec<String>, TranslationError>;
}

pub fn translate_chain(
    providers: &[&dyn TranslationProvider],
    lines: &[String],
    target: &str,
) -> Result<Vec<String>, TranslationError> {
    if target.trim().is_empty() {
        return Err(TranslationError::EmptyTarget);
    }
    if lines.is_empty() {
        return Err(TranslationError::EmptyInput);
    }
    let mut last = TranslationError::NoProvider;
    for provider in providers {
        match provider.translate_lines(lines, target.trim()) {
            Ok(translated) if translated.len() == lines.len() => return Ok(translated),
            Ok(translated) => {
                last = TranslationError::MismatchedLines {
                    expected: lines.len(),
                    got: translated.len(),
                };
            }
            Err(error) => last = error,
        }
    }
    Err(last)
}

pub fn attach_translation(
    lyrics: &Lyrics,
    translated: &[String],
) -> Result<Lyrics, TranslationError> {
    if translated.len() != lyrics.lines.len() {
        return Err(TranslationError::MismatchedLines {
            expected: lyrics.lines.len(),
            got: translated.len(),
        });
    }
    let mut out = lyrics.clone();
    for (line, text) in out.lines.iter_mut().zip(translated.iter()) {
        line.markers.translation = Some(text.clone());
    }
    Ok(out)
}

pub fn translate_lyrics(
    lyrics: &Lyrics,
    providers: &[&dyn TranslationProvider],
    target: &str,
) -> Result<Lyrics, TranslationError> {
    let source: Vec<String> = lyrics.lines.iter().map(|line| line.text.clone()).collect();
    let translated = translate_chain(providers, &source, target)?;
    attach_translation(lyrics, &translated)
}

pub fn encode_cache(target: &str, lines: &[String]) -> String {
    serde_json::json!({"target": target, "lines": lines}).to_string()
}

pub fn decode_cache(raw: &str) -> Option<(String, Vec<String>)> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let target = value.get("target")?.as_str()?.to_string();
    let lines = value
        .get("lines")?
        .as_array()?
        .iter()
        .map(|entry| entry.as_str().map(str::to_string))
        .collect::<Option<Vec<_>>>()?;
    Some((target, lines))
}
