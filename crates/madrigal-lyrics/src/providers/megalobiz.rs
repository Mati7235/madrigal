use regex::Regex;
use reqwest::Client;
use std::sync::OnceLock;

use crate::matching::{matches_artist, matches_title, similarity};
use crate::providers::{Provider, ProviderError, ProviderResult, Query, get_text, parse_text};
use crate::providers::kugou::percent_encode;

const CONTEXT_WINDOW: usize = 600;
const MIN_SCORE: f64 = 0.5;

fn link_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(r#"href=["'](/lrc/maker/download/[^"']+)["']"#).expect("megalobiz link")
    })
}

fn span_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(r#"(?s)id=["']lrc_[^"']*_details["'][^>]*>(.*?)</span>"#)
            .expect("megalobiz span")
    })
}

fn tag_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"<[^>]+>").expect("megalobiz tag"))
}

pub struct MegalobizProvider {
    base_url: String,
}

impl MegalobizProvider {
    pub fn new() -> Self {
        Self {
            base_url: "https://www.megalobiz.com".to_owned(),
        }
    }

    pub fn with_base_url(base: impl Into<String>) -> Self {
        Self {
            base_url: base.into(),
        }
    }

    fn score_context(context: &str, query: &Query) -> f64 {
        let title_sim = similarity(&query.title, context);
        let artist_sim = if query.artist().is_empty() {
            1.0
        } else {
            similarity(query.artist(), context)
        };
        (title_sim + artist_sim) / 2.0
    }

    fn pick_link(page: &str, query: &Query) -> Option<String> {
        let mut best: Option<(f64, String)> = None;
        for hit in link_re().captures_iter(page) {
            let link = hit.get(1)?.as_str().to_owned();
            let start = hit.get(0)?.start();
            let from = start.saturating_sub(CONTEXT_WINDOW);
            let to = (start + CONTEXT_WINDOW).min(page.len());
            let context = page.get(from..to).unwrap_or_default();
            let context_ok = matches_title(&query.title, context, 0.6)
                && matches_artist(query.artist(), context);
            let score = Self::score_context(context, query) + if context_ok { 0.2 } else { 0.0 };
            if score >= MIN_SCORE && best.as_ref().is_none_or(|(top, _)| score > *top) {
                best = Some((score, link));
            }
        }
        best.map(|(_, link)| link)
    }

    pub fn clean(text: &str) -> String {
        let staged = text.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&");
        let mut with_breaks = staged.clone();
        for br in ["<br>", "<br/>", "<br />", "<BR>", "<BR/>", "<BR />"] {
            with_breaks = with_breaks.replace(br, "\n");
        }
        let stripped = tag_re().replace_all(&with_breaks, "");
        html_unescape(&stripped).trim().to_owned()
    }
}

fn html_unescape(text: &str) -> String {
    let mut out = text.replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'");
    for (entity, ch) in [("&amp;", "&"), ("&lt;", "<"), ("&gt;", ">"), ("&nbsp;", " ")] {
        out = out.replace(entity, ch);
    }
    let num = Regex::new(r"&#(\d+);").expect("numeric entity");
    out = num
        .replace_all(&out, |caps: &regex::Captures| {
            caps.get(1)
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .and_then(char::from_u32)
                .map(|c| c.to_string())
                .unwrap_or_default()
        })
        .into_owned();
    out
}

impl Default for MegalobizProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for MegalobizProvider {
    fn name(&self) -> &'static str {
        "megalobiz"
    }

    async fn fetch(&self, client: &Client, query: &Query) -> ProviderResult {
        let artist = query.artist().trim();
        let raw_query = if artist.is_empty() {
            query.title.trim().to_owned()
        } else {
            format!("{artist} {}", query.title.trim())
        };
        let search_url = format!("{}/searchall?qry={}", self.base_url, percent_encode(&raw_query));
        let search = match get_text(client, &search_url, &[]).await? {
            Some(text) => text,
            None => return Ok(None),
        };
        let Some(link) = Self::pick_link(&search, query) else {
            return Ok(None);
        };
        let detail_url = format!("{}{link}", self.base_url);
        let detail = match get_text(client, &detail_url, &[]).await? {
            Some(text) => text,
            None => return Ok(None),
        };
        let body = span_re()
            .captures(&detail)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str())
            .unwrap_or(&detail);
        let cleaned = Self::clean(body);
        let parsed = parse_text(&cleaned).ok_or_else(|| ProviderError::Json("empty lyrics".to_owned()))?;
        if parsed.synced && !parsed.lines.is_empty() {
            Ok(Some(parsed))
        } else {
            Ok(None)
        }
    }
}
