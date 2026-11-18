use std::fmt;

pub const LOGIN_INFO_COOKIE: &str = "LOGIN_INFO";
pub const PRIMARY_SAPISID_COOKIE: &str = "__Secure-3PAPISID";
pub const SAPISID_COOKIES: [&str; 3] =
    ["SAPISID", "__Secure-3PAPISID", "__Secure-1PAPISID"];
pub const YOUTUBE_MUSIC_ORIGIN: &str = "https://music.youtube.com";
pub const YOUTUBE_COOKIE_DOMAINS: [&str; 3] =
    ["music.youtube.com", "www.youtube.com", "youtube.com"];
pub const NETSCAPE_HEADER: &str = "# Netscape HTTP Cookie File";
pub const HTTPONLY_PREFIX: &str = "#HttpOnly_";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieRecord {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub expires: i64,
}

impl CookieRecord {
    pub fn new(name: &str, value: &str, domain: &str) -> Self {
        Self {
            name: name.to_owned(),
            value: value.to_owned(),
            domain: domain.to_owned(),
            path: "/".to_owned(),
            secure: true,
            http_only: true,
            expires: 0,
        }
    }

    pub fn is_expired(&self, now: i64) -> bool {
        self.expires != 0 && self.expires <= now
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookieError {
    ControlChar,
    BadFieldCount(usize),
}

impl fmt::Display for CookieError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ControlChar => write!(f, "cookie forbids CR, LF and TAB"),
            Self::BadFieldCount(n) => write!(f, "netscape line needs 7 fields, got {n}"),
        }
    }
}

impl std::error::Error for CookieError {}

fn has_control(value: &str) -> bool {
    value.bytes().any(|b| b == b'\r' || b == b'\n' || b == b'\t')
}

pub fn parse_cookie_header(value: &str) -> Result<Vec<(String, String)>, CookieError> {
    if has_control(value) {
        return Err(CookieError::ControlChar);
    }
    let mut pairs = Vec::new();
    for chunk in value.split(';') {
        let (name, content) = match chunk.split_once('=') {
            Some((name, content)) => (name.trim(), content.trim()),
            None => (chunk.trim(), ""),
        };
        if name.is_empty() || pairs.iter().any(|(key, _)| key == name) {
            continue;
        }
        pairs.push((name.to_owned(), content.to_owned()));
    }
    Ok(pairs)
}

pub fn cookie_header(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

fn lookup<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

pub fn login_cookie_value(pairs: &[(String, String)]) -> Option<&str> {
    SAPISID_COOKIES
        .iter()
        .filter_map(|name| lookup(pairs, name))
        .find(|value| !value.is_empty())
}

pub fn has_login_cookie(pairs: &[(String, String)]) -> bool {
    match lookup(pairs, LOGIN_INFO_COOKIE) {
        Some(value) if !value.is_empty() => login_cookie_value(pairs).is_some(),
        _ => false,
    }
}

pub fn has_primary_sapisid(pairs: &[(String, String)]) -> bool {
    match lookup(pairs, PRIMARY_SAPISID_COOKIE) {
        Some(value) => !value.is_empty(),
        None => false,
    }
}

pub fn sapisidhash(cookie: &str, origin: &str, timestamp: i64) -> Option<String> {
    let pairs = parse_cookie_header(cookie).ok()?;
    let sapisid = login_cookie_value(&pairs)?;
    let hex = sha1_hex(format!("{timestamp} {sapisid} {origin}").as_bytes());
    Some(format!("SAPISIDHASH {timestamp}_{hex}"))
}

fn sha1_hex(data: &[u8]) -> String {
    let mut state: [u32; 5] = [
        0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0,
    ];
    let mut padded = data.to_vec();
    let bits = (padded.len() as u64).wrapping_mul(8);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bits.to_be_bytes());
    for block in padded.as_chunks::<64>().0 {
        let mut schedule = [0u32; 80];
        for (i, word) in schedule.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes([
                block[4 * i],
                block[4 * i + 1],
                block[4 * i + 2],
                block[4 * i + 3],
            ]);
        }
        for i in 16..80 {
            schedule[i] = (schedule[i - 3] ^ schedule[i - 8] ^ schedule[i - 14] ^ schedule[i - 16])
                .rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) =
            (state[0], state[1], state[2], state[3], state[4]);
        for (i, word) in schedule.iter().enumerate() {
            let (f, k) = match i {
                0..20 => ((b & c) | (!b & d), 0x5A827999),
                20..40 => (b ^ c ^ d, 0x6ED9EBA1),
                40..60 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
    state.iter().map(|word| format!("{word:08x}")).collect()
}

pub fn is_youtube_cookie(domain: &str) -> bool {
    let normalized = domain.strip_prefix('.').unwrap_or(domain);
    YOUTUBE_COOKIE_DOMAINS.iter().any(|known| {
        normalized == *known || normalized.ends_with(&format!(".{known}"))
    })
}

fn domain_rank(domain: &str) -> usize {
    let normalized = domain.strip_prefix('.').unwrap_or(domain);
    YOUTUBE_COOKIE_DOMAINS
        .iter()
        .position(|known| normalized == *known || normalized.ends_with(&format!(".{known}")))
        .unwrap_or(YOUTUBE_COOKIE_DOMAINS.len())
}

pub fn merge_login_cookies(records: &[CookieRecord]) -> String {
    let mut ranked: Vec<&CookieRecord> = records.iter().collect();
    ranked.sort_by_key(|record| domain_rank(&record.domain));
    let mut merged: Vec<(String, String)> = Vec::new();
    for record in ranked {
        if !merged.iter().any(|(name, _)| name == &record.name) {
            merged.push((record.name.clone(), record.value.clone()));
        }
    }
    cookie_header(&merged)
}

pub fn youtube_records(records: &[CookieRecord]) -> Vec<CookieRecord> {
    records
        .iter()
        .filter(|record| is_youtube_cookie(&record.domain))
        .cloned()
        .collect()
}

pub fn to_netscape(records: &[CookieRecord]) -> Result<String, CookieError> {
    for record in records {
        for field in [&record.name, &record.value, &record.domain, &record.path] {
            if has_control(field) {
                return Err(CookieError::ControlChar);
            }
        }
    }
    let mut lines = vec![NETSCAPE_HEADER.to_owned(), String::new()];
    for record in records {
        let domain = if record.http_only {
            format!("{HTTPONLY_PREFIX}{}", record.domain)
        } else {
            record.domain.clone()
        };
        lines.push(
            [
                domain,
                if record.domain.starts_with('.') {
                    "TRUE"
                } else {
                    "FALSE"
                }
                .to_owned(),
                if record.path.is_empty() {
                    "/".to_owned()
                } else {
                    record.path.clone()
                },
                if record.secure { "TRUE" } else { "FALSE" }.to_owned(),
                record.expires.to_string(),
                record.name.clone(),
                record.value.clone(),
            ]
            .join("\t"),
        );
    }
    let mut text = lines.join("\n");
    text.push('\n');
    Ok(text)
}

pub fn from_netscape(text: &str) -> Result<Vec<CookieRecord>, CookieError> {
    let mut records = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix(HTTPONLY_PREFIX) {
            records.push(parse_netscape_line(rest, true)?);
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        records.push(parse_netscape_line(line, false)?);
    }
    Ok(records)
}

fn parse_netscape_line(line: &str, http_only: bool) -> Result<CookieRecord, CookieError> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() != 7 {
        return Err(CookieError::BadFieldCount(fields.len()));
    }
    Ok(CookieRecord {
        name: fields[5].to_owned(),
        value: fields[6].to_owned(),
        domain: fields[0].to_owned(),
        path: fields[2].to_owned(),
        secure: fields[3] == "TRUE",
        http_only,
        expires: fields[4].parse().unwrap_or(0),
    })
}

pub fn sapisid_expired(records: &[CookieRecord], now: i64) -> bool {
    let mut found = false;
    for record in records {
        if SAPISID_COOKIES.contains(&record.name.as_str()) {
            found = true;
            if !record.is_expired(now) {
                return false;
            }
        }
    }
    found
}

pub fn sapisid_ttl(records: &[CookieRecord], now: i64) -> Option<i64> {
    records
        .iter()
        .filter(|record| {
            SAPISID_COOKIES.contains(&record.name.as_str()) && record.expires != 0
        })
        .map(|record| record.expires - now)
        .min()
}
