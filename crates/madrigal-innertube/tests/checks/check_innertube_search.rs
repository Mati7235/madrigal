use madrigal_innertube::{CatalogClient, SearchFilter, SearchOutcome};
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).expect("read fixture")
}

#[test]
fn search_songs_fixture_maps_to_tracks() {
    let source = fixture("search_songs_beatles.json");
    let tracks = CatalogClient::parse_search_songs(&source).expect("parse songs");
    assert_eq!(tracks.len(), 20);
    let first = &tracks[0];
    assert_eq!(first.title, "Tomorrow Never Knows (Mono)");
    assert_eq!(first.artists, vec!["The Beatles".to_owned()]);
    assert_eq!(first.album.as_deref(), Some("Revolver (Deluxe)"));
    assert_eq!(first.duration_secs, Some(178));
    assert!(tracks.iter().all(|t| t.is_valid() && t.is_remote()));
    assert!(tracks.iter().all(|t| t.thumbnail.as_ref().is_some_and(|u| u
        .starts_with("https://"))));
    assert_eq!(
        first.thumbnail.as_deref(),
        Some("https://yt3.googleusercontent.com/WbzeDYz9fRAzx_6-Ix97oujwsPzpC5z3dtSd0uftkwu1sMpoGykm7T6zx2UT3iCQw-uVxJ-OaFHtQGT9=w120-h120-l90-rj")
    );
}

#[test]
fn search_artists_fixture_maps_to_artists() {
    let source = fixture("search_artists_beatles.json");
    let artists = CatalogClient::parse_search_artists(&source).expect("parse artists");
    assert_eq!(artists.len(), 19);
    let first = &artists[0];
    assert_eq!(first.name, "The Beatles");
    assert_eq!(first.id.as_str(), "UC2XdaAVUannpujzv32jcouQ");
    assert_eq!(
        first.subscribers.as_deref(),
        Some("55.5M monthly audience")
    );
    assert_eq!(
        first.thumbnail.as_deref(),
        Some("https://lh3.googleusercontent.com/z8KZsHNKS-O1qYVyKlSErT_RLMSMwVht89USvSdFAd0EoRlBOppi9DOdRkv609Ye_tfq_Wp8WwhVJbw=w120-h120-p-l90-rj")
    );
    assert!(artists.iter().all(|a| a.is_valid()));
}

#[test]
fn search_albums_fixture_maps_to_albums() {
    let source = fixture("search_albums_beatles.json");
    let albums = CatalogClient::parse_search_albums(&source).expect("parse albums");
    assert_eq!(albums.len(), 20);
    let first = &albums[0];
    assert_eq!(first.title, "Rubber Soul (Remastered 2009)");
    assert_eq!(first.artists, vec!["The Beatles".to_owned()]);
    assert_eq!(first.year.as_deref(), Some("1965"));
    assert_eq!(
        first.thumbnail.as_deref(),
        Some("https://yt3.googleusercontent.com/IjEb5kMRk7qimcH3BCJaK85pf79X383z5KNrAxJSNOHVWC41_e66Id233ZGj1AQhH1H5AiKxQMEbHPn0=w544-h544-l90-rj")
    );
    assert!(albums.iter().all(|a| a.is_valid()));
}

#[test]
fn suggestions_fixture_maps_to_strings() {
    let source = fixture("suggestions_beat.json");
    let suggestions = CatalogClient::parse_suggestions(&source).expect("parse suggestions");
    assert_eq!(
        suggestions,
        vec![
            "beat it".to_owned(),
            "beatles".to_owned(),
            "beat boom bap".to_owned(),
            "beat".to_owned(),
            "beat trap".to_owned(),
            "beatle attaque 77".to_owned(),
        ]
    );
}

#[test]
fn filter_enum_selects_result_kind() {
    let songs = fixture("search_songs_beatles.json");
    let artists = fixture("search_artists_beatles.json");
    let albums = fixture("search_albums_beatles.json");
    match CatalogClient::parse_search(SearchFilter::Songs, &songs).expect("songs outcome") {
        SearchOutcome::Songs(tracks) => assert_eq!(tracks.len(), 20),
        other => panic!("wrong outcome: {other:?}"),
    }
    match CatalogClient::parse_search(SearchFilter::Artists, &artists).expect("artists outcome") {
        SearchOutcome::Artists(found) => assert_eq!(found.len(), 19),
        other => panic!("wrong outcome: {other:?}"),
    }
    match CatalogClient::parse_search(SearchFilter::Albums, &albums).expect("albums outcome") {
        SearchOutcome::Albums(found) => assert_eq!(found.len(), 20),
        other => panic!("wrong outcome: {other:?}"),
    }
    assert_ne!(SearchFilter::Songs, SearchFilter::Artists);
}

#[test]
fn malformed_search_fixture_is_parse_error() {
    assert!(CatalogClient::parse_search_songs("not json").is_err());
    assert!(CatalogClient::parse_search_songs("{}").is_err());
    assert!(CatalogClient::parse_search_artists("[]").is_err());
    assert!(CatalogClient::parse_search_albums("").is_err());
    assert!(CatalogClient::parse_suggestions("{\"contents\":[]}").is_err());
    assert!(CatalogClient::parse_search(SearchFilter::Songs, "garbage").is_err());
}
