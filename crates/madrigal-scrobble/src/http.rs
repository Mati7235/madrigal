use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::{HeaderName, HeaderValue, CONTENT_TYPE};
use reqwest::redirect::Policy;
use reqwest::Url;

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

fn validate_url(url: &str) -> Result<()> {
    let parsed =
        Url::parse(url).map_err(|_| Error::BadUrl(format!("unparseable url in {url:?}")))?;
    if parsed.host_str().is_none() || parsed.host_str() == Some("") {
        return Err(Error::BadUrl(format!("empty host in {url:?}")));
    }
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        scheme => Err(Error::BadUrl(format!(
            "unsupported scheme {scheme:?} in {url:?}"
        ))),
    }
}

pub fn post(
    url: &str,
    content_type: &str,
    headers: &[(&str, &str)],
    body: &[u8],
    timeout: Duration,
) -> Result<Response> {
    validate_url(url)?;
    let client = Client::builder()
        .timeout(timeout)
        .redirect(Policy::none())
        .build()
        .map_err(Error::Transport)?;
    let mut request = client
        .post(url)
        .header(CONTENT_TYPE, content_type)
        .body(body.to_vec());
    for (name, value) in headers {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| Error::BadResponse(format!("invalid header name {name:?}")))?;
        let value = HeaderValue::from_str(value)
            .map_err(|_| Error::BadResponse(format!("invalid header value {value:?}")))?;
        request = request.header(name, value);
    }
    let response = request.send().map_err(Error::Transport)?;
    let status = response.status().as_u16();
    let body = response.text().map_err(Error::Transport)?;
    Ok(Response { status, body })
}
