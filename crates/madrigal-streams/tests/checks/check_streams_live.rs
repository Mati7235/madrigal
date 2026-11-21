use std::path::Path;
use std::time::Duration;

use madrigal_core::TrackIdentity;
use madrigal_streams::{Orchestrator, PoConfig, StreamConfig, INNERTUBE_LABEL};
use reqwest::header::{CONTENT_RANGE, RANGE, USER_AGENT};

const CHUNK: u64 = 1024 * 1024;

fn fixture_ids(limit: usize) -> Vec<String> {
    let raw = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../madrigal-innertube/tests/fixtures/search_songs_beatles.json"),
    )
    .expect("read innertube fixture");
    let mut ids: Vec<String> = Vec::new();
    for part in raw.split("\"videoId\":\"").skip(1) {
        let Some((id, _)) = part.split_once('"') else {
            continue;
        };
        if !ids.iter().any(|known| known == id) {
            ids.push(id.to_string());
        }
        if ids.len() == limit {
            break;
        }
    }
    ids
}

fn content_range_total(value: Option<&str>) -> Option<u64> {
    let (_, total) = value?.rsplit_once('/')?;
    total.parse::<u64>().ok()
}

fn download(url: &str, user_agent: Option<&str>) -> u64 {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .expect("http client");
    let mut downloaded: u64 = 0;
    let mut total: Option<u64> = None;
    loop {
        let end = downloaded + CHUNK - 1;
        let mut request = client
            .get(url)
            .header(RANGE, format!("bytes={downloaded}-{end}"));
        if let Some(agent) = user_agent {
            request = request.header(USER_AGENT, agent);
        }
        let response = request.send().expect("range request");
        if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            break;
        }
        assert!(
            response.status().is_success(),
            "range status {}",
            response.status()
        );
        if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            total = total.or_else(|| {
                content_range_total(
                    response
                        .headers()
                        .get(CONTENT_RANGE)
                        .and_then(|v| v.to_str().ok()),
                )
            });
        } else if total.is_none() {
            total = response.content_length();
        }
        let bytes = response.bytes().expect("range body");
        let read = bytes.len() as u64;
        downloaded += read;
        if read == 0 || read < CHUNK {
            break;
        }
        if total.is_some_and(|total| downloaded >= total) {
            break;
        }
    }
    total.map_or(downloaded, |total| {
        assert_eq!(downloaded, total, "incomplete download");
        downloaded
    })
}

#[test]
fn streams_live_resolves_and_downloads_audio() {
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, root.path());
    let ids = fixture_ids(2);
    assert_eq!(ids.len(), 2, "fixture ids");
    let config = StreamConfig {
        timeout_secs: 20,
        po: PoConfig {
            enabled: false,
            ..PoConfig::default()
        },
        ..StreamConfig::default()
    };
    let streams = Orchestrator::new(config).expect("orchestrator");
    for id in ids {
        let (stream, label) = streams
            .resolve(&TrackIdentity::youtube(id.as_str()), None)
            .expect("resolve live stream");
        assert_eq!(label, INNERTUBE_LABEL, "{id}: {label}");
        assert_eq!(stream.resolution_path, INNERTUBE_LABEL);
        assert!(stream.url.starts_with("https://"), "{id}: {}", stream.url);
        let bytes = download(&stream.url, stream.user_agent.as_deref());
        assert!(bytes > 256 * 1024, "{id}: downloaded only {bytes} bytes");
    }
}
