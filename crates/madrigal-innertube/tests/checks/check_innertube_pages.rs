use madrigal_innertube::CatalogClient;
use std::path::PathBuf;

const CHANNEL: &str = "UC2XdaAVUannpujzv32jcouQ";
const ALBUM: &str = "MPREb_NPpc9jtWHdJ";
const PLAYLIST: &str = "VLPLxA687tYuMWhhUCc9YaJ_pjvyuq6eL7LL";
const VIDEO: &str = "J45RU9Iu8PA";

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).expect("read fixture")
}

#[test]
fn artist_fixture_maps_to_page() {
    let source = fixture(&format!("artist_{CHANNEL}.json"));
    let page = CatalogClient::parse_artist_page(CHANNEL, &source).expect("parse artist");
    assert_eq!(page.artist.name, "The Beatles");
    assert_eq!(page.artist.id.as_str(), "UCc4K7bAqpdBP8jh1j9XZAww");
    assert_eq!(page.songs.len(), 5);
    assert_eq!(page.albums.len(), 10);
    assert_eq!(page.songs[0].title, "Let It Be (Remastered 2009)");
    assert_eq!(page.albums[0].title, "Rubber Soul (Super Deluxe)");
    assert!(page.songs.iter().all(|t| t.is_valid()));
    assert!(page.albums.iter().all(|a| a.is_valid()));
}

#[test]
fn album_fixture_maps_to_page() {
    let source = fixture(&format!("album_{ALBUM}.json"));
    let page = CatalogClient::parse_album_page(ALBUM, &source).expect("parse album");
    assert_eq!(page.album.title, "Rubber Soul (Remastered 2009)");
    assert_eq!(page.album.id.as_str(), ALBUM);
    assert_eq!(page.album.year.as_deref(), Some("1965"));
    assert_eq!(page.album.track_count, Some(14));
    assert_eq!(
        page.album.audio_playlist_id.as_deref(),
        Some("OLAK5uy_lirCIxLpU2odTVLZ_Sbo1wZaeu5IStivs")
    );
    assert_eq!(page.tracks.len(), 14);
    let first = &page.tracks[0];
    assert_eq!(first.title, "Drive My Car (Remastered 2009)");
    assert_eq!(first.track_number, Some(1));
    assert_eq!(first.duration_secs, Some(149));
    assert_eq!(first.album.as_deref(), Some("Rubber Soul (Remastered 2009)"));
}

#[test]
fn playlist_fixture_maps_to_page() {
    let details = fixture(&format!("playlist_details_{PLAYLIST}.json"));
    let tracks = fixture(&format!("playlist_tracks_{PLAYLIST}.json"));
    let page =
        CatalogClient::parse_playlist_page(PLAYLIST, &details, &tracks).expect("parse playlist");
    assert_eq!(page.playlist.title, "The Beatles Greatest Hits");
    assert_eq!(page.playlist.author.as_deref(), Some("Revive Music"));
    assert_eq!(page.playlist.track_count, Some(78));
    assert_eq!(page.tracks.len(), 78);
    assert_eq!(
        page.tracks[0].title,
        "Love Me Do (Mono Version / Remastered 2009)"
    );
    assert_eq!(page.tracks[0].track_number, Some(1));
    assert!(page.tracks.iter().all(|t| t.is_valid()));
}

#[test]
fn watch_fixture_maps_to_tracks() {
    let source = fixture(&format!("watch_{VIDEO}.json"));
    let tracks = CatalogClient::parse_watch_playlist(&source).expect("parse watch");
    assert_eq!(tracks.len(), 50);
    assert_eq!(tracks[0].title, "Tomorrow Never Knows (Mono)");
    assert!(tracks.iter().all(|t| t.is_valid()));
}

#[test]
fn moods_fixture_uses_custom_parse() {
    let source = fixture("moods.json");
    let listing = CatalogClient::parse_mood_listing(&source).expect("parse moods");
    assert_eq!(listing.sections.len(), 2);
    assert_eq!(listing.sections[0].name, "Moods & moments");
    assert_eq!(listing.sections[0].entries.len(), 12);
    assert_eq!(listing.sections[0].entries[0].title, "Chill");
    assert_eq!(listing.sections[1].name, "Genres");
    assert_eq!(listing.sections[1].entries.len(), 34);
    assert_eq!(listing.sections[1].entries[0].title, "African");
    assert!(
        listing
            .sections
            .iter()
            .flat_map(|s| &s.entries)
            .all(|e| !e.title.is_empty() && !e.params.is_empty())
    );
}

#[test]
fn malformed_page_fixture_is_parse_error() {
    assert!(CatalogClient::parse_artist_page(CHANNEL, "not json").is_err());
    assert!(CatalogClient::parse_artist_page(CHANNEL, "{}").is_err());
    assert!(CatalogClient::parse_album_page(ALBUM, "[]").is_err());
    assert!(
        CatalogClient::parse_playlist_page(PLAYLIST, "{}", "not json").is_err()
    );
    assert!(CatalogClient::parse_watch_playlist("").is_err());
    assert!(CatalogClient::parse_mood_listing("{\"contents\":[]}").is_err());
    assert!(CatalogClient::parse_mood_listing("garbage").is_err());
}
