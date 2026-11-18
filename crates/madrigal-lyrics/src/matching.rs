use regex::Regex;
use std::sync::OnceLock;
use unicode_normalization::UnicodeNormalization;

static NOISE_RE: OnceLock<Regex> = OnceLock::new();
static FEAT_RE: OnceLock<Regex> = OnceLock::new();
static SPLIT_RE: OnceLock<Regex> = OnceLock::new();
static PLAIN_RE: OnceLock<Regex> = OnceLock::new();
static TITLE_CLEAN_RE: OnceLock<Regex> = OnceLock::new();

fn noise_re() -> &'static Regex {
    NOISE_RE.get_or_init(|| {
        Regex::new(r"(?i)\((?:official|lyric|audio|video|visualizer|hd|hq|4k|remaster(?:ed)?|mv|m/v)[^)]*\)|\[(?:official|lyric|audio|video|visualizer|hd|hq|4k|mv|m/v)[^\]]*\]|\b(?:official|lyric|audio|video|visualizer)\b").expect("noise re")
    })
}

fn feat_re() -> &'static Regex {
    FEAT_RE.get_or_init(|| {
        Regex::new(r"(?i)\s*[\(\[]?\s*(?:feat|ft|with)\.?\s+[^\)\]]*[\)\]]?").expect("feat re")
    })
}

fn split_re() -> &'static Regex {
    SPLIT_RE.get_or_init(|| Regex::new(r"[^0-9a-z]+").expect("split re"))
}

fn plain_re() -> &'static Regex {
    PLAIN_RE.get_or_init(|| Regex::new(r"[\s\W_]+").expect("plain re"))
}

fn title_clean_re() -> &'static Regex {
    TITLE_CLEAN_RE.get_or_init(|| {
        Regex::new(r"\(.*\)|（.*）|「.*」|『.*』|<.*>|《.*》|〈.*〉|＜.*＞").expect("title clean re")
    })
}

const NOISE_TOKENS: &[&str] = &["the", "and", "feat", "with", "ver", "version"];

pub fn clean_title(raw: &str) -> String {
    let no_feat = feat_re().replace_all(raw, "");
    let no_noise = noise_re().replace_all(&no_feat, "");
    no_noise
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c| c == '-' || c == '–' || c == '—')
        .trim()
        .to_owned()
}

pub fn normalize_query_title(raw: &str) -> String {
    title_clean_re().replace_all(raw, "").trim().to_owned()
}

pub fn normalize_query_artist(raw: &str) -> String {
    let staged = raw
        .replace(", ", "\u{3001}")
        .replace(" & ", "\u{3001}")
        .replace('.', "")
        .replace('\u{548c}', "\u{3001}");
    let no_paren = Regex::new(r"\(.*\)|（.*）").expect("artist paren re");
    no_paren.replace_all(&staged, "").trim().to_owned()
}

fn ascii_fold(text: &str) -> String {
    text.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase()
}

pub fn tokens(text: &str) -> Vec<String> {
    split_re()
        .split(&ascii_fold(text))
        .filter(|part| part.len() > 1 && !NOISE_TOKENS.contains(part))
        .map(str::to_owned)
        .collect()
}

fn plain(text: &str) -> String {
    plain_re()
        .replace_all(&text.to_lowercase(), "")
        .into_owned()
}

pub fn matches_title(expected: &str, candidate: &str, minimum: f64) -> bool {
    let wanted = tokens(expected);
    let found = tokens(candidate);
    if wanted.is_empty() || found.is_empty() {
        let left = plain(expected);
        let right = plain(candidate);
        if left.len() < 2 || right.len() < 2 {
            return false;
        }
        return left.contains(&right) || right.contains(&left);
    }
    let hits = wanted.iter().filter(|t| found.contains(t)).count();
    hits as f64 / wanted.len() as f64 >= minimum
}

pub fn matches_artist(expected: &str, candidate: &str) -> bool {
    let wanted = tokens(expected);
    let found = tokens(candidate);
    if wanted.is_empty() || found.is_empty() {
        return true;
    }
    wanted.iter().any(|t| found.contains(t))
}

pub fn similarity(left: &str, right: &str) -> f64 {
    let first = left.trim().to_lowercase();
    let second = right.trim().to_lowercase();
    if first == second {
        return 1.0;
    }
    if first.is_empty() || second.is_empty() {
        return 0.0;
    }
    if first.contains(&second) || second.contains(&first) {
        return 0.8;
    }
    let longest = first.len().max(second.len()) as f64;
    1.0 - levenshtein(&first, &second) as f64 / longest
}

fn levenshtein(first: &str, second: &str) -> usize {
    let left: Vec<char> = first.chars().collect();
    let right: Vec<char> = second.chars().collect();
    let mut prev: Vec<usize> = (0..=right.len()).collect();
    for (i, a) in left.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, b) in right.iter().enumerate() {
            let cost = usize::from(a != b);
            cur.push((prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost));
        }
        prev = cur;
    }
    prev[right.len()]
}

pub fn within_secs(left: Option<u32>, right: Option<u32>, tolerance: u32) -> bool {
    match (left, right) {
        (Some(a), Some(b)) => a.abs_diff(b) <= tolerance,
        _ => false,
    }
}
