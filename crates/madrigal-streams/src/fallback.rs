pub const CLIENT_LABELS: [&str; 4] = ["default", "web_safari", "tv", "ios"];

pub fn player_clients() -> Vec<Option<String>> {
    vec![
        None,
        Some("web_safari".to_string()),
        Some("tv".to_string()),
        Some("ios".to_string()),
    ]
}

pub fn chain_labels() -> Vec<&'static str> {
    CLIENT_LABELS.to_vec()
}

pub fn client_label(client: Option<&str>) -> &'static str {
    match client {
        None => "default",
        Some("web_safari") => "web_safari",
        Some("tv") => "tv",
        Some("ios") => "ios",
        Some(_) => "default",
    }
}

pub fn is_rate_limited(text: &str) -> bool {
    text.contains("429")
        || text.contains("Too Many Requests")
        || text.to_lowercase().contains("rate-limit")
}

pub fn is_retryable_failure(stderr: &str, code: Option<i32>) -> bool {
    if is_rate_limited(stderr) {
        return true;
    }
    if code.is_none() {
        return true;
    }
    let lower = stderr.to_lowercase();
    for marker in [
        "sign in to confirm",
        "not a bot",
        "player response",
        "requested format is not available",
        "unable to extract",
        "sin campo url",
        "no url field",
    ] {
        if lower.contains(marker) {
            return true;
        }
    }
    code != Some(0)
}
