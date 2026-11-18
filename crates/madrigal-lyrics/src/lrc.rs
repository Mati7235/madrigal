use regex::Regex;
use std::sync::OnceLock;

use crate::{LineMarkers, LyricLine, Lyrics, WordTiming, secs_to_ms};

fn timestamp_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(r"\[(\d{1,3}):(\d{1,2}(?:[.:]\d{1,3})?)\]").expect("lrc timestamp")
    })
}

fn word_mark_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(r"<(\d{1,3}):(\d{1,2}(?:[.:]\d{1,3})?)>").expect("lrc word mark")
    })
}

fn metadata_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(r"(?i)^\[(ar|ti|al|au|by|length|offset|re|ve):").expect("lrc metadata")
    })
}

fn offset_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(r"(?im)^\[offset:\s*([+-]?\d+)\s*\]").expect("lrc offset")
    })
}

pub(crate) fn to_seconds(minutes: &str, rest: &str) -> Option<f64> {
    let mins: f64 = minutes.parse().ok()?;
    let secs: f64 = rest.replace(':', ".").parse().ok()?;
    Some(mins * 60.0 + secs)
}

pub(crate) fn extract_offset_ms(input: &str) -> i64 {
    offset_re()
        .captures_iter(input)
        .filter_map(|caps| caps.get(1).map(|m| m.as_str().parse::<i64>().unwrap_or(0)))
        .last()
        .unwrap_or(0)
}

pub(crate) fn split_words(content: &str) -> (String, Option<Vec<WordTiming>>) {
    let marks: Vec<(usize, usize, f64)> = word_mark_re()
        .captures_iter(content)
        .filter_map(|caps| {
            let whole = caps.get(0)?;
            let seconds = to_seconds(caps.get(1)?.as_str(), caps.get(2)?.as_str())?;
            Some((whole.start(), whole.end(), seconds))
        })
        .collect();
    if marks.is_empty() {
        return (content.to_string(), None);
    }
    let mut texts: Vec<(i64, String)> = Vec::with_capacity(marks.len());
    for (index, (_, end, seconds)) in marks.iter().enumerate() {
        let stop = marks
            .get(index + 1)
            .map(|next| next.0)
            .unwrap_or(content.len());
        texts.push((secs_to_ms(*seconds), content[*end..stop].to_string()));
    }
    let lead = content[..marks[0].0].to_string();
    if !lead.trim().is_empty() {
        texts[0].1 = format!("{lead}{}", texts[0].1);
    }
    let text = texts
        .iter()
        .filter_map(|(_, piece)| {
            let trimmed = piece.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect::<Vec<_>>()
        .join(" ");
    let mut words = Vec::new();
    for (start_ms, piece) in texts {
        let trimmed = piece.trim();
        if !trimmed.is_empty() {
            words.push(WordTiming {
                start_ms,
                end_ms: None,
                text: trimmed.to_string(),
            });
        }
    }
    if words.is_empty() {
        return (text, None);
    }
    (text, Some(words))
}

pub(crate) fn strip_word_marks(content: &str) -> String {
    word_mark_re().replace_all(content, "").to_string()
}

pub(crate) fn parse_core(input: &str, with_words: bool) -> Lyrics {
    let offset_ms = extract_offset_ms(input);
    let mut lines: Vec<LyricLine> = Vec::new();
    let mut synced = false;
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || metadata_re().is_match(line) {
            continue;
        }
        let mut stamps: Vec<i64> = Vec::new();
        for caps in timestamp_re().captures_iter(line) {
            let (Some(minutes), Some(rest)) = (caps.get(1), caps.get(2)) else {
                continue;
            };
            if let Some(seconds) = to_seconds(minutes.as_str(), rest.as_str()) {
                stamps.push(secs_to_ms(seconds) + offset_ms);
            }
        }
        let content = timestamp_re().replace_all(line, "").trim().to_string();
        if !stamps.is_empty() {
            synced = true;
            let (text, words) = if with_words {
                split_words(&content)
            } else {
                (strip_word_marks(&content).trim().to_string(), None)
            };
            for start_ms in stamps {
                lines.push(LyricLine {
                    start_ms,
                    end_ms: None,
                    text: text.clone(),
                    words: words.clone(),
                    markers: LineMarkers::default(),
                });
            }
        } else if !content.is_empty() {
            let text = if with_words {
                split_words(&content).0
            } else {
                strip_word_marks(&content).trim().to_string()
            };
            lines.push(LyricLine {
                start_ms: 0,
                end_ms: None,
                text,
                words: None,
                markers: LineMarkers::default(),
            });
        }
    }
    if synced {
        lines.sort_by_key(|line| line.start_ms);
        let mut deduped: Vec<LyricLine> = Vec::with_capacity(lines.len());
        for line in lines {
            let duplicate = deduped.last().is_some_and(|prev: &LyricLine| {
                prev.start_ms == line.start_ms && prev.text == line.text
            });
            if !duplicate {
                deduped.push(line);
            }
        }
        lines = deduped;
    }
    Lyrics { lines, synced }
}

pub fn parse(input: &str) -> Lyrics {
    parse_core(input, false)
}
