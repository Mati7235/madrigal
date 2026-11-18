use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use madrigal_lyrics::providers::{
    Provider, Query, betterlyrics::BetterLyricsProvider, kugou::KugouProvider,
    lrclib::LrclibProvider, megalobiz::MegalobizProvider, paxsenix::PaxsenixProvider,
    simpmusic::SimpMusicProvider, tidal::TidalProvider, unison::UnisonProvider,
    youlyplus::YouLyPlusProvider, youtube::YoutubeProvider,
};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("providers")
            .join(name),
    )
    .expect("read fixture")
}

type Handler = Arc<dyn Fn(&str, &str) -> (u16, &'static str, Vec<u8>) + Send + Sync>;

fn mock(handler: Handler) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let base = format!("http://127.0.0.1:{}/", listener.local_addr().expect("addr").port());
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().take(64).flatten() {
            counter.fetch_add(1, Ordering::SeqCst);
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            let mut header_end = None;
            loop {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        raw.extend_from_slice(&buf[..n]);
                        if let Some(pos) = find_headers_end(&raw) {
                            header_end = Some(pos);
                            break;
                        }
                        if raw.len() > 65536 {
                            break;
                        }
                    }
                }
            }
            let header_end = match header_end {
                Some(pos) => pos,
                None => continue,
            };
            let content_length = std::str::from_utf8(&raw[..header_end])
                .unwrap_or_default()
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length:"))
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let mut body = raw[header_end..].to_vec();
            while body.len() < content_length {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => body.extend_from_slice(&buf[..n]),
                }
            }
            let text = String::from_utf8_lossy(&raw[..header_end]).into_owned();
            let target = text.lines().next().unwrap_or_default().to_owned();
            let body_text = String::from_utf8_lossy(&body).into_owned();
            let (status, kind, payload) = handler(&target, &body_text);
            let reason = if status == 200 { "OK" } else { "Error" };
            let head = format!(
                "HTTP/1.0 {status} {reason}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                payload.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&payload);
        }
    });
    (base, hits)
}

fn find_headers_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4).position(|w| w == b"\r\n\r\n").map(|pos| pos + 4)
}

fn json_routes(body: Vec<u8>) -> Handler {
    Arc::new(move |_, _| (200, "application/json", body.clone()))
}

fn query() -> Query {
    Query {
        title: "Yesterday".to_owned(),
        artists: vec!["The Beatles".to_owned()],
        album: None,
        duration_secs: Some(125),
        video_id: Some("NrgmdOz227I".to_owned()),
    }
}

fn client() -> reqwest::Client {
    madrigal_lyrics::providers::http_client()
}

#[tokio::test]
async fn lrclib_selects_synced_match() {
    let (base, _) = mock(json_routes(fixture("lrclib_search.json")));
    let provider = LrclibProvider::with_base_url(base);
    let lyrics = provider
        .fetch(&client(), &query())
        .await
        .expect("fetch")
        .expect("lyrics");
    assert!(lyrics.synced);
    assert_eq!(lyrics.lines[0].start_ms, 4950);
    assert_eq!(lyrics.lines[0].text, "Yesterday");
    assert!(lyrics.lines.len() > 10);
}

#[tokio::test]
async fn lrclib_rejects_mismatched_query() {
    let (base, _) = mock(json_routes(fixture("lrclib_search.json")));
    let provider = LrclibProvider::with_base_url(base);
    let mut mismatch = query();
    mismatch.title = "Xyzq".to_owned();
    mismatch.artists = vec!["Qqqwx".to_owned()];
    assert!(provider.fetch(&client(), &mismatch).await.expect("fetch").is_none());
    let mut far = query();
    far.duration_secs = Some(300);
    assert!(provider.fetch(&client(), &far).await.expect("fetch").is_none());
}

#[tokio::test]
async fn kugou_fetches_and_cleans_headers() {
    let songs = fixture("kugou_songs.json");
    let candidates = fixture("kugou_candidates.json");
    let download = fixture("kugou_download.json");
    let (base, _) = mock(Arc::new(move |target: &str, _| {
        if target.contains("/api/v3/search/song") {
            (200, "application/json", songs.clone())
        } else if target.contains("/download") {
            (200, "application/json", download.clone())
        } else if target.contains("/search") {
            (200, "application/json", candidates.clone())
        } else {
            (404, "text/plain", b"no".to_vec())
        }
    }));
    let provider = KugouProvider::with_bases(base.clone(), base);
    let lyrics = provider.fetch(&client(), &query()).await.expect("fetch").expect("lyrics");
    assert!(lyrics.synced);
    assert!(lyrics.lines.iter().all(|line| !line.text.contains("Written by")));
    let first = lyrics.lines.iter().find(|line| line.text.starts_with("Yesterday")).expect("line");
    assert_eq!(first.start_ms, 5710);
}

#[tokio::test]
async fn megalobiz_compares_title_and_artist() {
    let search = fixture("megalobiz_search.html");
    let detail = fixture("megalobiz_detail.html");
    let (base, _) = mock(Arc::new(move |target: &str, _| {
        if target.contains("/lrc/maker/download/AAA111") {
            (200, "text/html", detail.clone())
        } else if target.contains("/lrc/maker/download/") {
            (404, "text/plain", b"no".to_vec())
        } else if target.contains("qry=") {
            (200, "text/html", search.clone())
        } else {
            (404, "text/plain", b"no".to_vec())
        }
    }));
    let provider = MegalobizProvider::with_base_url(base.trim_end_matches('/'));
    let lyrics = provider.fetch(&client(), &query()).await.expect("fetch").expect("lyrics");
    assert!(lyrics.synced);
    assert!(lyrics.lines.iter().any(|line| line.text == "Yesterday"));
    let mut decoy = query();
    decoy.title = "Nobody Knows This Song".to_owned();
    decoy.artists = vec!["Nobody".to_owned()];
    assert!(provider.fetch(&client(), &decoy).await.expect("fetch").is_none());
}

#[tokio::test]
async fn simpmusic_picks_closest_duration() {
    let (base, _) = mock(json_routes(fixture("simpmusic.json")));
    let provider = SimpMusicProvider::with_base_url(base);
    let lyrics = provider.fetch(&client(), &query()).await.expect("fetch").expect("lyrics");
    assert!(lyrics.synced);
    assert_eq!(lyrics.lines[1].text, "Yesterday");
    let mut no_video = query();
    no_video.video_id = None;
    assert!(provider.fetch(&client(), &no_video).await.expect("fetch").is_none());
}

#[tokio::test]
async fn unison_falls_back_to_video_entry_with_words() {
    let video = fixture("unison_video.json");
    let (base, _) = mock(Arc::new(move |target: &str, _| {
        if target.contains("/lyrics/search") {
            (200, "application/json", b"{\"success\":true,\"data\":[]}".to_vec())
        } else {
            (200, "application/json", video.clone())
        }
    }));
    let provider = UnisonProvider::with_base_url(base);
    let lyrics = provider.fetch(&client(), &query()).await.expect("fetch").expect("lyrics");
    assert!(lyrics.synced);
    assert!(lyrics.has_words());
}

#[tokio::test]
async fn youlyplus_ttml_then_lrc_fallback() {
    let ttml = fixture("youlyplus_ttml.json");
    let lrc = fixture("youlyplus_lrc.json");
    let (base, hits) = mock(Arc::new(move |target: &str, _| {
        if target.contains("/v1/ttml/get") {
            (200, "application/json", ttml.clone())
        } else {
            (200, "application/json", lrc.clone())
        }
    }));
    let provider = YouLyPlusProvider::with_mirrors(vec![base.clone()]);
    let lyrics = provider.fetch(&client(), &query()).await.expect("fetch").expect("lyrics");
    assert!(lyrics.synced);
    assert!(lyrics.has_words());
    assert!(hits.load(Ordering::SeqCst) >= 1);
}

#[tokio::test]
async fn youlyplus_lrc_path_parses_times() {
    let (base, _) = mock(Arc::new(move |target: &str, _| {
        if target.contains("/v1/ttml/get") {
            (404, "text/plain", b"no".to_vec())
        } else {
            (200, "application/json", fixture("youlyplus_lrc.json"))
        }
    }));
    let provider = YouLyPlusProvider::with_mirrors(vec![base]);
    let lyrics = provider.fetch(&client(), &query()).await.expect("fetch").expect("lyrics");
    assert!(lyrics.synced);
    assert_eq!(lyrics.lines[0].text, "Yesterday");
    assert_eq!(lyrics.lines[0].start_ms, 5070);
}

#[tokio::test]
async fn betterlyrics_raw_and_wrapped_ttml() {
    let raw = fixture("betterlyrics.ttml");
    let (base, _) = mock(Arc::new(move |target: &str, _| {
        if target.contains("kugou/getLyrics") {
            (404, "text/plain", b"no".to_vec())
        } else {
            (200, "application/xml", raw.clone())
        }
    }));
    let provider = BetterLyricsProvider::with_base_url(base, false);
    let lyrics = provider.fetch(&client(), &query()).await.expect("fetch").expect("lyrics");
    assert!(lyrics.has_words());
    let wrapped = fixture("betterlyrics_wrapped.json");
    let (base2, _) = mock(json_routes(wrapped));
    let provider2 = BetterLyricsProvider::with_base_url(base2, true);
    let lyrics2 = provider2.fetch(&client(), &query()).await.expect("fetch").expect("lyrics");
    assert_eq!(lyrics.plain_text(), lyrics2.plain_text());
}

#[tokio::test]
async fn paxsenix_scores_search_and_extracts() {
    let search = fixture("paxsenix_search.json");
    let apple = fixture("paxsenix_apple.json");
    let (base, hits) = mock(Arc::new(move |target: &str, _| {
        if target.contains("apple-music/search") {
            (200, "application/json", search.clone())
        } else if target.contains("id=am-4242") {
            (200, "application/json", apple.clone())
        } else {
            (404, "text/plain", b"no".to_vec())
        }
    }));
    let provider = PaxsenixProvider::apple_music("test-key").with_base_url(base);
    let lyrics = provider.fetch(&client(), &query()).await.expect("fetch").expect("lyrics");
    assert!(lyrics.synced);
    assert!(lyrics.lines.iter().any(|line| line.text == "Yesterday"));
    assert!(hits.load(Ordering::SeqCst) >= 2);
    let keyless = PaxsenixProvider::spotify("");
    assert!(keyless.fetch(&client(), &query()).await.expect("fetch").is_none());
}

#[tokio::test]
async fn paxsenix_spotify_extracts_plain_field() {
    let body = fixture("paxsenix_spotify.json");
    let (base, _) = mock(Arc::new(move |target: &str, _| {
        if target.contains("spotify/search") {
            (200, "application/json", fixture("paxsenix_search.json"))
        } else {
            (200, "application/json", body.clone())
        }
    }));
    let provider = PaxsenixProvider::spotify("test-key").with_base_url(base);
    let lyrics = provider.fetch(&client(), &query()).await.expect("fetch").expect("lyrics");
    assert!(lyrics.synced);
    assert_eq!(lyrics.lines[0].text, "Yesterday");
}

#[tokio::test]
async fn youtube_timed_then_plain() {
    let watch = fixture("yt_watch.json");
    let timed = fixture("yt_timed.json");
    let plain = fixture("yt_browse_plain.json");
    let (base, _) = mock(Arc::new(move |target: &str, body: &str| {
        if target.contains("/next") {
            (200, "application/json", watch.clone())
        } else if body.contains("ANDROID_MUSIC") {
            (200, "application/json", timed.clone())
        } else {
            (200, "application/json", plain.clone())
        }
    }));
    let provider = YoutubeProvider::with_base_url(base);
    let lyrics = provider.fetch(&client(), &query()).await.expect("fetch").expect("lyrics");
    assert!(lyrics.synced);
    assert_eq!(lyrics.lines[1].text, "Yesterday");
    assert_eq!(lyrics.lines[1].start_ms, 5070);
    let mut no_video = query();
    no_video.video_id = None;
    assert!(provider.fetch(&client(), &no_video).await.expect("fetch").is_none());
}

#[tokio::test]
async fn tidal_returns_unsupported() {
    let provider = TidalProvider;
    let err = provider.fetch(&client(), &query()).await.expect_err("unsupported");
    assert!(err.to_string().contains("tidal"));
}
