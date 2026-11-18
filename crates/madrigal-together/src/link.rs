use crate::models::SessionCode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinInfo {
    pub host: String,
    pub port: u16,
    pub session_id: String,
    pub session_key: String,
}

impl JoinInfo {
    pub fn deep_link(&self) -> String {
        format!(
            "madrigal://together?host={}&port={}&sid={}&key={}",
            enc(&self.host),
            self.port,
            enc(&self.session_id),
            enc(&self.session_key),
        )
    }

    pub fn code_link(code: &SessionCode) -> String {
        format!("madrigal://together/join?code={}", enc(code.as_str()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinLink {
    Full(JoinInfo),
    Code(SessionCode),
}

pub fn parse(raw: &str) -> Option<JoinLink> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(link) = parse_deep_link(trimmed) {
        return Some(link);
    }
    if let Some(info) = parse_ws_url(trimmed) {
        return Some(JoinLink::Full(info));
    }
    parse_compact(trimmed)
}

fn scheme_of(raw: &str) -> Option<String> {
    raw.split_once("://")
        .map(|(scheme, _)| scheme.to_lowercase())
}

fn rest_of(raw: &str) -> &str {
    match raw.split_once("://") {
        Some((_, rest)) => rest,
        None => raw,
    }
}

fn parse_deep_link(raw: &str) -> Option<JoinLink> {
    let scheme = scheme_of(raw)?;
    let rest = rest_of(raw);
    if scheme != "madrigal" {
        return None;
    }
    let (authority, query) = match rest.split_once('?') {        Some((a, q)) => (a.to_lowercase(), q),
        None => (rest.to_lowercase(), ""),
    };
    if authority == "together/join" {
        let params = parse_query(query);
        let code = params.get("code")?;
        return SessionCode::normalize(code).map(JoinLink::Code);
    }
    if authority != "together" {
        return None;
    }
    let params = parse_query(query);
    let info = JoinInfo {
        host: params.get("host")?.trim().to_owned(),
        port: params.get("port")?.parse().ok()?,
        session_id: params.get("sid")?.trim().to_owned(),
        session_key: params.get("key")?.trim().to_owned(),
    };
    if info.host.is_empty()
        || info.session_id.is_empty()
        || info.session_key.is_empty()
        || info.port == 0
    {
        return None;
    }
    Some(JoinLink::Full(info))
}

fn parse_ws_url(raw: &str) -> Option<JoinInfo> {
    let scheme = scheme_of(raw)?;
    let rest = rest_of(raw);
    if !matches!(scheme.as_str(), "ws" | "wss" | "http" | "https") {
        return None;
    }
    let (host_port, query) = match rest.split_once('?') {
        Some((h, q)) => (h, q),
        None => (rest, ""),
    };
    let host_port = host_port.trim_end_matches('/');
    let (host, port) = match host_port.split_once(':') {
        Some((h, p)) => (h.trim().to_owned(), p.parse().ok()?),
        None => (
            host_port.to_owned(),
            if scheme == "wss" || scheme == "https" {
                443
            } else {
                80
            },
        ),
    };    let params = parse_query(query);
    let info = JoinInfo {
        host,
        port,
        session_id: params.get("sid")?.trim().to_owned(),
        session_key: params.get("key")?.trim().to_owned(),
    };
    if info.host.is_empty() || info.session_id.is_empty() || info.session_key.is_empty() {
        return None;
    }
    Some(info)
}

fn parse_compact(raw: &str) -> Option<JoinLink> {
    let parts: Vec<&str> = raw.split('|').collect();
    if parts.len() != 4 {
        return None;
    }
    let info = JoinInfo {
        host: parts[0].trim().to_owned(),
        port: parts[1].trim().parse().ok()?,
        session_id: parts[2].trim().to_owned(),
        session_key: parts[3].trim().to_owned(),
    };
    if info.host.is_empty()
        || info.session_id.is_empty()
        || info.session_key.is_empty()
        || info.port == 0
    {
        return None;
    }
    Some(JoinLink::Full(info))
}

fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for pair in query.split('&') {
        let Some(idx) = pair.find('=') else {
            continue;
        };
        out.insert(dec(&pair[..idx]), dec(&pair[idx + 1..]));
    }
    out
}

fn enc(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for b in raw.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn dec(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8(out).unwrap_or_default()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
