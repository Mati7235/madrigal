use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::{LineMarkers, LyricLine, Lyrics, ParseError, WordTiming, secs_to_ms};

fn paragraph_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(r"(?is)<(?:\w+:)?p\b[^>]*>.*?</(?:\w+:)?p>").expect("ttml paragraph")
    })
}

fn attr_re(pattern: &'static str) -> Regex {
    Regex::new(pattern).expect("ttml attribute")
}

fn begin_value(opening: &str) -> Option<String> {
    attr_re(r##"(?i)\bbegin\s*=\s*["']([^"']+)["']"##)
        .captures(opening)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn end_value(opening: &str) -> Option<String> {
    attr_re(r##"(?i)\bend\s*=\s*["']([^"']+)["']"##)
        .captures(opening)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn key_value(opening: &str) -> Option<String> {
    attr_re(r##"(?i)\b(?:\w+:)?key\s*=\s*["']([^"']+)["']"##)
        .captures(opening)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn tag_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"<[^>]+>").expect("ttml tag"))
}

fn whitespace_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"\s+").expect("ttml whitespace"))
}

fn break_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"(?i)<(?:\w+:)?br\s*/?>").expect("ttml break"))
}

fn supplementary_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(
            r#"(?is)<(?:\w+:)?span\b[^>]*\brole\s*=\s*["']x-(?:bg|translation)["'][^>]*>.*?</(?:\w+:)?span>"#,
        )
        .expect("ttml supplementary")
    })
}

fn roman_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(
            r#"(?is)<(?:\w+:)?span\b[^>]*\brole\s*=\s*["']x-roman["'][^>]*>(.*?)</(?:\w+:)?span>"#,
        )
        .expect("ttml romanization")
    })
}

fn background_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(
            r#"(?is)<(?:\w+:)?span\b[^>]*\brole\s*=\s*["']x-bg["'][^>]*>(.*?)</(?:\w+:)?span>"#,
        )
        .expect("ttml background")
    })
}

fn translation_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(
            r#"(?is)<(?:\w+:)?span\b[^>]*\brole\s*=\s*["']x-translation["'][^>]*>(.*?)</(?:\w+:)?span>"#,
        )
        .expect("ttml translation")
    })
}

fn span_tag_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(r"(?i)<(/?)(?:\w+:)?span\b([^>]*)>").expect("ttml span tag")
    })
}

fn role_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(r#"(?i)\brole\s*=\s*["']([^"']+)["']"#).expect("ttml role")
    })
}

fn offset_time_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(r"(?i)^([0-9]+(?:\.[0-9]+)?)(ms|h|m|s|f|t)$").expect("ttml offset time")
    })
}

fn transliteration_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(
            r"(?is)<(?:\w+:)?transliteration\b([^>]*)>(.*?)</(?:\w+:)?transliteration>",
        )
        .expect("ttml transliteration")
    })
}

fn transliteration_text_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(r"(?is)<(?:\w+:)?text\b([^>]*)>(.*?)</(?:\w+:)?text>")
            .expect("ttml transliteration text")
    })
}

fn for_value(attributes: &str) -> Option<String> {
    attr_re(r##"(?i)\bfor\s*=\s*["']([^"']+)["']"##)
        .captures(attributes)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn lang_value(attributes: &str) -> Option<String> {
    attr_re(r##"(?i)\bxml:lang\s*=\s*["']([^"']+)["']"##)
        .captures(attributes)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

pub fn looks_like_ttml(text: &str) -> bool {
    let head: String = text.chars().take(4096).collect();
    Regex::new(r"(?i)<(?:\w+:)?tt\b")
        .expect("ttml root")
        .is_match(&head)
}

pub fn parse_time(value: &str) -> Option<f64> {
    let raw = value.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some(caps) = offset_time_re().captures(raw) {
        let amount: f64 = caps.get(1)?.as_str().parse().ok()?;
        match caps.get(2)?.as_str().to_lowercase().as_str() {
            "ms" => return Some(amount / 1000.0),
            "m" => return Some(amount * 60.0),
            "h" => return Some(amount * 3600.0),
            "f" | "t" => return Some(amount / 30.0),
            _ => return Some(amount),
        }
    }
    let normalized = raw.replace(';', ":");
    let parts: Vec<&str> = normalized.split(':').collect();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    let mut numbers = Vec::with_capacity(parts.len());
    for part in parts {
        numbers.push(part.parse::<f64>().ok()?);
    }
    Some(match numbers.len() {
        1 => numbers[0],
        2 => numbers[0] * 60.0 + numbers[1],
        3 => numbers[0] * 3600.0 + numbers[1] * 60.0 + numbers[2],
        _ => numbers[0] * 3600.0 + numbers[1] * 60.0 + numbers[2] + numbers[3] / 30.0,
    })
}

pub(crate) fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest.find(';') else {
            break;
        };
        if semi > 32 {
            out.push('&');
            rest = &rest[1..];
            continue;
        }
        let entity = &rest[1..semi];
        let resolved = if let Some(hex) = entity
            .strip_prefix("#x")
            .or_else(|| entity.strip_prefix("#X"))
        {
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else if let Some(digits) = entity.strip_prefix('#') {
            digits.parse::<u32>().ok().and_then(char::from_u32)
        } else {
            match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => None,
            }
        };
        match resolved {
            Some(ch) => {
                out.push(ch);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

pub(crate) fn collapse(text: &str) -> String {
    whitespace_re()
        .replace_all(text, " ")
        .trim()
        .to_string()
}

fn paragraph_inner(paragraph: &str) -> &str {
    let start = paragraph.find('>').map(|pos| pos + 1).unwrap_or(0);
    let end = paragraph.rfind('<').unwrap_or(paragraph.len());
    paragraph.get(start..end.max(start)).unwrap_or("")
}

fn strip_fixpoint(body: &str, pattern: &'static Regex) -> String {
    let mut current = body.to_string();
    loop {
        let next = pattern.replace_all(&current, "").to_string();
        if next == current {
            return next;
        }
        current = next;
    }
}

fn line_text(paragraph: &str) -> String {
    let mut body = strip_fixpoint(paragraph_inner(paragraph), supplementary_re());
    body = strip_fixpoint(&body, roman_re());
    let with_breaks = break_re().replace_all(&body, " ").to_string();
    let stripped = tag_re().replace_all(&with_breaks, "").to_string();
    collapse(&unescape(&stripped))
}

fn cleaned_text(fragment: &str) -> String {
    let with_breaks = break_re().replace_all(fragment, " ").to_string();
    let stripped = tag_re().replace_all(&with_breaks, "").to_string();
    collapse(&unescape(&stripped))
}

fn marker_texts(paragraph: &str, pattern: &'static Regex) -> Vec<String> {
    pattern
        .captures_iter(paragraph_inner(paragraph))
        .filter_map(|caps| caps.get(1).map(|m| cleaned_text(m.as_str())))
        .filter(|text| !text.is_empty())
        .collect()
}

fn romanization(paragraph: &str) -> String {
    roman_re()
        .captures_iter(paragraph_inner(paragraph))
        .filter_map(|caps| caps.get(1).map(|m| cleaned_text(m.as_str())))
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn transliterations(document: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for block in transliteration_re().captures_iter(document) {
        let (Some(attrs), Some(body)) = (block.get(1), block.get(2)) else {
            continue;
        };
        let lang = lang_value(attrs.as_str()).unwrap_or_default();
        if lang.to_lowercase() != "ja-latn" {
            continue;
        }
        for entry in transliteration_text_re().captures_iter(body.as_str()) {
            let (Some(attrs), Some(value)) = (entry.get(1), entry.get(2)) else {
                continue;
            };
            let (Some(key), cleaned) = (
                for_value(attrs.as_str()),
                cleaned_text(value.as_str()),
            ) else {
                continue;
            };
            if !cleaned.is_empty() {
                map.insert(key, cleaned);
            }
        }
    }
    map
}

fn skip_role(attributes: &str) -> bool {
    role_re()
        .captures(attributes)
        .and_then(|caps| caps.get(1))
        .is_some_and(|role| matches!(role.as_str().to_lowercase().as_str(), "x-bg" | "x-translation" | "x-roman"))
}

fn paragraph_words(paragraph: &str) -> Vec<WordTiming> {
    let body = paragraph_inner(paragraph);
    let mut frames: Vec<(bool, Option<usize>)> = Vec::new();
    let mut tokens: Vec<(String, Option<usize>)> = Vec::new();
    let mut words: Vec<(i64, Option<i64>)> = Vec::new();
    let mut last = 0;
    let emit = |fragment: &str,
                    frames: &[(bool, Option<usize>)],
                    tokens: &mut Vec<(String, Option<usize>)>| {
        if fragment.is_empty() || frames.iter().any(|frame| frame.0) {
            return;
        }
        let owner = frames
            .iter()
            .rev()
            .find_map(|frame| frame.1);
        tokens.push((unescape(fragment), owner));
    };
    for hit in span_tag_re().find_iter(body) {
        let between = break_re()
            .replace_all(&body[last..hit.start()], " ")
            .to_string();
        let between = tag_re().replace_all(&between, "").to_string();
        emit(&between, &frames, &mut tokens);
        last = hit.end();
        let caps = span_tag_re()
            .captures(hit.as_str())
            .expect("ttml span attributes");
        if caps.get(1).is_some_and(|slash| !slash.as_str().is_empty()) {
            frames.pop();
            continue;
        }
        let attributes = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let skip = frames.iter().any(|frame| frame.0) || skip_role(attributes);
        let mut word = None;
        if !skip {
            if let Some(begin) = begin_value(attributes).and_then(|raw| parse_time(&raw)) {
                let end = end_value(attributes).and_then(|raw| parse_time(&raw));
                words.push((
                    secs_to_ms(begin),
                    end.map(secs_to_ms),
                ));
                word = Some(words.len() - 1);
            }
        }
        frames.push((skip, word));
    }
    let tail = break_re().replace_all(&body[last..], " ").to_string();
    let tail = tag_re().replace_all(&tail, "").to_string();
    emit(&tail, &frames, &mut tokens);
    if words.is_empty() {
        return Vec::new();
    }
    let mut buckets: Vec<Vec<String>> = vec![Vec::new(); words.len()];
    let mut trailing: Vec<String> = Vec::new();
    let mut carry: Option<usize> = None;
    for (fragment, owner) in tokens {
        match owner {
            None => match carry {
                Some(slot) => buckets[slot].push(fragment),
                None => trailing.push(fragment),
            },
            Some(slot) => {
                buckets[slot].push(fragment);
                carry = Some(slot);
            }
        }
    }
    if !trailing.is_empty() {
        buckets[0].insert(0, trailing.concat());
    }
    words
        .into_iter()
        .zip(buckets)
        .filter_map(|((start_ms, end_ms), bucket)| {
            let text = collapse(&bucket.concat());
            (!text.is_empty()).then_some(WordTiming {
                start_ms,
                end_ms,
                text,
            })
        })
        .collect()
}

pub fn parse(document: &str) -> Result<Lyrics, ParseError> {
    if document.trim().is_empty() {
        return Err(ParseError::EmptyInput);
    }
    let latin = transliterations(document);
    let mut lines = Vec::new();
    for hit in paragraph_re().find_iter(document) {
        let paragraph = hit.as_str();
        let Some(gt) = paragraph.find('>') else {
            continue;
        };
        let opening = &paragraph[..gt + 1];
        let Some(start_ms) = begin_value(opening)
            .and_then(|raw| parse_time(&raw))
            .map(secs_to_ms)
        else {
            continue;
        };
        let text = line_text(paragraph);
        if text.is_empty() {
            continue;
        }
        let end_ms = end_value(opening)
            .and_then(|raw| parse_time(&raw))
            .map(secs_to_ms);
        let mut marker_roman = romanization(paragraph);
        if marker_roman.is_empty() {
            if let Some(key) = key_value(opening) {
                if let Some(found) = latin.get(&key) {
                    marker_roman = found.clone();
                }
            }
        }
        let background = marker_texts(paragraph, background_re());
        let translations = marker_texts(paragraph, translation_re());
        let words = paragraph_words(paragraph);
        lines.push(LyricLine {
            start_ms,
            end_ms,
            words: (!words.is_empty()).then_some(words),
            markers: LineMarkers {
                background,
                translation: translations.into_iter().next(),
                romanization: (!marker_roman.is_empty()).then_some(marker_roman),
            },
            text,
        });
    }
    if lines.is_empty() {
        return Err(ParseError::NoSyncedLines);
    }
    Ok(Lyrics { lines, synced: true })
}
