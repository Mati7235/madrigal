use madrigal_innertube::library::AuthCatalogClient;
use madrigal_innertube::{CatalogClient, Error, HomeItem};
use std::path::PathBuf;
use ytmapi_rs::common::ApiOutcome;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).expect("read fixture")
}

fn track_of(item: &HomeItem) -> (&str, &str) {
    match item {
        HomeItem::Track(track) => (
            track.identity.id.as_str(),
            track.title.as_str(),
        ),
        _ => panic!("expected track"),
    }
}

#[test]
fn home_sections_map_to_models() {
    let feed = CatalogClient::parse_home(&fixture("home.json")).expect("parse home");
    assert_eq!(feed.sections.len(), 2);
    let playlists = &feed.sections[0];
    assert_eq!(playlists.title, "Trending community playlists");
    assert_eq!(playlists.items.len(), 10);
    for item in &playlists.items {
        match item {
            HomeItem::Playlist(playlist) => {
                assert!(playlist.is_valid());
                assert!(playlist.id.as_str().starts_with("VLPL"));
            }
            _ => panic!("expected playlist"),
        }
    }
    match &playlists.items[0] {
        HomeItem::Playlist(playlist) => {
            assert!(playlist.title.starts_with("MUSICA LATINA 2026"));
            assert_eq!(playlist.author.as_deref(), Some("Anvbhjktlcpryn"));
            assert!(playlist.thumbnail.is_some());
        }
        _ => panic!("expected playlist"),
    }
    let picks = &feed.sections[1];
    assert_eq!(picks.title, "Quick picks");
    assert_eq!(picks.items.len(), 20);
    for item in &picks.items {
        match item {
            HomeItem::Track(track) => {
                assert!(track.is_valid());
                assert!(!track.artists.is_empty());
            }
            _ => panic!("expected track"),
        }
    }
    assert_eq!(track_of(&picks.items[0]).0, "1qmJYs4DfgE");
    assert!(picks.items.iter().any(|item| match item {
        HomeItem::Track(track) => track.album.is_some(),
        _ => false,
    }));
}

#[test]
fn liked_songs_map_to_tracks() {
    let tracks =
        AuthCatalogClient::parse_liked_songs(&fixture("get_library_songs_20240701.json"))
            .expect("parse liked");
    assert_eq!(tracks.len(), 25);
    let first = &tracks[0];
    assert_eq!(first.identity.id.as_str(), "0Toq8VrbLTQ");
    assert_eq!(first.title, "The Winter of Our Passing");
    assert_eq!(first.artists, vec!["Katatonia".to_owned()]);
    assert_eq!(first.album.as_deref(), Some("City Burials"));
    assert_eq!(first.duration_secs, Some(199));
    assert!(tracks.iter().all(|t| t.is_valid()));
}

#[test]
fn library_playlists_map_to_playlists() {
    let playlists =
        AuthCatalogClient::parse_library_playlists(&fixture("get_library_playlists.json"))
            .expect("parse playlists");
    assert_eq!(playlists.len(), 2);
    let first = &playlists[0];
    assert_eq!(
        first.id.as_str(),
        "VLPLCZQcydUIP07hMOwAXIag92l76d3z3Thv"
    );
    assert_eq!(first.title, "Listen later");
    assert_eq!(first.author.as_deref(), Some("Nick Dowsett"));
    assert_eq!(first.track_count, Some(20));
    assert_eq!(playlists[1].title, "Deadly Hotlist");
    assert!(playlists.iter().all(|p| p.is_valid()));
}

#[test]
fn history_periods_map_to_tracks() {
    let feed = AuthCatalogClient::parse_history(&fixture("get_history_20240713.json"))
        .expect("parse history");
    let names: Vec<&str> = feed
        .periods
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["Today", "Yesterday", "This week", "Last week", "June 2024"]
    );
    let counts: Vec<usize> = feed.periods.iter().map(|p| p.tracks.len()).collect();
    assert_eq!(counts, vec![7, 12, 68, 68, 44]);
    assert_eq!(feed.periods[0].tracks[0].identity.id.as_str(), "UofrSj07rnI");
    assert!(feed
        .periods
        .iter()
        .flat_map(|p| &p.tracks)
        .all(|t| t.is_valid()));
}

#[test]
fn writes_parse_pinned_outcomes() {
    AuthCatalogClient::parse_rate_song(&fixture("rate_song_like_20240710.json"))
        .expect("parse rate");
    let edited =
        AuthCatalogClient::parse_edit_playlist(&fixture("edit_playlist_title_20240626.json"))
            .expect("parse edit");
    assert_eq!(edited, ApiOutcome::Success);
    let added =
        AuthCatalogClient::parse_add_playlist_items(&fixture("add_playlist_items_20240626.json"))
            .expect("parse add");
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].video_id, "AWCVHllzG9Y");
    assert_eq!(added[0].set_video_id, "F3D73C336952E57D");
    let removed =
        AuthCatalogClient::parse_remove_history_items(&fixture("remove_history_items_20240704.json"))
            .expect("parse remove history");
    assert_eq!(
        removed,
        vec![
            ApiOutcome::Success,
            ApiOutcome::Failure,
            ApiOutcome::Failure,
            ApiOutcome::Failure,
        ]
    );
}

#[test]
fn malformed_fixtures_error() {
    assert!(CatalogClient::parse_home("not json").is_err());
    assert!(CatalogClient::parse_home("{}").is_err());
    assert!(CatalogClient::parse_home("{\"contents\":[]}").is_err());
    assert!(AuthCatalogClient::parse_liked_songs("not json").is_err());
    assert!(AuthCatalogClient::parse_liked_songs("{}").is_err());
    assert!(AuthCatalogClient::parse_library_playlists("not json").is_err());
    assert!(AuthCatalogClient::parse_library_playlists(BAD_GRID).is_err());
    assert!(AuthCatalogClient::parse_history("").is_err());
    assert!(AuthCatalogClient::parse_history("{\"contents\":[]}").is_err());
    assert!(AuthCatalogClient::parse_rate_song("garbage").is_err());
    assert!(AuthCatalogClient::parse_edit_playlist("{}").is_err());
    assert!(AuthCatalogClient::parse_add_playlist_items("not json").is_err());
    assert!(AuthCatalogClient::parse_remove_history_items("").is_err());
}

#[test]
fn account_menu_logged_out_is_not_signed_in() {
    let err = AuthCatalogClient::parse_account_info(&fixture("account_menu_logged_out.json"))
        .expect_err("logged-out menu must not parse as an account");
    assert!(matches!(err, Error::NotSignedIn), "distinct error: {err:?}");
}

#[test]
fn account_menu_parses_structural_logged_in_fixture() {
    let info = AuthCatalogClient::parse_account_info(&fixture(
        "account_menu_logged_in_structural.json",
    ))
    .expect("parse structural logged-in menu");
    assert_eq!(info.name, "Sample User");
    assert_eq!(info.handle.as_deref(), Some("@sampleuser"));
    assert_eq!(info.email.as_deref(), Some("sample@example.com"));
    assert_eq!(info.channel_id.as_deref(), Some("UC00000000000000000000"));
    assert_eq!(
        info.avatar.as_deref(),
        Some("https://yt3.ggpht.com/ytc/sample-user-photo=s176-c-k-c0x00ffffff-no-rj")
    );
}

#[test]
fn account_menu_accepts_email_without_handle() {
    let source = r#"{"actions":[{"openPopupAction":{"popup":{"multiPageMenuRenderer":{"header":{"activeAccountHeaderRenderer":{"accountName":{"runs":[{"text":"Sample User"}]},"email":{"simpleText":"sample@example.com"}}}}}}}]}"#;
    let info = AuthCatalogClient::parse_account_info(source).expect("parse email header");
    assert_eq!(info.name, "Sample User");
    assert_eq!(info.email.as_deref(), Some("sample@example.com"));
    assert_eq!(info.handle, None);
    assert_eq!(info.channel_id, None);
    assert_eq!(info.avatar, None);
}

#[test]
fn account_menu_malformed_errors() {
    assert!(AuthCatalogClient::parse_account_info("not json").is_err());
    assert!(matches!(
        AuthCatalogClient::parse_account_info("{}"),
        Err(Error::NotSignedIn)
    ));
    assert!(matches!(
        AuthCatalogClient::parse_account_info(
            r#"{"activeAccountHeaderRenderer":{"channelHandle":{"runs":[{"text":"@x"}]}}}"#
        ),
        Err(Error::Fixture(_))
    ));
}

const FAKE_COOKIE: &str = "SAPISID=fake_sapisid_123; __Secure-3PAPISID=fake_secure_456; LOGIN_INFO=fake_login_789; SID=fake_sid";

const BAD_GRID: &str = "{\"contents\":{\"singleColumnBrowseResultsRenderer\":{\"tabs\":[{\"tabRenderer\":{\"content\":{\"sectionListRenderer\":{\"contents\":[{\"gridRenderer\":{\"items\":{}}}]}}}}]}}}";

#[test]
fn cookie_upgrade_builds_without_network() {
    let _upgraded = CatalogClient::with_cookies(FAKE_COOKIE).expect("upgrade");
    assert!(CatalogClient::with_cookies("").is_err());
    assert!(CatalogClient::with_cookies("SID=only_a_sid; PREF=tz=UTC").is_err());
    assert!(CatalogClient::with_cookies("SAPISID=abc; SID=def").is_err());
}
