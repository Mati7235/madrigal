use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use madrigal_app::catalog::{
    apply_catalog_event, wire_catalog_with, CatalogEvent, CatalogSlot, CatalogSource, ExplorePages,
};
use madrigal_innertube::{CatalogClient, HomeFeed, SearchFilter, SearchOutcome};
use madrigal_settings::Content;
use madrigal_testkit as kit;
use madrigal_ui::MainShell;
use slint::Model;

const TIMEOUT: Duration = Duration::from_secs(5);

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../madrigal-innertube/tests/fixtures")
            .join(name),
    )
    .expect("read fixture")
}

struct FixtureSource;

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
}

fn next_event(rx: &Receiver<CatalogEvent>) -> CatalogEvent {
    rx.recv_timeout(TIMEOUT).expect("catalog event")
}

#[test]
fn app_catalog() {
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, root.path());
    std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", root.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", root.path().join("state"));

    kit::init_headless();
    let shell = MainShell::new().expect("shell");
    let (sink, rx) = mpsc::channel();
    let delivery: madrigal_app::catalog::Delivery = {
        let sink = Arc::new(Mutex::new(sink));
        Arc::new(move |event| {
            if let Ok(sink) = sink.lock() {
                let _ = sink.send(event);
            }
        })
    };
    let wiring =
        wire_catalog_with(&shell, &Content::default(), Box::new(FixtureSource), delivery)
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
    assert_eq!(shell.get_search_results().row_count(), 20, "songs");
    assert_eq!(
        shell.get_search_results().row_data(0).unwrap().title.as_str(),
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

    println!("OK app_catalog");
}
