use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use madrigal_core::Track;
use reqwest::blocking::Client;
use reqwest::header::{ORIGIN, REFERER, USER_AGENT};

use crate::{CanvasEntry, CanvasSource, Error};

pub const AMP_BASE: &str = "https://amp-api.music.apple.com";
pub const PAGE_BASE: &str = "https://music.apple.com";
const MIN_SCORE: i32 = 12;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|span| span.as_secs())
        .unwrap_or(0)
}

pub struct Fetcher {
    amp_base: String,
    page_base: String,
    token: Mutex<Option<WebToken>>,
}

struct WebToken {
    value: String,
    expires_at: u64,
}

impl Default for Fetcher {
    fn default() -> Fetcher {
        Fetcher::new()
    }
}

impl Fetcher {
    pub fn new() -> Fetcher {
        Fetcher {
            amp_base: AMP_BASE.to_owned(),
            page_base: PAGE_BASE.to_owned(),
            token: Mutex::new(None),
        }
    }

    pub fn with_bases(amp_base: impl Into<String>, page_base: impl Into<String>) -> Fetcher {
        Fetcher {
            amp_base: amp_base.into(),
            page_base: page_base.into(),
            token: Mutex::new(None),
        }
    }

    pub fn fetch(&self, http: &Client, track: &Track) -> Result<Option<CanvasEntry>, Error> {
        let artist = track.artist_text();
        let album = track.album.as_deref();
        self.search_motion(http, &track.title, &artist, album, "songs")
    }

    pub fn fetch_album(&self, http: &Client, album_id: &str) -> Result<Option<CanvasEntry>, Error> {
        self.album_motion(http, album_id, None, None, None)
    }

    fn web_token(&self, http: &Client, rejected: Option<&str>) -> Result<String, Error> {
        if let Ok(guard) = self.token.lock() {
            if let Some(cached) = guard.as_ref() {
                if Some(cached.value.as_str()) != rejected && cached.expires_at > now_secs() + 300 {
                    return Ok(cached.value.clone());
                }
            }
        }
        let page_url = format!("{}/us/new", self.page_base);
        let page = get_text(http, &page_url)?;
        if let Some(found) = find_token(&page) {
            if Some(found.value.as_str()) != rejected {
                let value = found.value.clone();
                if let Ok(mut guard) = self.token.lock() {
                    *guard = Some(found);
                }
                return Ok(value);
            }
        }
        for script in script_urls(&page_url, &page).into_iter().take(3) {
            if let Some(found) = get_text(http, &script).ok().and_then(|body| find_token(&body)) {
                let value = found.value.clone();
                if let Ok(mut guard) = self.token.lock() {
                    *guard = Some(found);
                }
                return Ok(value);
            }
        }
        Err(Error::Protocol(
            "apple music web catalog token is unavailable".to_owned(),
        ))
    }

    fn catalog_get(
        &self,
        http: &Client,
        url: &str,
        params: &[(&str, &str)],
    ) -> Result<reqwest::blocking::Response, Error> {
        let token = self.web_token(http, None)?;
        let response = catalog_request(http, url, params, &token)?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            let fresh = self.web_token(http, Some(&token))?;
            return catalog_request(http, url, params, &fresh);
        }
        Ok(response)
    }

    fn search_motion(
        &self,
        http: &Client,
        term: &str,
        artist: &str,
        album: Option<&str>,
        kind: &str,
    ) -> Result<Option<CanvasEntry>, Error> {
        let mut query = if contains_ci(term, artist) {
            term.to_owned()
        } else {
            format!("{artist} {term}")
        };
        if let Some(album) = album.filter(|album| !album.trim().is_empty()) {
            if !contains_ci(&query, album) {
                query = format!("{query} {album}");
            }
        }
        let url = format!("{}/v1/catalog/us/search", self.amp_base);
        let response = self.catalog_get(
            http,
            &url,
            &[
                ("term", &query),
                ("types", kind),
                ("limit", "10"),
                ("extend", "editorialVideo"),
                ("include", "albums"),
            ],
        )?;
        if !response.status().is_success() {
            return Err(Error::Status(response.status().as_u16()));
        }
        let root: serde_json::Value = response
            .json()
            .map_err(|err| Error::Protocol(err.to_string()))?;
        let items = root
            .get("results")
            .and_then(|value| value.get(kind))
            .and_then(|value| value.get("data"))
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let mut scored: Vec<(i32, serde_json::Value)> = items
            .into_iter()
            .filter_map(|item| score_item(&item, term, artist, album).map(|score| (score, item)))
            .collect();
        scored.sort_by_key(|(score, _)| -*score);
        for (score, item) in scored {
            if score < MIN_SCORE {
                continue;
            }
            let attributes = match item.get("attributes") {
                Some(attributes) => attributes,
                None => continue,
            };
            let artist_name = str_of(attributes, "artistName").unwrap_or_default();
            let item_type = str_of(&item, "type");
            let album_id = match resolve_album_id(&item, attributes, item_type.as_deref()) {
                Some(id) if !id.starts_with("pl.") => id,
                _ => continue,
            };
            if let Some(direct) = entry_of(
                attributes,
                &album_id,
                title_override(item_type.as_deref(), attributes, &artist_name),
            ) {
                return Ok(Some(direct));
            }
            let title_override = match item_type.as_deref() {
                Some("songs") => str_of(attributes, "name"),
                _ => None,
            };
            let artist_override = match item_type.as_deref() {
                Some("songs") => Some(artist_name.clone()),
                _ => None,
            };
            if let Some(fetched) =
                self.album_motion(http, &album_id, Some(&artist_name), title_override.as_deref(), artist_override.as_deref())?
            {
                return Ok(Some(fetched));
            }
        }
        Ok(None)
    }

    fn album_motion(
        &self,
        http: &Client,
        album_id: &str,
        fallback_artist: Option<&str>,
        title_override: Option<&str>,
        artist_override: Option<&str>,
    ) -> Result<Option<CanvasEntry>, Error> {
        if album_id.starts_with("pl.") {
            return Ok(None);
        }
        let url = format!("{}/v1/catalog/us/albums/{album_id}", self.amp_base);
        let response = self.catalog_get(
            http,
            &url,
            &[("extend", "editorialVideo"), ("include", "tracks")],
        )?;
        if !response.status().is_success() {
            return Err(Error::Status(response.status().as_u16()));
        }
        let root: serde_json::Value = response
            .json()
            .map_err(|err| Error::Protocol(err.to_string()))?;
        let album = root
            .get("data")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let attributes = album.get("attributes").cloned().unwrap_or_default();
        let album_name = str_of(&attributes, "name").unwrap_or_default();
        if is_blacklisted(&album_name) {
            return Ok(None);
        }
        let artist_name = str_of(&attributes, "artistName").or_else(|| fallback_artist.map(str::to_owned));
        let title = title_override.map(str::to_owned).unwrap_or_else(|| album_name.clone());
        let artist = artist_override.map(str::to_owned).or(artist_name);
        Ok(entry_of(
            &attributes,
            album_id,
            Some((title, artist.unwrap_or_default(), album_name)),
        ))
    }
}

fn title_override(
    item_type: Option<&str>,
    attributes: &serde_json::Value,
    artist_name: &str,
) -> Option<(String, String, String)> {
    let direct = attributes.get("editorialVideo")?;
    let urls = editorial_urls(direct);
    if urls.0.is_none() && urls.1.is_none() {
        return None;
    }
    let name = str_of(attributes, "name");
    let collection = str_of(attributes, "collectionName");
    let album_name = match item_type {
        Some("songs") => collection,
        _ => name.clone(),
    };
    Some((name.unwrap_or_default(), artist_name.to_owned(), album_name.unwrap_or_default()))
}

fn entry_of(
    attributes: &serde_json::Value,
    album_id: &str,
    resolved: Option<(String, String, String)>,
) -> Option<CanvasEntry> {
    let (title, artist, album_name) = resolved?;
    let urls = editorial_urls(attributes.get("editorialVideo")?);
    if urls.0.is_none() && urls.1.is_none() {
        return None;
    }
    Some(CanvasEntry {
        source: CanvasSource::AppleMusic,
        name: Some(title),
        artist: Some(artist),
        album_id: Some(album_id.to_owned()),
        album_name: Some(album_name),
        static_url: None,
        animated: urls.0,
        animated_vertical: urls.1,
        video_url: None,
        video_url_vertical: None,
    })
}

fn editorial_urls(ev: &serde_json::Value) -> (Option<String>, Option<String>) {
    fn video(value: &serde_json::Value) -> Option<String> {
        for key in ["video", "videoUrl", "hlsUrl", "url"] {
            if let Some(url) = value.get(key).and_then(|value| value.as_str()) {
                if !url.trim().is_empty() {
                    return Some(url.to_owned());
                }
            }
        }
        None
    }
    let get = |key: &str| ev.get(key).and_then(video);
    let animated = get("motionDetailRaw")
        .or_else(|| get("motionDetailSquare"))
        .or_else(|| get("motionDetailStatic"))
        .or_else(|| get("motionDetailTall"));
    (animated, get("motionDetailTall"))
}

fn str_of(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_owned)
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn is_blacklisted(name: &str) -> bool {
    let lower = name.to_lowercase();
    [
        "playlist",
        "set list",
        "essentials",
        "dj mix",
        "mixed",
        "apple music",
        "today's hits",
        "session",
    ]
    .iter()
    .any(|token| lower.contains(token))
}

fn score_item(
    item: &serde_json::Value,
    term: &str,
    artist: &str,
    album: Option<&str>,
) -> Option<i32> {
    let attributes = item.get("attributes")?;
    let artist_name = str_of(attributes, "artistName").unwrap_or_default();
    let name = str_of(attributes, "name").unwrap_or_default();
    let collection = str_of(attributes, "collectionName").unwrap_or_default();
    if is_blacklisted(&name) || is_blacklisted(&collection) {
        return None;
    }
    if !contains_ci(&artist_name, artist) && !contains_ci(artist, &artist_name) {
        return None;
    }
    let mut score = if artist_name.eq_ignore_ascii_case(artist) {
        10
    } else {
        5
    };
    score += if name.eq_ignore_ascii_case(term) {
        15
    } else if contains_ci(&name, term) || contains_ci(term, &name) {
        7
    } else {
        -10
    };
    for word in ["deluxe", "expanded", "remastered", "remix", "version", "edit", "mix", "bonus"] {
        let in_term = contains_ci(term, word);
        let in_result = contains_ci(&name, word);
        score += match (in_term, in_result) {
            (true, true) => 5,
            (false, true) => -3,
            _ => 0,
        };
    }
    if let Some(album) = album.filter(|album| !album.trim().is_empty()) {
        if !collection.is_empty() {
            score += if collection.eq_ignore_ascii_case(album) {
                20
            } else if contains_ci(&collection, album) || contains_ci(album, &collection) {
                10
            } else {
                0
            };
        }
    }
    Some(score)
}

fn resolve_album_id(
    item: &serde_json::Value,
    attributes: &serde_json::Value,
    item_type: Option<&str>,
) -> Option<String> {
    match item_type {
        Some("albums") => str_of(item, "id"),
        Some("songs") => {
            let via_relationships = item
                .get("relationships")
                .and_then(|value| value.get("albums"))
                .and_then(|value| value.get("data"))
                .and_then(|value| value.as_array())
                .and_then(|items| items.first())
                .and_then(|first| first.get("id"))
                .and_then(|value| value.as_str())
                .map(str::to_owned);
            if via_relationships.is_some() {
                return via_relationships;
            }
            if let Some(collection) = str_of(attributes, "collectionId") {
                return Some(collection);
            }
            let url = str_of(attributes, "url")?;
            let after = url.split("/album/").nth(1)?.split('?').next()?;
            let id = after.rsplit('/').next()?;
            if !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()) {
                Some(id.to_owned())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn get_text(http: &Client, url: &str) -> Result<String, Error> {
    let response = http
        .get(url)
        .header(USER_AGENT, "Mozilla/5.0")
        .header(reqwest::header::CACHE_CONTROL, "no-cache")
        .send()
        .map_err(|err| Error::Transport(err.to_string()))?;
    if !response.status().is_success() {
        return Err(Error::Status(response.status().as_u16()));
    }
    response
        .text()
        .map_err(|err| Error::Protocol(err.to_string()))
}

fn catalog_request(
    http: &Client,
    url: &str,
    params: &[(&str, &str)],
    token: &str,
) -> Result<reqwest::blocking::Response, Error> {
    http.get(url)
        .bearer_auth(token)
        .header(ORIGIN, "https://music.apple.com")
        .header(REFERER, "https://music.apple.com/")
        .header(
            USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .query(params)
        .send()
        .map_err(|err| Error::Transport(err.to_string()))
}

fn find_token(text: &str) -> Option<WebToken> {
    for candidate in jwt_candidates(text) {
        let mut parts = candidate.split('.');
        let (_, payload, _) = (parts.next()?, parts.next()?, parts.next()?);
        let decoded = base64_url_decode(payload)?;
        let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
        if json.get("iss").and_then(|value| value.as_str()) != Some("AMPWebPlay") {
            continue;
        }
        let expiry = json.get("exp").and_then(|value| value.as_u64())?;
        if expiry > now_secs() + 300 {
            return Some(WebToken {
                value: candidate,
                expires_at: expiry,
            });
        }
    }
    None
}

fn jwt_candidates(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 20 < bytes.len() {
        if text[i..].starts_with("eyJ") {
            let mut end = i + 3;
            while end < bytes.len() && is_jwt_char(bytes[end]) {
                end += 1;
            }
            let candidate = &text[i..end];
            if candidate.split('.').count() == 3 && candidate.len() > 32 {
                out.push(candidate.to_owned());
            }
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

fn is_jwt_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.' || byte == b'='
}

fn base64_url_decode(input: &str) -> Option<Vec<u8>> {
    let clean: String = input.chars().filter(|c| *c != '=').collect();
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u8;
    for c in clean.chars() {
        let value = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '-' | '+' => 62,
            '_' | '/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn script_urls(page_url: &str, page: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = page;
    while let Some(start) = rest.find("<script") {
        let tag = &rest[start..];
        let end = match tag.find('>') {
            Some(end) => end,
            None => break,
        };
        let header = &tag[..end];
        rest = &tag[end + 1..];
        let src = match src_of(header) {
            Some(src) => src,
            None => continue,
        };
        let resolved = resolve_url(page_url, &src);
        if resolved.starts_with("https://music.apple.com/assets/index") {
            out.push(resolved);
        }
    }
    out
}

fn src_of(tag: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("src={quote}");
        if let Some(start) = tag.find(&needle) {
            let after = &tag[start + needle.len()..];
            if let Some(end) = after.find(quote) {
                return Some(after[..end].to_owned());
            }
        }
    }
    None
}

fn resolve_url(page_url: &str, src: &str) -> String {
    if src.starts_with("https://") || src.starts_with("http://") {
        return src.to_owned();
    }
    if let Some(scheme_end) = page_url.find("://") {
        let scheme = &page_url[..scheme_end];
        let rest = &page_url[scheme_end + 3..];
        let host = rest.split('/').next().unwrap_or_default();
        if let Some(path) = src.strip_prefix("//") {
            return format!("{scheme}://{path}");
        }
        if let Some(path) = src.strip_prefix('/') {
            return format!("{scheme}://{host}/{path}");
        }
        return format!("{scheme}://{host}/{src}");
    }
    src.to_owned()
}
