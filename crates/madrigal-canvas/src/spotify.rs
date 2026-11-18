use madrigal_core::Track;

use crate::{CanvasEntry, CanvasSource, Error, SpotifyCanvasAuth};

pub const SEARCH_URL: &str = "https://api-partner.spotify.com/pathfinder/v2/query";
pub const SEARCH_OPERATION: &str = "searchDesktop";
pub const CANVAS_URL: &str = "https://spclient.wg.spotify.com/canvaz-cache/v0/canvases";
pub const CLIENT_TOKEN_URL: &str = "https://clienttoken.spotify.com/v1/clienttoken";
pub const OPEN_PAGE_URL: &str = "https://open.spotify.com/";
pub const WEB_USER_AGENT: &str = "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36";
pub const APP_USER_AGENT: &str = "Spotify/9.0.34.593 iOS/18.4 (iPhone15,3)";

#[derive(Debug, Clone)]
pub struct Credentials {
    pub access_token: String,
    pub client_id: String,
    pub search_hashes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Bases {
    pub search_url: String,
    pub canvas_url: String,
    pub client_token_url: String,
    pub page_url: String,
}

impl Bases {
    pub fn production() -> Bases {
        Bases {
            search_url: SEARCH_URL.to_owned(),
            canvas_url: CANVAS_URL.to_owned(),
            client_token_url: CLIENT_TOKEN_URL.to_owned(),
            page_url: OPEN_PAGE_URL.to_owned(),
        }
    }
}

pub struct Fetcher<'a> {
    pub bases: Bases,
    pub auth: Option<&'a dyn SpotifyCanvasAuth>,
}

impl<'a> Fetcher<'a> {
    pub fn offline() -> Fetcher<'a> {
        Fetcher {
            bases: Bases::default(),
            auth: None,
        }
    }

    pub fn production(auth: &'a dyn SpotifyCanvasAuth) -> Fetcher<'a> {
        Fetcher {
            bases: Bases::production(),
            auth: Some(auth),
        }
    }

    pub fn credentials(&self) -> Result<Credentials, Error> {
        match self.auth {
            Some(auth) => auth.canvas_credentials(),
            None => Err(Error::Unsupported(
                "spotify canvas needs the spotifycore session (pending F10-4)".to_owned(),
            )),
        }
    }
}

pub fn valid_track_uri(uri: &str) -> bool {
    let tail = match uri.strip_prefix("spotify:track:") {
        Some(tail) => tail,
        None => return false,
    };
    tail.len() == 22
        && tail
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric())
}

pub fn is_canvas_url(value: &str) -> bool {
    let rest = match value.strip_prefix("https://") {
        Some(rest) => rest,
        None => return false,
    };
    let host_end = rest.find('/').unwrap_or(rest.len());
    if &rest[..host_end] != "canvaz.scdn.co" {
        return false;
    }
    rest.ends_with(".mp4")
}

pub fn encode_request(track_uris: &[String]) -> Result<Vec<u8>, Error> {
    if track_uris.len() > 10 {
        return Err(Error::Protocol("too many spotify canvas uris".to_owned()));
    }
    let mut out = Vec::new();
    for uri in track_uris {
        if !valid_track_uri(uri) {
            return Err(Error::Protocol("invalid spotify track uri".to_owned()));
        }
        let mut inner = Vec::new();
        write_string(&mut inner, 1, uri);
        write_bytes(&mut out, 1, &inner);
    }
    Ok(out)
}

pub fn decode_response(bytes: &[u8]) -> Result<Vec<(String, String)>, Error> {
    if bytes.len() > 1_048_576 {
        return Err(Error::Protocol("spotify canvas response is too large".to_owned()));
    }
    let mut pairs = Vec::new();
    let mut input = bytes;
    while !input.is_empty() {
        let (field, wire, rest) = read_tag(input)?;
        input = rest;
        if field == 1 && wire == 2 {
            let (body, rest) = read_bytes(input)?;
            input = rest;
            let mut inner = body.as_slice();
            let mut uri: Option<String> = None;
            let mut url: Option<String> = None;
            while !inner.is_empty() {
                let (field, wire, rest) = read_tag(inner)?;
                inner = rest;
                if wire != 2 {
                    skip_field(&mut inner, wire)?;
                    continue;
                }
                let (text, rest) = read_string(inner)?;
                inner = rest;
                match field {
                    2 => url = Some(text),
                    5 => uri = Some(text),
                    _ => {}
                }
            }
            if let (Some(uri), Some(url)) = (uri, url) {
                if is_canvas_url(&url) {
                    pairs.push((uri, url));
                }
            }
        } else {
            skip_field(&mut input, wire)?;
        }
    }
    Ok(pairs)
}

fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8 & 0x7F) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn write_string(out: &mut Vec<u8>, field: u32, value: &str) {
    write_varint(out, (field as u64) << 3 | 2);
    write_varint(out, value.len() as u64);
    out.extend_from_slice(value.as_bytes());
}

fn write_bytes(out: &mut Vec<u8>, field: u32, value: &[u8]) {
    write_varint(out, (field as u64) << 3 | 2);
    write_varint(out, value.len() as u64);
    out.extend_from_slice(value);
}

fn read_varint(input: &[u8]) -> Result<(u64, &[u8]), Error> {
    let mut value = 0u64;
    let mut shift = 0u32;
    for (index, byte) in input.iter().enumerate() {
        if shift >= 64 {
            return Err(Error::Protocol("invalid spotify canvas field".to_owned()));
        }
        value |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok((value, &input[index + 1..]));
        }
        shift += 7;
    }
    Err(Error::Protocol("invalid spotify canvas response".to_owned()))
}

fn read_tag(input: &[u8]) -> Result<(u32, u8, &[u8]), Error> {
    let (tag, rest) = read_varint(input)?;
    Ok(((tag >> 3) as u32, (tag & 7) as u8, rest))
}

fn read_bytes(input: &[u8]) -> Result<(Vec<u8>, &[u8]), Error> {
    let (len, rest) = read_varint(input)?;
    let len = len as usize;
    if rest.len() < len {
        return Err(Error::Protocol("invalid spotify canvas response".to_owned()));
    }
    Ok((rest[..len].to_vec(), &rest[len..]))
}

fn read_string(input: &[u8]) -> Result<(String, &[u8]), Error> {
    let (bytes, rest) = read_bytes(input)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| Error::Protocol("invalid spotify canvas field".to_owned()))?;
    Ok((text, rest))
}

fn skip_field(input: &mut &[u8], wire: u8) -> Result<(), Error> {
    match wire {
        0 => {
            let (_, rest) = read_varint(input)?;
            *input = rest;
            Ok(())
        }
        1 => {
            if input.len() < 8 {
                return Err(Error::Protocol("invalid spotify canvas response".to_owned()));
            }
            *input = &input[8..];
            Ok(())
        }
        2 => {
            let (_, rest) = read_bytes(input)?;
            *input = rest;
            Ok(())
        }
        5 => {
            if input.len() < 4 {
                return Err(Error::Protocol("invalid spotify canvas response".to_owned()));
            }
            *input = &input[4..];
            Ok(())
        }
        _ => Err(Error::Protocol("invalid spotify canvas field".to_owned())),
    }
}

fn track_of(item: &serde_json::Value, uri_override: Option<String>) -> Option<(String, CanvasEntry)> {
    let uri = uri_override
        .or_else(|| item.get("uri").and_then(|value| value.as_str()).map(str::to_owned))
        .or_else(|| {
            item.get("id")
                .and_then(|value| value.as_str())
                .map(|id| format!("spotify:track:{id}"))
        })?;
    if !valid_track_uri(&uri) {
        return None;
    }
    let title = item.get("name").and_then(|value| value.as_str())?;
    let mut credits = Vec::new();
    if let Some(items) = item
        .get("artists")
        .and_then(|value| value.get("items"))
        .and_then(|value| value.as_array())
    {
        for artist in items {
            if let Some(name) = artist
                .get("profile")
                .and_then(|value| value.get("name"))
                .and_then(|value| value.as_str())
            {
                credits.push(name.to_owned());
            }
        }
    }
    let album = item.get("albumOfTrack");
    let image = album
        .and_then(|value| value.get("coverArt"))
        .and_then(|value| value.get("sources"))
        .and_then(|value| value.as_array())
        .map(|sources| {
            sources
                .iter()
                .filter_map(|source| {
                    let width = source.get("width").and_then(|value| value.as_i64()).unwrap_or(640);
                    let url = source.get("url").and_then(|value| value.as_str())?;
                    Some((width, url.to_owned()))
                })
                .min_by_key(|(width, _)| (width - 640).abs())
                .map(|(_, url)| url)
        })
        .unwrap_or(None);
    Some((
        uri,
        CanvasEntry {
            source: CanvasSource::Spotify,
            name: Some(title.to_owned()),
            artist: Some(credits.join(", ")),
            album_id: None,
            album_name: album
                .and_then(|value| value.get("name"))
                .and_then(|value| value.as_str())
                .map(str::to_owned),
            static_url: image,
            animated: None,
            animated_vertical: None,
            video_url: None,
            video_url_vertical: None,
        },
    ))
}

fn search_body(song: &str, artist: &str, hash: &str) -> serde_json::Value {
    serde_json::json!({
        "variables": {
            "searchTerm": format!("{song} {artist}"),
            "offset": 0,
            "limit": 10,
            "numberOfTopResults": 5,
            "includeAudiobooks": false,
            "includeArtistHasConcertsField": false,
            "includePreReleases": false,
            "includeLocalConcertsField": false,
            "includeAuthors": false,
        },
        "operationName": SEARCH_OPERATION,
        "extensions": {"persistedQuery": {"version": 1, "sha256Hash": hash}},
    })
}

fn search_candidates(
    body: &serde_json::Value,
    song: &str,
    artist: &str,
) -> Result<Vec<(String, CanvasEntry)>, Error> {
    if let Some(errors) = body.get("errors").and_then(|value| value.as_array()) {
        if !errors.is_empty() {
            return Err(Error::Protocol("spotify search returned an error".to_owned()));
        }
    }
    let items = body
        .get("data")
        .and_then(|value| value.get("searchV2"))
        .and_then(|value| value.get("tracksV2"))
        .and_then(|value| value.get("items"))
        .and_then(|value| value.as_array())
        .ok_or_else(|| Error::Protocol("spotify search response is invalid".to_owned()))?;
    let mut out = Vec::new();
    for item in items {
        let wrapper = match item.get("item") {
            Some(wrapper) => wrapper,
            None => continue,
        };
        let uri_override = wrapper
            .get("_uri")
            .and_then(|value| value.as_str())
            .or_else(|| wrapper.get("uri").and_then(|value| value.as_str()))
            .map(str::to_owned);
        if let Some(track) = wrapper.get("data") {
            if let Some((uri, entry)) = track_of(track, uri_override) {
                if entry.matches_identity(song, artist) {
                    out.push((uri, entry));
                }
            }
        }
    }
    Ok(out)
}

pub fn fetch(
    http: &reqwest::blocking::Client,
    bases: &Bases,
    creds: &Credentials,
    track: &Track,
) -> Result<Option<CanvasEntry>, Error> {
    if creds.access_token.trim().is_empty() || creds.client_id.trim().is_empty() {
        return Err(Error::Unsupported(
            "spotify canvas needs the spotifycore session (pending F10-4)".to_owned(),
        ));
    }
    let mut hashes = creds.search_hashes.clone();
    hashes.sort();
    hashes.dedup();
    let mut candidates = Vec::new();
    for hash in &hashes {
        match search_once(http, bases, creds, track, hash)? {
            Some(found) => {
                candidates = found;
                break;
            }
            None => continue,
        }
    }
    if candidates.is_empty() {
        return Ok(None);
    }
    let mut uris: Vec<String> = Vec::new();
    for (uri, _) in &candidates {
        if !uris.contains(uri) {
            uris.push(uri.clone());
        }
        if uris.len() == 10 {
            break;
        }
    }
    let urls = canvases(http, bases, creds, &uris)?;
    for (uri, entry) in candidates {
        if let Some(url) = urls.get(&uri) {
            let mut resolved = entry;
            resolved.video_url = Some(url.clone());
            resolved.video_url_vertical = Some(url.clone());
            return Ok(Some(resolved));
        }
    }
    Ok(None)
}

fn search_once(
    http: &reqwest::blocking::Client,
    bases: &Bases,
    creds: &Credentials,
    track: &Track,
    hash: &str,
) -> Result<Option<Vec<(String, CanvasEntry)>>, Error> {
    let response = http
        .post(&bases.search_url)
        .header("Authorization", format!("Bearer {}", creds.access_token))
        .header("App-Platform", "WebPlayer")
        .header(reqwest::header::USER_AGENT, WEB_USER_AGENT)
        .header("Origin", "https://open.spotify.com")
        .header("Referer", "https://open.spotify.com/")
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .body(search_body(&track.title, &track.artist_text(), hash).to_string())
        .send()
        .map_err(|err| Error::Transport(err.to_string()))?;
    let status = response.status().as_u16();
    let text = response
        .text()
        .map_err(|err| Error::Protocol(err.to_string()))?;
    if status == 401 || status == 429 {
        return Err(Error::Status(status));
    }
    if status == 412 || text.contains("PersistedQueryNotFound") {
        return Ok(None);
    }
    if status != 200 {
        return Err(Error::Status(status));
    }
    let body: serde_json::Value =
        serde_json::from_str(&text).map_err(|err| Error::Protocol(err.to_string()))?;
    Ok(Some(search_candidates(&body, &track.title, &track.artist_text())?))
}

fn canvases(
    http: &reqwest::blocking::Client,
    bases: &Bases,
    creds: &Credentials,
    uris: &[String],
) -> Result<std::collections::HashMap<String, String>, Error> {
    let token = client_token(http, bases, &creds.client_id)?;
    let payload = encode_request(uris)?;
    let response = http
        .post(&bases.canvas_url)
        .header("Authorization", format!("Bearer {}", creds.access_token))
        .header("Client-Token", token)
        .header(reqwest::header::USER_AGENT, APP_USER_AGENT)
        .header("Accept", "application/protobuf")
        .header("Content-Type", "application/protobuf")
        .header("Accept-Language", "en")
        .body(payload)
        .send()
        .map_err(|err| Error::Transport(err.to_string()))?;
    if !response.status().is_success() {
        return Err(Error::Status(response.status().as_u16()));
    }
    let bytes = response
        .bytes()
        .map_err(|err| Error::Protocol(err.to_string()))?;
    let mut map = std::collections::HashMap::new();
    for (uri, url) in decode_response(&bytes)? {
        if uris.contains(&uri) {
            map.insert(uri, url);
        }
    }
    Ok(map)
}

fn client_token(http: &reqwest::blocking::Client, bases: &Bases, client_id: &str) -> Result<String, Error> {
    if client_id.trim().is_empty() {
        return Err(Error::Protocol("spotify client id is missing".to_owned()));
    }
    let page = http
        .get(&bases.page_url)
        .header(reqwest::header::USER_AGENT, WEB_USER_AGENT)
        .send()
        .map_err(|err| Error::Transport(err.to_string()))?;
    if !page.status().is_success() {
        return Err(Error::Status(page.status().as_u16()));
    }
    let headers: Vec<String> = page
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok().map(str::to_owned))
        .collect();
    let body = page
        .text()
        .map_err(|err| Error::Protocol(err.to_string()))?;
    let version = page_config_version(&body)?;
    let device_id = headers
        .iter()
        .filter_map(|cookie| {
            cookie
                .split(';')
                .next()
                .filter(|part| part.trim_start().starts_with("sp_t="))
                .map(|part| part.trim_start()["sp_t=".len()..].to_owned())
        })
        .next()
        .unwrap_or_else(|| format!("{:x}", crate::cache::now_epoch()));
    let payload = serde_json::json!({
        "client_data": {
            "client_version": version,
            "client_id": client_id,
            "js_sdk_data": {
                "device_brand": "unknown",
                "device_model": "unknown",
                "os": "android",
                "os_version": "14",
                "device_id": device_id,
                "device_type": "smartphone",
            },
        },
    });
    let response = http
        .post(&bases.client_token_url)
        .header(reqwest::header::USER_AGENT, WEB_USER_AGENT)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .body(payload.to_string())
        .send()
        .map_err(|err| Error::Transport(err.to_string()))?;
    if !response.status().is_success() {
        return Err(Error::Status(response.status().as_u16()));
    }
    let root: serde_json::Value = response
        .json()
        .map_err(|err| Error::Protocol(err.to_string()))?;
    root.get("granted_token")
        .and_then(|value| value.get("token"))
        .and_then(|value| value.as_str())
        .filter(|token| !token.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Error::Protocol("spotify client token was not granted".to_owned()))
}

fn page_config_version(page: &str) -> Result<String, Error> {
    let marker = "<script";
    let mut rest = page;
    while let Some(start) = rest.find(marker) {
        let tag = &rest[start..];
        rest = &tag[marker.len()..];
        if !tag_contains_config(tag) {
            continue;
        }
        let body_start = match tag.find('>') {
            Some(end) => end + 1,
            None => continue,
        };
        let body = &tag[body_start..];
        let encoded = match body.split('<').next() {
            Some(text) => text.trim(),
            None => continue,
        };
        if encoded.is_empty() {
            continue;
        }
        let decoded = base64_decode(encoded)
            .ok_or_else(|| Error::Protocol("spotify client configuration is invalid".to_owned()))?;
        let config: serde_json::Value = serde_json::from_slice(&decoded)
            .map_err(|_| Error::Protocol("spotify client configuration is invalid".to_owned()))?;
        if let Some(version) = config
            .get("clientVersion")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
        {
            return Ok(version.to_owned());
        }
    }
    Err(Error::Protocol("spotify client configuration is missing".to_owned()))
}

fn tag_contains_config(tag: &str) -> bool {
    tag.split('>')
        .next()
        .is_some_and(|header| header.contains("appServerConfig"))
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u8;
    for c in input.chars().filter(|c| !c.is_whitespace()) {
        if c == '=' {
            break;
        }
        let value = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' | '-' => 62,
            '/' | '_' => 63,
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
