use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:4416";
const PATH_TEMPLATE: &str = "/po_token";

#[derive(Debug, Clone)]
pub struct PoConfig {
    pub base_url: String,
    pub enabled: bool,
    pub timeout_ms: u64,
}

impl Default for PoConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            enabled: true,
            timeout_ms: 2000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoToken {
    pub po_token: String,
    pub visitor_data: String,
}

fn split_base_url(base_url: &str) -> Option<(String, u16)> {
    let rest = base_url
        .strip_prefix("http://")
        .or_else(|| base_url.strip_prefix("https://"))?;
    let rest = rest.trim_end_matches('/');
    if rest.contains('/') {
        return None;
    }
    let mut parts = rest.splitn(2, ':');
    let host = parts.next()?.to_string();
    let port: u16 = parts.next()?.parse().ok()?;
    if host.is_empty() {
        return None;
    }
    Some((host, port))
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn read_body(mut stream: TcpStream) -> Option<String> {
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).ok()?;
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (_, body) = text.split_once("\r\n\r\n")?;
    if !text.starts_with("HTTP/1.") || !text.contains(" 200 ") {
        return None;
    }
    Some(body.to_string())
}

fn parse_token(body: &str) -> Option<PoToken> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let po_token = value
        .get("poToken")
        .or_else(|| value.get("po_token"))?
        .as_str()?;
    let visitor_data = value
        .get("visitorData")
        .or_else(|| value.get("visitor_data"))?
        .as_str()?;
    if po_token.is_empty() || visitor_data.is_empty() {
        return None;
    }
    Some(PoToken {
        po_token: po_token.to_string(),
        visitor_data: visitor_data.to_string(),
    })
}

pub fn fetch(config: &PoConfig, video_id: &str, client: Option<&str>) -> Option<PoToken> {
    if !config.enabled {
        return None;
    }
    let (host, port) = split_base_url(&config.base_url)?;
    let timeout = Duration::from_millis(config.timeout_ms.max(1));
    let addrs: Vec<SocketAddr> = (host.as_str(), port).to_socket_addrs().ok()?.collect();
    let addr = addrs.into_iter().next()?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    let target = format!(
        "{PATH_TEMPLATE}?video_id={}&client={}",
        percent_encode(video_id),
        percent_encode(client.unwrap_or("default")),
    );
    let request = format!("GET {target} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).ok()?;
    parse_token(&read_body(stream)?)
}

pub fn extractor_args(base_url: &str, token: &PoToken) -> Vec<String> {
    vec![
        "--extractor-args".to_string(),
        format!(
            "youtubepot:bgutilhttp:base_url={base_url};po_token={};visitor_data={}",
            token.po_token, token.visitor_data
        ),
    ]
}
