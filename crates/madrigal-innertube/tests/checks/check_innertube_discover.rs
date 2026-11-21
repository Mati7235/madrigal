use madrigal_innertube::{
    is_radio_id, BrowseItem, CatalogClient, ChartTrend,
};
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).expect("read fixture")
}

#[test]
fn charts_fixture_maps_exact_entries() {
    let source = fixture("charts_ZZ.json");
    let page = CatalogClient::parse_charts(&source).expect("parse charts");
    assert_eq!(page.videos.len(), 2);
    assert_eq!(page.videos[0].title, "Daily Top Music Videos - Global");
    assert_eq!(
        page.videos[0].id.as_str(),
        "VLPL4fGSI1pDJn6t3TXLGiiJdD-sZbrG3tG0"
    );
    assert_eq!(page.videos[1].title, "Top 100 Music Videos Global");
    assert_eq!(page.artists.len(), 40);
    assert_eq!(page.artists[0].artist.name, "Alka Yagnik");
    assert_eq!(page.artists[0].artist.id.as_str(), "UCptBkLZ6XRxoyn8SkUMc_Iw");
    assert_eq!(page.artists[0].rank.as_deref(), Some("1"));
    assert_eq!(page.artists[0].trend, Some(ChartTrend::Neutral));
    assert_eq!(
        page.artists[0].artist.subscribers.as_deref(),
        Some("2.33M")
    );
    assert_eq!(page.artists[1].artist.name, "Arijit Singh");
    assert_eq!(page.artists[1].rank.as_deref(), Some("2"));
    assert!(page.genres.is_empty());
}

#[test]
fn charts_page_keeps_section_titles() {
    let source = fixture("charts_ZZ.json");
    let page = CatalogClient::parse_charts_page(&source).expect("parse charts page");
    let titles: Vec<&str> = page.sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, vec!["Video charts", "Top artists"]);
    assert_eq!(page.sections[0].items.len(), 2);
    assert!(matches!(page.sections[0].items[0], BrowseItem::Playlist(_)));
    assert_eq!(page.sections[1].items.len(), 40);
    assert!(matches!(page.sections[1].items[0], BrowseItem::Artist(_)));
}

#[test]
fn new_releases_fixture_maps_tabs() {
    let source = fixture("explore.json");
    let releases = CatalogClient::parse_new_releases(&source).expect("parse new releases");
    assert_eq!(releases.albums.len(), 24);
    assert_eq!(releases.albums[0].title, "El Mundo");
    assert_eq!(releases.albums[0].id.as_str(), "MPREb_PBfI2Zo66qz");
    assert_eq!(
        releases.albums[0].audio_playlist_id.as_deref(),
        Some("OLAK5uy_lZYsKrcn5m4OusOx6CQK3NM-WHI0f7QF4")
    );
    assert_eq!(releases.albums[1].title, "Cumbia Del Monte (DamianDiazMix)");
    assert_eq!(releases.videos.len(), 24);
    assert_eq!(releases.videos[0].title, "Piscina");
    assert_eq!(releases.videos[0].identity.id.as_str(), "vK9AG0gVvnQ");
    assert_eq!(
        releases.videos[0].artists,
        vec![
            "Maria Becerra".to_owned(),
            "Chencho Corleone".to_owned(),
            "Ovy On The Drums".to_owned(),
        ]
    );
    assert!(releases.videos.iter().all(|t| t.is_valid()));
}

#[test]
fn browse_fixture_walks_sections() {
    let source = fixture("explore.json");
    let page = CatalogClient::parse_browse(&source).expect("parse browse");
    assert_eq!(page.sections.len(), 3);
    assert_eq!(page.sections[0].title, "New albums & singles");
    assert_eq!(page.sections[1].title, "Trending");
    assert_eq!(page.sections[2].title, "New music videos");
    assert_eq!(page.sections[0].items.len(), 24);
    assert_eq!(page.sections[1].items.len(), 20);
    assert_eq!(page.sections[2].items.len(), 24);
    assert!(matches!(
        page.sections[0].items[0],
        BrowseItem::Album(_)
    ));
    assert!(matches!(
        page.sections[1].items[0],
        BrowseItem::Track(_)
    ));
    let releases = CatalogClient::parse_new_releases(&source).expect("parse new releases");
    assert_eq!(
        page.sections[0].items[0],
        BrowseItem::Album(releases.albums[0].clone())
    );
    assert_eq!(
        page.sections[2].items[0],
        BrowseItem::Track(releases.videos[0].clone())
    );
}

#[test]
fn mood_deep_link_from_listing_params() {
    let listing_source = fixture("moods.json");
    let listing = CatalogClient::parse_mood_listing(&listing_source).expect("parse moods");
    let chill = listing
        .sections
        .iter()
        .flat_map(|s| &s.entries)
        .find(|e| e.title == "Chill")
        .expect("chill entry");
    assert_eq!(chill.params, "ggMPOg1uX1JOQWZFeDByc2Jm");
    let source = fixture("mood_playlists_chill.json");
    let linked = CatalogClient::parse_mood_playlists(&source).expect("parse mood playlists");
    assert_eq!(linked.categories.len(), 15);
    assert_eq!(linked.categories[0].name, "Coffee shop blends");
    assert_eq!(linked.categories[0].playlists.len(), 13);
    assert_eq!(
        linked.categories[0].playlists[0].title,
        "Coffee Shop Blend"
    );
    assert_eq!(
        linked.categories[0].playlists[0].id.as_str(),
        "VLRDCLAK5uy_nBE4bLuBHUXWZrF59ZrkPEToKt8M_I3Vc"
    );
    assert_eq!(
        linked.categories[0].playlists[0].author.as_deref(),
        Some("YouTube Music")
    );
    assert_eq!(linked.categories[1].name, "Unwind + explore");
    assert_eq!(linked.categories[1].playlists[0].title, "Pop Gold");
    assert_eq!(
        linked.categories[1].playlists[1].title,
        "Soft Rock Ballads"
    );
}

#[test]
fn radio_fallback_for_vl_ids() {
    assert!(is_radio_id("RDAMVMJ45RU9Iu8PA"));
    assert!(is_radio_id("VLPLxA687tYuMWhhUCc9YaJ_pjvyuq6eL7LL"));
    assert!(is_radio_id("VLRDCLAK5uy_nBE4bLuBHUXWZrF59ZrkPEToKt8M_I3Vc"));
    assert!(!is_radio_id("PLxA687tYuMWhhUCc9YaJ_pjvyuq6eL7LL"));
    assert!(!is_radio_id("MPREb_NPpc9jtWHdJ"));
    assert!(!is_radio_id("UC2XdaAVUannpujzv32jcouQ"));
    assert!(!is_radio_id(""));
    let source = fixture("radio_RDAMVMJ45RU9Iu8PA.json");
    let radio = CatalogClient::parse_playlist_radio(&source).expect("parse playlist radio");
    let watch = CatalogClient::parse_watch_playlist(&source).expect("parse watch");
    assert_eq!(radio, watch);
    assert_eq!(radio.len(), 50);
    assert_eq!(radio[0].title, "Rain (2022 Stereo Mix)");
    assert_eq!(radio[0].artists, vec!["The Beatles".to_owned()]);
    assert_eq!(radio[1].title, "Psychotic Reaction");
    assert!(radio.iter().all(|t| t.is_valid()));
}

#[test]
fn malformed_discover_fixture_is_parse_error() {
    assert!(CatalogClient::parse_charts("not json").is_err());
    assert!(CatalogClient::parse_charts("{}").is_err());
    assert!(CatalogClient::parse_new_releases("[]").is_err());
    assert!(CatalogClient::parse_new_releases("").is_err());
    assert!(CatalogClient::parse_browse("{\"contents\":[]}").is_err());
    assert!(CatalogClient::parse_browse("garbage").is_err());
    assert!(CatalogClient::parse_mood_playlists("not json").is_err());
    assert!(CatalogClient::parse_mood_playlists("{}").is_err());
    assert!(CatalogClient::parse_playlist_radio("").is_err());
    assert!(CatalogClient::parse_playlist_radio("[]").is_err());
}
