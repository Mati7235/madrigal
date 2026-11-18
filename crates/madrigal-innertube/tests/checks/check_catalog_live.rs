use std::sync::OnceLock;
use std::time::Duration;

use madrigal_innertube::CatalogClient;
use tokio::sync::Mutex;

fn gate() -> &'static Mutex<()> {
    static GATE: OnceLock<Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(()))
}

async fn live_client() -> (CatalogClient, tokio::sync::MutexGuard<'static, ()>) {
    let guard = gate().lock().await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let client = CatalogClient::new().await.expect("live catalog client");
    (client, guard)
}

#[tokio::test]
async fn search_returns_song_tracks() {
    let (client, _guard) = live_client().await;
    let tracks = client
        .search_songs("Yesterday Beatles")
        .await
        .expect("live search");
    assert!(
        tracks.len() >= 10,
        "expected >= 10 tracks, got {}",
        tracks.len()
    );
    for track in &tracks {
        assert!(!track.title.is_empty(), "track title is empty");
        assert!(!track.artists.is_empty(), "track has no artists");
        assert!(
            track.artists.iter().all(|a| !a.is_empty()),
            "track has empty artist"
        );
    }
}

#[tokio::test]
async fn suggestions_return_entries() {
    let (client, _guard) = live_client().await;
    let suggestions = client.suggestions("beatl").await.expect("live suggestions");
    assert!(!suggestions.is_empty(), "expected suggestions");
    assert!(
        suggestions.iter().all(|s| !s.is_empty()),
        "suggestion entry is empty"
    );
}

#[tokio::test]
async fn home_returns_sections_with_items() {
    let (client, _guard) = live_client().await;
    let home = client.get_home().await.expect("live home");
    assert!(!home.sections.is_empty(), "expected home sections");
    assert!(
        home.sections.iter().any(|s| !s.items.is_empty()),
        "expected a section with items"
    );
}

#[tokio::test]
async fn charts_parse_videos() {
    let (client, _guard) = live_client().await;
    let page = client.charts("ZZ").await.expect("live charts");
    assert!(!page.videos.is_empty(), "expected chart videos");
    assert!(
        page.videos.iter().all(|v| !v.title.is_empty()),
        "chart video without title"
    );
}

#[tokio::test]
async fn moods_list_categories() {
    let (client, _guard) = live_client().await;
    let listing = client.mood_listing().await.expect("live moods");
    assert!(
        listing.sections.len() >= 2,
        "expected >= 2 mood categories, got {}",
        listing.sections.len()
    );
    assert!(
        listing
            .sections
            .iter()
            .all(|s| !s.name.is_empty() && !s.entries.is_empty()),
        "mood section without name or entries"
    );
}
