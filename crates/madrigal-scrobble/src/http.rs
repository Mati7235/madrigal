use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::error::{Error, Result};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Response {
    pub status: u16,
    pub body: String,
}

pub fn form_encode(value: &str) -> String {
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

pub fn encode_form(params: &[(&str, &str)]) -> String {
    params
        .iter()
        .map(|(key, value)| format!("{}={}", form_encode(key), form_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn split_url(url: &str) -> Result<(String, u16, String)> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| Error::BadUrl(format!("missing scheme in {url:?}")))?;
    if scheme == "https" {
        return Err(Error::HttpsUnsupported(url.to_owned()));
    }
    if scheme != "http" {
        return Err(Error::BadUrl(format!("unsupported scheme in {url:?}")));
    }
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], rest[index..].to_owned()),
        None => (rest, "/".to_owned()),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, raw)) => (
            host.to_owned(),
            raw.parse::<u16>()
                .map_err(|_| Error::BadUrl(format!("invalid port in {url:?}")))?,
        ),
        None => (authority.to_owned(), 80),
    };
    if host.is_empty() {
        return Err(Error::BadUrl(format!("empty host in {url:?}")));
    }
    Ok((host, port, path))
}

fn parse_response(raw: &[u8]) -> Result<Response> {
    let text = String::from_utf8_lossy(raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| Error::BadResponse("missing header/body separator".to_owned()))?;
    let status_line = head
        .lines()
        .next()
        .ok_or_else(|| Error::BadResponse("empty response".to_owned()))?;
    if !status_line.starts_with("HTTP/1.") {
        return Err(Error::BadResponse(format!(
            "unsupported status line {status_line:?}"
        )));
    }
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| Error::BadResponse(format!("unparseable status line {status_line:?}")))?;
    Ok(Response {
        status,
        body: body.to_owned(),
    })
}

pub fn post(
    url: &str,
    content_type: &str,
    headers: &[(&str, &str)],
    body: &[u8],
    timeout: Duration,
) -> Result<Response> {
    let (host, port, path) = split_url(url)?;
    let addrs: Vec<SocketAddr> = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(Error::Transport)?
        .collect();
    let addr = addrs
        .into_iter()
        .next()
        .ok_or_else(|| Error::BadUrl(format!("unresolvable host in {url:?}")))?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let mut request = format!(
        "POST {path} HTTP/1.0\r\nHost: {host}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("Connection: close\r\n\r\n");
    stream.write_all(request.as_bytes())?;
    stream.write_all(body)?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    parse_response(&raw)
}
