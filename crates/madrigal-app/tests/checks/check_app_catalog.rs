use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use madrigal_app::catalog::{
    apply_catalog_event, playable_track, wire_catalog_with, CatalogEvent, CatalogSlot,
    CatalogSource, DetailPage, ExplorePages,
};
use madrigal_innertube::{CatalogClient, HomeFeed, SearchFilter, SearchOutcome};
use madrigal_settings::Content;
use madrigal_testkit as kit;
use madrigal_ui::{DetailKind, DetailRef, MainShell};
use slint::Model;

const TIMEOUT: Duration = Duration::from_secs(5);
const ARTIST_ID: &str = "UC2XdaAVUannpujzv32jcouQ";
const ALBUM_ID: &str = "MPREb_NPpc9jtWHdJ";
const PLAYLIST_ID: &str = "VLPLxA687tYuMWhhUCc9YaJ_pjvyuq6eL7LL";

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../madrigal-innertube/tests/fixtures")
            .join(name),
    )
    .expect("read fixture")
}

struct FixtureSource {
    fail_detail: Arc<AtomicBool>,
    fail_search: Arc<AtomicBool>,
}

impl CatalogSource for FixtureSource {
    fn home(&mut self) -> Result<HomeFeed, madrigal_innertube::Error> {
        CatalogClient::parse_home(&fixture("home.json"))
    }

    fn suggestions(&mut self, _query: &str) -> Result<Vec<String>, madrigal_innertube::Error> {
        CatalogClient::parse_suggestions(&fixture("suggestions_beat.json"))
    }

    fn search(
        &mut self,
        _query: &str,
        filter: SearchFilter,
    ) -> Result<SearchOutcome, madrigal_innertube::Error> {
        if self.fail_search.load(Ordering::SeqCst) {
            return Err(madrigal_innertube::Error::Fixture(
                "fixture search failure".to_owned(),
            ));
        }
        match filter {
            SearchFilter::Songs => Ok(SearchOutcome::Songs(CatalogClient::parse_search_songs(
                &fixture("search_songs_beatles.json"),
            )?)),
            SearchFilter::Artists => Ok(SearchOutcome::Artists(
                CatalogClient::parse_search_artists(&fixture("search_artists_beatles.json"))?,
            )),
            SearchFilter::Albums => Ok(SearchOutcome::Albums(CatalogClient::parse_search_albums(
                &fixture("search_albums_beatles.json"),
            )?)),
        }
    }

    fn explore(&mut self, country: &str) -> Result<ExplorePages, madrigal_innertube::Error> {
        let mut charts = CatalogClient::parse_charts(&fixture("charts_ZZ.json"))?;
        charts.country = country.to_owned();
        Ok(ExplorePages {
            moods: CatalogClient::parse_mood_listing(&fixture("moods.json"))?,
            releases: CatalogClient::parse_new_releases(&fixture("explore.json"))?,
            browse: CatalogClient::parse_browse(&fixture("explore.json"))?,
            charts,
        })
    }

    fn detail(
        &mut self,
        kind: DetailKind,
        id: &str,
    ) -> Result<DetailPage, madrigal_innertube::Error> {
        if self.fail_detail.load(Ordering::SeqCst) {
            return Err(madrigal_innertube::Error::Fixture(
                "fixture detail failure".to_owned(),
            ));
        }
        match kind {
            DetailKind::Artist => Ok(DetailPage::Artist(CatalogClient::parse_artist_page(
                id,
                &fixture("artist_UC2XdaAVUannpujzv32jcouQ.json"),
            )?)),
            DetailKind::Album => Ok(DetailPage::Album(CatalogClient::parse_album_page(
                id,
                &fixture("album_MPREb_NPpc9jtWHdJ.json"),
            )?)),
            DetailKind::Playlist => Ok(DetailPage::Playlist(CatalogClient::parse_playlist_page(
                id,
                &fixture("playlist_details_VLPLxA687tYuMWhhUCc9YaJ_pjvyuq6eL7LL.json"),
                &fixture("playlist_tracks_VLPLxA687tYuMWhhUCc9YaJ_pjvyuq6eL7LL.json"),
            )?)),
            DetailKind::Browse => {
                if id.starts_with("charts:") {
                    Ok(DetailPage::Browse(CatalogClient::parse_charts_page(&fixture(
                        "charts_ZZ.json",
                    ))?))
                } else {
                    Ok(DetailPage::Browse(CatalogClient::parse_browse(&fixture(
                        "mood_playlists_chill.json",
                    ))?))
                }
            }
            DetailKind::Podcast => Err(madrigal_innertube::Error::Fixture(
                "podcast detail not in fixtures".to_owned(),
            )),
        }
    }
}

fn next_event(rx: &Receiver<CatalogEvent>) -> CatalogEvent {
    rx.recv_timeout(TIMEOUT).expect("catalog event")
}

fn click_nth(shell: &MainShell, id: &str, n: usize) {
    let mut found = kit::ElementHandle::find_by_element_id(shell, id);
    let handle = found
        .nth(n)
        .unwrap_or_else(|| panic!("element {id} #{n} missing"));
    kit::click(&handle);
}

#[test]
fn app_catalog() {
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, root.path());
    std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", root.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", root.path().join("state"));
    std::env::remove_var("LC_ALL");
    std::env::remove_var("LC_MESSAGES");
    std::env::remove_var("LANG");

    kit::init_headless();
    let shell = MainShell::new().expect("shell");
    let fail_detail = Arc::new(AtomicBool::new(false));
    let fail_search = Arc::new(AtomicBool::new(false));
    let (sink, rx) = mpsc::channel();
    let delivery: madrigal_app::catalog::Delivery = {
        let sink = Arc::new(Mutex::new(sink));
        Arc::new(move |event| {
            if let Ok(sink) = sink.lock() {
                let _ = sink.send(event);
            }
        })
    };
    let wiring = wire_catalog_with(
        &shell,
        &Content::default(),
        Box::new(FixtureSource {
            fail_detail: Arc::clone(&fail_detail),
            fail_search: Arc::clone(&fail_search),
        }),
        delivery,
    )
    .expect("wire catalog");

    let apply = |event: CatalogEvent| {
        apply_catalog_event(
            &shell,
            event,
            &wiring.generations(),
            &wiring.home(),
            true,
        );
    };

    for _ in 0..2 {
        apply(next_event(&rx));
    }

    assert_eq!(shell.get_home_sections().row_count(), 2, "home sections");
    assert_eq!(shell.get_home_chips().row_count(), 3, "home chips");
    assert_eq!(
        shell.get_home_chips().row_data(0).unwrap().as_str(),
        "Todo",
        "home chip uses i18n"
    );
    assert_eq!(
        shell.get_home_quick_picks().row_count(),
        20,
        "home quick picks"
    );
    assert_eq!(shell.get_explore_moods_left().row_count(), 23, "moods left");
    assert_eq!(shell.get_explore_moods_right().row_count(), 23, "moods right");
    assert_eq!(
        shell.get_explore_releases_albums().row_count(),
        24,
        "releases albums"
    );
    assert_eq!(
        shell.get_explore_releases_videos().row_count(),
        24,
        "releases videos"
    );
    assert_eq!(
        shell.get_explore_browse_sections().row_count(),
        3,
        "browse sections"
    );
    assert_eq!(shell.get_explore_charts_id().as_str(), "charts:US");

    shell.invoke_chip_selected(1);
    assert_eq!(shell.get_home_current_chip(), 1);
    assert_eq!(shell.get_home_sections().row_count(), 1, "filtered chip");
    shell.invoke_chip_selected(0);
    assert_eq!(shell.get_home_sections().row_count(), 2, "all chip");

    shell.set_search_query("beatles".into());
    shell.invoke_query_changed("beatles".into());
    for _ in 0..2 {
        apply(next_event(&rx));
    }
    assert_eq!(shell.get_search_results().row_count(), 24, "mixed results");
    assert_eq!(
        shell.get_search_results().row_data(0).unwrap().title.as_str(),
        "The Beatles"
    );
    assert_eq!(
        shell.get_search_results().row_data(8).unwrap().title.as_str(),
        "Rubber Soul (Remastered 2009)"
    );
    assert_eq!(
        shell.get_search_results().row_data(16).unwrap().title.as_str(),
        "Tomorrow Never Knows (Mono)"
    );
    assert_eq!(
        shell.get_search_suggestions().row_count(),
        1,
        "suggestions"
    );
    assert_eq!(
        shell.get_search_suggestions().row_data(0).unwrap().as_str(),
        "beatles"
    );

    shell.invoke_filter_selected(2);
    apply(next_event(&rx));
    assert_eq!(shell.get_search_results().row_count(), 19, "artists");
    assert_eq!(
        shell.get_search_results().row_data(0).unwrap().title.as_str(),
        "The Beatles"
    );

    shell.invoke_filter_selected(3);
    apply(next_event(&rx));
    assert_eq!(shell.get_search_results().row_count(), 20, "albums");
    assert_eq!(
        shell.get_search_results().row_data(0).unwrap().title.as_str(),
        "Rubber Soul (Remastered 2009)"
    );

    shell.invoke_filter_selected(4);
    assert_eq!(
        shell.get_search_results().row_count(),
        0,
        "playlist filter is unsupported"
    );
    assert!(
        rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "playlist filter must not hit the source"
    );

    let current = wiring.generations().begin(CatalogSlot::Search);
    apply(CatalogEvent::Search {
        request_id: current,
        outcome: SearchOutcome::Songs(Vec::new()),
    });
    assert_eq!(shell.get_search_results().row_count(), 0, "current event");

    let stale = wiring.generations().begin(CatalogSlot::Search);
    let _ = wiring.generations().begin(CatalogSlot::Search);
    apply(CatalogEvent::Search {
        request_id: stale,
        outcome: SearchOutcome::Songs(
            CatalogClient::parse_search_songs(&fixture("search_songs_beatles.json"))
                .expect("songs"),
        ),
    });
    assert_eq!(
        shell.get_search_results().row_count(),
        0,
        "stale event dropped"
    );

    shell.invoke_filter_selected(1);
    apply(next_event(&rx));
    assert_eq!(shell.get_search_results().row_count(), 20, "songs again");

    shell.invoke_source_selected(0);
    assert_eq!(
        shell.get_search_results().row_count(),
        0,
        "local source clears results"
    );
    assert_eq!(
        shell.get_search_suggestions().row_count(),
        0,
        "local source clears suggestions"
    );
    assert!(
        rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "local source must not hit the source"
    );

    shell.invoke_source_selected(1);
    for _ in 0..2 {
        apply(next_event(&rx));
    }
    assert_eq!(shell.get_search_results().row_count(), 20, "online source");

    fail_search.store(true, Ordering::SeqCst);
    shell.invoke_query_changed("beatles".into());
    for _ in 0..2 {
        apply(next_event(&rx));
    }
    assert!(shell.get_search_error(), "failed search is reported");
    assert_eq!(shell.get_search_results().row_count(), 0, "failed search empty");
    fail_search.store(false, Ordering::SeqCst);
    shell.invoke_search_retry();
    for _ in 0..2 {
        apply(next_event(&rx));
    }
    assert!(!shell.get_search_error(), "retry clears the error");
    assert_eq!(shell.get_search_results().row_count(), 20, "retry results");

    shell.invoke_query_changed("".into());
    assert_eq!(
        shell.get_search_results().row_count(),
        0,
        "empty query clears results"
    );
    assert_eq!(
        shell.get_search_suggestions().row_count(),
        0,
        "empty query clears suggestions"
    );
    assert!(
        rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "empty query must not hit the source"
    );

    let played: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let played = played.clone();
        shell.on_play_requested(move |item| {
            played.borrow_mut().push(item.id.to_string());
        });
    }
    let reference = |kind, id: &str, title: &str| DetailRef {
        kind,
        id: id.into(),
        title: title.into(),
    };

    shell.set_page(1);
    shell.invoke_detail_requested(reference(DetailKind::Artist, ARTIST_ID, "The Beatles"));
    assert!(shell.get_detail_loading(), "artist starts loading");
    assert!(shell.get_detail_error().is_empty());
    apply(next_event(&rx));
    assert!(!shell.get_detail_loading(), "artist finished loading");
    assert_eq!(shell.get_detail_title().as_str(), "The Beatles");
    assert_eq!(
        shell.get_detail_subtitle().as_str(),
        "9,8 M suscriptores"
    );
    assert_eq!(shell.get_detail_tracks().row_count(), 5);
    assert_eq!(shell.get_detail_albums().row_count(), 10);
    assert_eq!(shell.get_detail_tab(), 0);
    let first_song = shell.get_detail_tracks().row_data(0).unwrap();
    assert_eq!(
        playable_track(&first_song).unwrap().identity.id.as_str(),
        first_song.id.as_str()
    );
    click_nth(&shell, "DetailPage::song-row", 0);
    assert_eq!(played.borrow().last().unwrap(), first_song.id.as_str());
    click_nth(&shell, "DetailPage::artist-chip", 1);
    assert_eq!(shell.get_detail_tab(), 1);
    click_nth(&shell, "DetailPage::album-touch", 0);
    assert!(shell.get_detail_loading(), "album from artist starts loading");
    apply(next_event(&rx));
    assert_eq!(
        shell.get_detail_title().as_str(),
        "Rubber Soul (Remastered 2009)"
    );
    assert_eq!(shell.get_detail_tracks().row_count(), 14);
    click_nth(&shell, "DetailPage::track-row", 2);
    let third_track = shell.get_detail_tracks().row_data(2).unwrap();
    assert_eq!(played.borrow().last().unwrap(), third_track.id.as_str());

    shell.invoke_detail_requested(reference(
        DetailKind::Playlist,
        PLAYLIST_ID,
        "The Beatles Greatest Hits",
    ));
    apply(next_event(&rx));
    assert_eq!(
        shell.get_detail_title().as_str(),
        "The Beatles Greatest Hits"
    );
    assert_eq!(shell.get_detail_subtitle().as_str(), "Revive Music · 78");
    assert_eq!(shell.get_detail_tracks().row_count(), 78);

    shell.invoke_detail_requested(reference(DetailKind::Browse, "ggMPOg1uX1JOQWZFeDByc2Jm", "Chill"));
    apply(next_event(&rx));
    assert_eq!(shell.get_detail_title().as_str(), "Chill");
    assert_eq!(shell.get_detail_sections().row_count(), 15);
    assert_eq!(
        shell
            .get_detail_sections()
            .row_data(0)
            .unwrap()
            .title
            .as_str(),
        "Coffee shop blends"
    );

    shell.invoke_detail_requested(reference(DetailKind::Browse, "charts:ZZ", "Rankings"));
    apply(next_event(&rx));
    assert_eq!(shell.get_detail_title().as_str(), "Rankings");
    assert_eq!(shell.get_detail_sections().row_count(), 2);
    assert_eq!(
        shell
            .get_detail_sections()
            .row_data(1)
            .unwrap()
            .title
            .as_str(),
        "Top artists"
    );

    shell.invoke_detail_requested(reference(DetailKind::Artist, ARTIST_ID, "Stale artist"));
    let stale = next_event(&rx);
    shell.invoke_detail_requested(reference(DetailKind::Album, ALBUM_ID, "Pending album"));
    assert_eq!(shell.get_detail_title().as_str(), "Pending album");
    let fresh = next_event(&rx);
    apply(stale);
    assert_eq!(
        shell.get_detail_title().as_str(),
        "Pending album",
        "stale artist response must not replace the pending album"
    );
    assert!(shell.get_detail_loading(), "pending album still loading");
    apply(fresh);
    assert_eq!(
        shell.get_detail_title().as_str(),
        "Rubber Soul (Remastered 2009)"
    );
    assert!(!shell.get_detail_loading());

    fail_detail.store(true, Ordering::SeqCst);
    shell.invoke_detail_requested(reference(DetailKind::Artist, ARTIST_ID, "Fail artist"));
    apply(next_event(&rx));
    assert!(!shell.get_detail_loading(), "failed load stops loading");
    assert!(!shell.get_detail_error().is_empty(), "error is visible");
    assert_eq!(shell.get_detail_tracks().row_count(), 0);
    assert_eq!(shell.get_detail_title().as_str(), "Fail artist");
    fail_detail.store(false, Ordering::SeqCst);
    click_nth(&shell, "DetailPage::detail-retry", 0);
    assert!(shell.get_detail_loading(), "retry starts loading");
    apply(next_event(&rx));
    assert!(shell.get_detail_error().is_empty(), "retry clears error");
    assert_eq!(shell.get_detail_title().as_str(), "The Beatles");
    assert_eq!(shell.get_detail_tracks().row_count(), 5);

    shell.invoke_detail_requested(reference(DetailKind::Artist, ARTIST_ID, "Nav artist"));
    let pending = next_event(&rx);
    assert_eq!(shell.get_detail_stack(), 1);
    click_nth(&shell, "SideNav::nav-home", 0);
    assert_eq!(shell.get_detail_stack(), 0, "nav closes the detail page");
    assert_eq!(shell.get_page(), 0);
    apply(pending);
    assert_eq!(
        shell.get_detail_tracks().row_count(),
        0,
        "detail closed while loading drops the response"
    );

    println!("OK app_catalog");
}
