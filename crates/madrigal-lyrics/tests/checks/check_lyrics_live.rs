use madrigal_core::{Track, TrackIdentity};
use madrigal_lyrics::chain::{self, ChainConfig, EndpointBases};
use madrigal_lyrics::providers::http_client;
use madrigal_lyrics::Lyrics;
use madrigal_settings::{LyricsProvider, DEFAULT_PROVIDER_ORDER};

fn lrclib_only() -> ChainConfig {
    let order: Vec<LyricsProvider> = DEFAULT_PROVIDER_ORDER
        .into_iter()
        .filter(|p| *p == LyricsProvider::Lrclib)
        .collect();
    ChainConfig {
        order,
        disabled: Vec::new(),
        paxsenix_api_key: String::new(),
        youtube_subtitle_enabled: false,
        bases: EndpointBases::default(),
    }
}

fn track(identity: &str, title: &str, artist: &str) -> Track {
    Track {
        identity: TrackIdentity::youtube(identity),
        title: title.to_owned(),
        artists: vec![artist.to_owned()],
        album: None,
        duration_secs: Some(125),
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

async fn attempt(track: Track) -> Option<Lyrics> {
    let dir = tempfile::tempdir().expect("tempdir");
    let conn = madrigal_db::open(&dir.path().join("lyrics.db")).expect("open db");
    chain::fetch_track(&http_client(), &conn, &track, &lrclib_only())
        .await
        .expect("fetch")
        .map(|outcome| outcome.lyrics)
}

#[tokio::test]
async fn lrclib_returns_synced_lyrics() {
    let lyrics = match attempt(track("LIVEGATE001", "Yesterday", "The Beatles")).await {
        Some(lyrics) => lyrics,
        None => attempt(track("LIVEGATE002", "Yesterday", "Beatles"))
            .await
            .expect("lrclib retry hit"),
    };
    assert!(
        lyrics.lines.len() >= 10,
        "expected >= 10 lines, got {}",
        lyrics.lines.len()
    );
    assert!(
        lyrics.lines.iter().any(|line| !line.text.trim().is_empty()),
        "expected non-empty lyric text"
    );
}
