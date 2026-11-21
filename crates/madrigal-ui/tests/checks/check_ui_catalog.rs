use madrigal_testkit as kit;
use madrigal_ui::{
    apply_browse_detail, apply_detail_error, apply_home, apply_home_chip, apply_home_chips,
    apply_suggestions, begin_detail_load, close_detail, dispatch_key, is_detail_open,
    load_album_fixture, load_artist_fixture, load_browse_fixture, load_explore_fixtures,
    load_home_fixture, load_playlist_fixture, load_search_fixture, load_suggestions_fixture,
    mixed_results, playlist_item, set_shell_lang, subscriber_label, wire_shell_i18n, DetailKind,
    DetailRef, MainShell, MIXED_PER_KIND,
};
use slint::{ComponentHandle, Model, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

const ALBUM_ID: &str = "MPREb_NPpc9jtWHdJ";
const ARTIST_ID: &str = "UC2XdaAVUannpujzv32jcouQ";
const PLAYLIST_ID: &str = "VLPLxA687tYuMWhhUCc9YaJ_pjvyuq6eL7LL";

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../madrigal-innertube/tests/fixtures")
            .join(name),
    )
    .expect("read fixture")
}

fn click_nth(shell: &MainShell, id: &str, n: usize) {
    let mut found = kit::ElementHandle::find_by_element_id(shell, id);
    let handle = found
        .nth(n)
        .unwrap_or_else(|| panic!("element {id} #{n} missing"));
    kit::click(&handle);
}

fn trl(shell: &MainShell, lang: &str, key: &str) -> String {
    shell
        .global::<madrigal_ui::I18n>()
        .invoke_trl(lang.into(), key.into())
        .as_str()
        .to_owned()
}

fn snapshot(shell: &MainShell, dir: &std::path::Path, name: &str) {
    let shot = dir.join(name);
    kit::snapshot_to(shell.window(), &shot).unwrap();
    let bytes = std::fs::read(&shot).unwrap();
    assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    assert!(bytes.len() > 4096, "{name} snapshot too small");
}

#[test]
fn ui_catalog() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", tmp.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", tmp.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", tmp.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", tmp.path().join("state"));
    let snapshot_dir = kit::snapshot_dir(tmp.path());

    kit::init_headless();
    let shell = MainShell::new().unwrap();
    let lang = wire_shell_i18n(&shell, madrigal_i18n::Lang::Es);

    assert_eq!(trl(&shell, "es", "catalog.albums"), "Álbumes");
    assert_eq!(trl(&shell, "en", "catalog.albums"), "Albums");
    assert_eq!(trl(&shell, "es", "home.empty"), "No hay contenido para mostrar");
    assert_eq!(
        trl(&shell, "en", "home.empty"),
        "There is no content to show"
    );
    assert_eq!(
        trl(&shell, "es", "search.section.empty"),
        "Nada en esta categoría"
    );
    assert_eq!(trl(&shell, "es", "explore.empty"), "No hay categorías disponibles.");
    assert_eq!(trl(&shell, "en", "explore.empty"), "No categories are available.");
    assert_eq!(trl(&shell, "es", "detail.empty"), "Nada para mostrar acá");
    assert_eq!(trl(&shell, "en", "detail.empty"), "Nothing to show here");
    assert_eq!(trl(&shell, "es", "catalog.moods"), "Ánimos y géneros");
    assert_eq!(trl(&shell, "en", "catalog.moods"), "Moods and genres");
    assert_eq!(trl(&shell, "es", "catalog.quickPicks"), "Selección rápida");
    assert_eq!(trl(&shell, "es", "catalog.pins"), "Acceso rápido");
    assert_eq!(trl(&shell, "es", "source.local"), "Local");
    assert_eq!(trl(&shell, "es", "catalog.online"), "En línea");
    assert_eq!(trl(&shell, "es", "catalog.filter.all"), "Todo");
    assert_eq!(trl(&shell, "es", "detail.play"), "Reproducir");
    assert_eq!(trl(&shell, "en", "detail.play"), "Play");
    assert_eq!(trl(&shell, "es", "home.section.newReleases"), "Novedades");
    assert_eq!(trl(&shell, "en", "home.section.newReleases"), "New releases");
    set_shell_lang(&shell, &lang, madrigal_i18n::Lang::En);
    assert_eq!(trl(&shell, "en", "catalog.charts"), "Charts");
    set_shell_lang(&shell, &lang, madrigal_i18n::Lang::Es);

    assert_eq!(shell.get_home_sections().row_count(), 0);
    assert_eq!(shell.get_home_quick_picks().row_count(), 0);
    assert_eq!(shell.get_home_pins().row_count(), 0);
    assert!(!is_detail_open(&shell));

    let home_src = Rc::new(fixture("home.json"));
    let songs_src = Rc::new(fixture("search_songs_beatles.json"));
    let artists_src = Rc::new(fixture("search_artists_beatles.json"));
    let albums_src = Rc::new(fixture("search_albums_beatles.json"));
    let suggestions_src = Rc::new(fixture("suggestions_beat.json"));
    let moods_src = Rc::new(fixture("moods.json"));
    let explore_src = Rc::new(fixture("explore.json"));
    let charts_src = Rc::new(fixture("charts_ZZ.json"));
    let artist_src = Rc::new(fixture("artist_UC2XdaAVUannpujzv32jcouQ.json"));
    let album_src = Rc::new(fixture("album_MPREb_NPpc9jtWHdJ.json"));
    let playlist_details_src = Rc::new(fixture(
        "playlist_details_VLPLxA687tYuMWhhUCc9YaJ_pjvyuq6eL7LL.json",
    ));
    let playlist_tracks_src = Rc::new(fixture(
        "playlist_tracks_VLPLxA687tYuMWhhUCc9YaJ_pjvyuq6eL7LL.json",
    ));
    let mood_playlists_src = Rc::new(fixture("mood_playlists_chill.json"));

    let played: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let accepted: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let queries: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let opened: Rc<RefCell<Vec<(DetailKind, String)>>> = Rc::new(RefCell::new(Vec::new()));
    let closed: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));

    {
        let shell_ref = shell.as_weak();
        let home_src = home_src.clone();
        shell.on_chip_selected(move |chip| {
            let shell = shell_ref.unwrap();
            let feed = madrigal_innertube::CatalogClient::parse_home(&home_src).unwrap();
            apply_home_chip(&shell, chip, &feed);
        });
    }
    {
        let shell_ref = shell.as_weak();
        let suggestions_src = suggestions_src.clone();
        let queries = queries.clone();
        shell.on_query_changed(move |text| {
            let shell = shell_ref.unwrap();
            queries.borrow_mut().push(text.to_string());
            let all = madrigal_innertube::CatalogClient::parse_suggestions(&suggestions_src)
                .unwrap();
            apply_suggestions(&shell, &text, &all);
        });
    }
    {
        let accepted = accepted.clone();
        shell.on_suggestion_accepted(move |text| {
            accepted.borrow_mut().push(text.to_string());
        });
    }
    {
        let shell_ref = shell.as_weak();
        let songs_src = songs_src.clone();
        let artists_src = artists_src.clone();
        let albums_src = albums_src.clone();
        let mood_playlists_src = mood_playlists_src.clone();
        shell.on_filter_selected(move |tab| {
            let shell = shell_ref.unwrap();
            let items = match tab {
                2 => madrigal_ui::artist_results(
                    &madrigal_innertube::CatalogClient::parse_search_artists(&artists_src)
                        .unwrap(),
                    madrigal_i18n::Lang::Es,
                ),
                3 => madrigal_ui::album_results(
                    &madrigal_innertube::CatalogClient::parse_search_albums(&albums_src)
                        .unwrap(),
                ),
                4 => madrigal_innertube::CatalogClient::parse_mood_playlists(&mood_playlists_src)
                    .unwrap()
                    .categories[0]
                    .playlists
                    .iter()
                    .map(playlist_item)
                    .collect(),
                _ => madrigal_ui::song_results(
                    &madrigal_innertube::CatalogClient::parse_search_songs(&songs_src)
                        .unwrap(),
                ),
            };
            madrigal_ui::apply_search_results(&shell, items);
        });
    }
    {
        let played = played.clone();
        shell.on_play_requested(move |item| {
            played.borrow_mut().push(item.id.to_string());
        });
    }
    {
        let shell_ref = shell.as_weak();
        let opened = opened.clone();
        let artist_src = artist_src.clone();
        let album_src = album_src.clone();
        let playlist_details_src = playlist_details_src.clone();
        let playlist_tracks_src = playlist_tracks_src.clone();
        let explore_src = explore_src.clone();
        shell.on_detail_opened(move |detail| {
            opened
                .borrow_mut()
                .push((detail.kind, detail.id.to_string()));
            let shell = shell_ref.unwrap();
            match detail.kind {
                DetailKind::Artist => {
                    load_artist_fixture(&shell, ARTIST_ID, &artist_src).unwrap()
                }
                DetailKind::Album => {
                    load_album_fixture(&shell, ALBUM_ID, &album_src).unwrap()
                }
                DetailKind::Playlist => load_playlist_fixture(
                    &shell,
                    PLAYLIST_ID,
                    &playlist_details_src,
                    &playlist_tracks_src,
                )
                .unwrap(),
                DetailKind::Browse => {
                    load_browse_fixture(&shell, &explore_src, "Browse").unwrap()
                }
                DetailKind::Podcast => {}
            }
        });
    }
    {
        let closed = closed.clone();
        shell.on_detail_closed(move || {
            *closed.borrow_mut() += 1;
        });
    }

    shell.set_page(0);
    let feed = load_home_fixture(&shell, &home_src).unwrap();
    assert_eq!(feed.sections.len(), 2);
    assert_eq!(feed.sections[0].title, "Trending community playlists");
    assert_eq!(feed.sections[1].title, "Quick picks");
    apply_home_chips(
        &shell,
        &feed,
        &madrigal_settings::Content::default(),
        madrigal_i18n::Lang::Es,
    );
    assert_eq!(shell.get_home_chips().row_count(), 3);
    assert_eq!(shell.get_home_chips().row_data(0).unwrap().as_str(), "Todo");
    assert_eq!(shell.get_home_sections().row_count(), 2);
    assert_eq!(shell.get_home_quick_picks().row_count(), 20);
    let first_pick = shell.get_home_quick_picks().row_data(0).unwrap();
    assert_eq!(first_pick.id.as_str(), "1qmJYs4DfgE");
    assert!(!first_pick.title.is_empty());
    assert!(first_pick.playable);

    let tracks: Vec<madrigal_core::Track> = feed
        .sections
        .iter()
        .flat_map(|s| s.items.iter())
        .filter_map(|item| match item {
            madrigal_innertube::HomeItem::Track(t) => Some(t.clone()),
            _ => None,
        })
        .collect();
    let pin_ids: Vec<String> = tracks.iter().take(2).map(|t| t.identity.id.clone()).collect();
    madrigal_ui::apply_pins(&shell, &pin_ids, &tracks);
    assert_eq!(shell.get_home_pins().row_count(), 2);
    assert_eq!(
        shell.get_home_pins().row_data(0).unwrap().id.as_str(),
        pin_ids[0].as_str()
    );

    assert!(shell.get_home_card_layout());
    shell.set_home_card_layout(false);
    assert!(!shell.get_home_card_layout());
    shell.set_home_card_layout(true);

    click_nth(&shell, "HomePage::home-chip", 2);
    assert_eq!(shell.get_home_current_chip(), 2);
    assert_eq!(shell.get_home_sections().row_count(), 1);
    assert_eq!(
        shell
            .get_home_sections()
            .row_data(0)
            .unwrap()
            .title
            .as_str(),
        "Quick picks"
    );
    click_nth(&shell, "HomePage::home-chip", 0);
    assert_eq!(shell.get_home_current_chip(), 0);
    assert_eq!(shell.get_home_sections().row_count(), 2);

    click_nth(&shell, "HomePage::pick-card", 0);
    assert_eq!(shell.get_home_selected_item(), 0);
    assert_eq!(played.borrow().last().unwrap().as_str(), "1qmJYs4DfgE");
    click_nth(&shell, "HomePage::pin-chip", 0);
    assert_eq!(shell.get_home_selected_item(), 10000);
    assert_eq!(played.borrow().last().unwrap().as_str(), pin_ids[0].as_str());

    let missing = kit::click_by_id(&shell, "HomePage::no-existe");
    assert!(matches!(missing, Err(kit::Error::ElementNotFound(_))));

    shell.set_page(1);
    let (songs, artists, albums) =
        load_search_fixture(&shell, &songs_src, &artists_src, &albums_src).unwrap();
    assert_eq!(songs.len(), 20);
    assert_eq!(artists.len(), 19);
    assert_eq!(albums.len(), 20);
    assert_eq!(shell.get_search_results().row_count(), 20);
    let all_suggestions = load_suggestions_fixture(&shell, &suggestions_src).unwrap();
    assert_eq!(all_suggestions.len(), 6);

    shell.set_search_query("beat".into());
    shell.invoke_query_changed("beat".into());
    assert_eq!(shell.get_search_suggestions().row_count(), 6);
    assert_eq!(
        shell
            .get_search_suggestions()
            .row_data(0)
            .unwrap()
            .as_str(),
        "beat it"
    );

    kit::click_by_id(&shell, "MainShell::top-bar").unwrap();
    shell.window().dispatch_event(slint::platform::WindowEvent::KeyPressed {
        text: slint::platform::Key::DownArrow.into(),
    });
    assert_eq!(shell.get_search_suggestion_index(), 0);
    shell.window().dispatch_event(slint::platform::WindowEvent::KeyPressed {
        text: slint::platform::Key::DownArrow.into(),
    });
    assert_eq!(shell.get_search_suggestion_index(), 1);
    shell.window().dispatch_event(slint::platform::WindowEvent::KeyPressed {
        text: slint::platform::Key::UpArrow.into(),
    });
    assert_eq!(shell.get_search_suggestion_index(), 0);
    shell.window().dispatch_event(slint::platform::WindowEvent::KeyPressed {
        text: slint::platform::Key::Return.into(),
    });
    assert_eq!(accepted.borrow().last().unwrap().as_str(), "beat it");
    assert_eq!(shell.get_search_query().as_str(), "beat it");
    assert_eq!(shell.get_search_submit_count(), 1);
    assert_eq!(shell.get_search_last_submitted().as_str(), "beat it");
    click_nth(&shell, "SearchPage::sug-row", 1);
    assert_eq!(accepted.borrow().last().unwrap().as_str(), "beatles");
    assert_eq!(shell.get_search_query().as_str(), "beatles");
    assert_eq!(shell.get_search_suggestion_index(), 1);
    assert_eq!(shell.get_search_submit_count(), 2);
    assert_eq!(shell.get_search_last_submitted().as_str(), "beatles");

    click_nth(&shell, "SearchPage::filter-chip", 2);
    assert_eq!(shell.get_search_filter_tab(), 2);
    assert_eq!(shell.get_search_results().row_count(), 19);
    assert_eq!(
        shell
            .get_search_results()
            .row_data(0)
            .unwrap()
            .title
            .as_str(),
        "The Beatles"
    );
    click_nth(&shell, "SearchPage::filter-chip", 3);
    assert_eq!(shell.get_search_results().row_count(), 20);
    assert_eq!(
        shell
            .get_search_results()
            .row_data(0)
            .unwrap()
            .title
            .as_str(),
        "Rubber Soul (Remastered 2009)"
    );
    click_nth(&shell, "SearchPage::filter-chip", 4);
    assert_eq!(shell.get_search_results().row_count(), 13);
    assert_eq!(
        shell
            .get_search_results()
            .row_data(0)
            .unwrap()
            .title
            .as_str(),
        "Coffee Shop Blend"
    );
    click_nth(&shell, "SearchPage::filter-chip", 1);
    assert_eq!(shell.get_search_results().row_count(), 20);

    click_nth(&shell, "M3SearchBar::search-clear", 0);
    assert_eq!(shell.get_search_query().as_str(), "");
    assert_eq!(queries.borrow().last().map(String::as_str), Some(""));

    let mixed = mixed_results(
        &madrigal_innertube::CatalogClient::parse_search_songs(&songs_src).unwrap(),
        &madrigal_innertube::CatalogClient::parse_search_artists(&artists_src).unwrap(),
        &madrigal_innertube::CatalogClient::parse_search_albums(&albums_src).unwrap(),
    );
    assert_eq!(mixed.len(), MIXED_PER_KIND * 3);
    assert_eq!(mixed[0].kind, DetailKind::Artist);
    assert_eq!(mixed[MIXED_PER_KIND].kind, DetailKind::Album);

    let source_signals: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    {
        let source_signals = source_signals.clone();
        shell.on_source_selected(move |_| {
            *source_signals.borrow_mut() += 1;
        });
    }
    let segments: Vec<_> =
        kit::ElementHandle::find_by_element_id(&shell, "M3SegmentedButton::segment").collect();
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].accessible_label().unwrap().as_str(), "Local");
    assert_eq!(segments[1].accessible_label().unwrap().as_str(), "En línea");
    kit::click(&segments[0]);
    assert_eq!(shell.get_search_source(), 0);
    assert_eq!(*source_signals.borrow(), 1);
    kit::click(&segments[0]);
    assert_eq!(shell.get_search_source(), 0);
    assert_eq!(*source_signals.borrow(), 1);
    kit::click(&segments[1]);
    assert_eq!(shell.get_search_source(), 1);
    assert_eq!(*source_signals.borrow(), 2);
    kit::lint_layout_at_sizes(&shell, "search");
    for (width, height) in kit::WINDOW_SIZES {
        kit::resize(&shell, width, height);
        kit::mock_advance(Duration::from_millis(50));
        let clearance = kit::bottom_text_clearance(&shell, "SearchPage::results-scroll");
        assert!(
            clearance >= 90.0,
            "search bottom clearance {clearance} at {width}x{height}"
        );
    }
    kit::resize(&shell, 1280.0, 800.0);

    shell.set_search_query("zzz-no-match".into());
    madrigal_ui::apply_search_results(&shell, Vec::new());
    shell.invoke_query_changed("zzz-no-match".into());
    assert_eq!(shell.get_search_suggestions().row_count(), 0);
    assert_eq!(shell.get_search_results().row_count(), 0);
    assert!(!shell.get_search_query().is_empty());

    shell.set_search_query("the beatles".into());
    shell.set_search_source(1);
    shell.set_search_filter_tab(0);
    madrigal_ui::apply_search_results(&shell, Vec::new());
    kit::mock_advance(Duration::from_millis(50));
    let empty = kit::find_by_id(&shell, "SearchPage::search-empty").unwrap();
    assert_eq!(
        empty.accessible_label().unwrap().as_str(),
        "No hay resultados para «the beatles». Probá otras palabras."
    );
    shell.set_search_filter_tab(4);
    kit::mock_advance(Duration::from_millis(50));
    let empty = kit::find_by_id(&shell, "SearchPage::search-empty").unwrap();
    assert_eq!(
        empty.accessible_label().unwrap().as_str(),
        "No hay listas para «the beatles». Probá Todo."
    );
    shell.set_search_source(0);
    shell.set_search_filter_tab(0);
    kit::mock_advance(Duration::from_millis(50));
    let empty = kit::find_by_id(&shell, "SearchPage::search-empty").unwrap();
    assert_eq!(
        empty.accessible_label().unwrap().as_str(),
        "No tenés canciones descargadas que coincidan. Probá En línea."
    );
    let retries: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    {
        let retries = retries.clone();
        shell.on_search_retry(move || {
            *retries.borrow_mut() += 1;
        });
    }
    shell.set_search_source(1);
    shell.set_search_error(true);
    kit::mock_advance(Duration::from_millis(50));
    let error = kit::find_by_id(&shell, "SearchPage::search-error-text").unwrap();
    assert_eq!(
        error.accessible_label().unwrap().as_str(),
        "No pudimos buscar. Revisá tu conexión y reintentá."
    );
    assert!(kit::find_by_id(&shell, "SearchPage::search-empty").is_err());
    kit::click_by_id(&shell, "SearchPage::search-retry").unwrap();
    assert_eq!(*retries.borrow(), 1);
    shell.set_search_error(false);
    shell.set_search_filter_tab(1);

    shell.set_search_query("".into());
    madrigal_ui::apply_suggestions(&shell, "", &[]);
    let recent = [
        slint::SharedString::from("abbey road"),
        slint::SharedString::from("beatles"),
    ];
    shell.set_search_recent(Rc::new(VecModel::from_slice(&recent)).into());
    assert_eq!(shell.get_search_recent().row_count(), 2);
    kit::lint_layout_at_sizes(&shell, "search-recent");
    click_nth(&shell, "M3ListItem::action-button", 0);
    assert_eq!(shell.get_search_remove_count(), 1);
    assert_eq!(shell.get_search_last_removed().as_str(), "abbey road");
    click_nth(&shell, "SearchPage::recent-clear", 0);
    assert_eq!(shell.get_search_clear_count(), 1);
    click_nth(&shell, "SearchPage::recent-row", 1);
    assert_eq!(shell.get_search_query().as_str(), "beatles");
    assert_eq!(shell.get_search_submit_count(), 3);
    assert_eq!(shell.get_search_last_submitted().as_str(), "beatles");
    shell.set_search_recent(
        Rc::new(VecModel::from_slice(&[] as &[slint::SharedString])).into(),
    );
    assert_eq!(shell.get_search_recent().row_count(), 0);

    assert!(!shell.get_search_focused());
    let focus_signals: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    {
        let focus_signals = focus_signals.clone();
        shell.on_search_focus_requested(move || {
            *focus_signals.borrow_mut() += 1;
        });
    }
    assert!(dispatch_key(&shell, "/"));
    assert!(shell.get_search_focused());
    assert_eq!(shell.get_search_focus_count(), 1);
    assert_eq!(*focus_signals.borrow(), 1);

    shell.set_page(2);
    load_explore_fixtures(&shell, &moods_src, &explore_src, &charts_src).unwrap();
    assert_eq!(
        shell.get_explore_moods_left().row_count()
            + shell.get_explore_moods_right().row_count(),
        46
    );
    assert_eq!(shell.get_explore_moods_left().row_count(), 23);
    assert_eq!(shell.get_explore_releases_albums().row_count(), 24);
    assert_eq!(shell.get_explore_releases_videos().row_count(), 24);
    assert_eq!(shell.get_explore_browse_sections().row_count(), 3);
    assert_eq!(
        shell
            .get_explore_browse_sections()
            .row_data(0)
            .unwrap()
            .title
            .as_str(),
        "New albums & singles"
    );
    assert_eq!(shell.get_explore_charts_id().as_str(), "charts:");

    let listing = madrigal_innertube::CatalogClient::parse_mood_listing(&moods_src).unwrap();
    let mut few = listing.clone();
    few.sections.truncate(1);
    few.sections[0].entries.truncate(4);
    madrigal_ui::apply_moods(&shell, &few);
    assert_eq!(shell.get_explore_moods_left().row_count(), 2);
    assert_eq!(shell.get_explore_moods_right().row_count(), 2);

    click_nth(&shell, "ExplorePage::mood-chip-l", 0);
    assert_eq!(opened.borrow().last().unwrap().0, DetailKind::Browse);
    assert_eq!(
        opened.borrow().last().unwrap().1.as_str(),
        "ggMPOg1uX1JOQWZFeDByc2Jm"
    );
    assert!(is_detail_open(&shell));
    assert_eq!(shell.get_detail_sections().row_count(), 3);
    assert_eq!(shell.get_detail_title().as_str(), "Browse");
    click_nth(&shell, "DetailPage::detail-back", 0);
    assert!(!is_detail_open(&shell));

    click_nth(&shell, "ExplorePage::charts-card", 0);
    assert_eq!(opened.borrow().last().unwrap().0, DetailKind::Browse);
    assert_eq!(opened.borrow().last().unwrap().1.as_str(), "charts:");
    assert!(is_detail_open(&shell));
    click_nth(&shell, "DetailPage::detail-back", 0);
    assert!(!is_detail_open(&shell));

    assert_eq!(shell.get_explore_releases_tab(), 0);
    click_nth(&shell, "ExplorePage::rel-chip", 1);
    assert_eq!(shell.get_explore_releases_tab(), 1);
    click_nth(&shell, "ExplorePage::rel-video-card", 0);
    let first_video = shell
        .get_explore_releases_videos()
        .row_data(0)
        .unwrap();
    assert_eq!(played.borrow().last().unwrap().as_str(), first_video.id.as_str());
    click_nth(&shell, "ExplorePage::rel-chip", 0);
    assert_eq!(shell.get_explore_releases_tab(), 0);
    click_nth(&shell, "ExplorePage::rel-album-card", 0);
    assert_eq!(opened.borrow().last().unwrap().0, DetailKind::Album);
    assert!(is_detail_open(&shell));
    assert_eq!(
        shell.get_detail_title().as_str(),
        "Rubber Soul (Remastered 2009)"
    );
    assert_eq!(shell.get_detail_tracks().row_count(), 14);
    let first_track = shell.get_detail_tracks().row_data(0).unwrap();
    assert_eq!(first_track.title.as_str(), "Drive My Car (Remastered 2009)");
    assert_eq!(first_track.duration.as_str(), "2:29");
    click_nth(&shell, "DetailPage::detail-back", 0);

    shell.set_page(1);
    click_nth(&shell, "SearchPage::filter-chip", 2);
    assert_eq!(
        shell
            .get_search_results()
            .row_data(0)
            .unwrap()
            .subtitle
            .as_str(),
        "55,5 M oyentes mensuales"
    );
    assert_eq!(
        shell
            .get_search_results()
            .row_data(6)
            .unwrap()
            .title
            .as_str(),
        "Re Beatles"
    );
    click_nth(&shell, "SearchPage::hit-row", 0);
    assert_eq!(opened.borrow().last().unwrap().0, DetailKind::Artist);
    assert!(is_detail_open(&shell));
    assert_eq!(shell.get_detail_title().as_str(), "The Beatles");
    assert_eq!(
        subscriber_label("5.48M subscribers", madrigal_i18n::Lang::Es).as_deref(),
        Some("5,48 M suscriptores")
    );
    assert_eq!(
        subscriber_label("5.48M subscribers", madrigal_i18n::Lang::En).as_deref(),
        Some("5.48M subscribers")
    );
    assert_eq!(
        subscriber_label("55.5M monthly audience", madrigal_i18n::Lang::Es).as_deref(),
        Some("55,5 M oyentes mensuales")
    );
    assert_eq!(
        subscriber_label("55.5M monthly audience", madrigal_i18n::Lang::En).as_deref(),
        Some("55.5M monthly audience")
    );
    assert_eq!(
        subscriber_label("2.28K subscribers", madrigal_i18n::Lang::Es).as_deref(),
        Some("2,28 K suscriptores")
    );
    let artists =
        madrigal_innertube::CatalogClient::parse_search_artists(&artists_src).unwrap();
    let artist_items = madrigal_ui::artist_results(&artists, madrigal_i18n::Lang::Es);
    assert_eq!(artist_items[0].title.as_str(), "The Beatles");
    assert_eq!(artist_items[0].subtitle.as_str(), "55,5 M oyentes mensuales");
    assert_eq!(artist_items[4].title.as_str(), "Not The Beatles");
    assert_eq!(artist_items[4].subtitle.as_str(), "");
    assert_eq!(artist_items[6].title.as_str(), "Re Beatles");
    assert_eq!(artist_items[6].subtitle.as_str(), "2,28 K suscriptores");
    assert_eq!(artist_items[18].subtitle.as_str(), "115 suscriptores");
    let first_channel = artist_items
        .iter()
        .position(|item| item.subtitle.ends_with("suscriptores"))
        .unwrap();
    assert_eq!(first_channel, 6);
    let artist_items_en = madrigal_ui::artist_results(&artists, madrigal_i18n::Lang::En);
    assert_eq!(
        artist_items_en[0].subtitle.as_str(),
        "55.5M monthly audience"
    );
    assert_eq!(artist_items_en[6].subtitle.as_str(), "2.28K subscribers");
    assert_eq!(
        shell.get_detail_subtitle().as_str(),
        "9,8 M suscriptores"
    );
    assert!(shell.get_detail_desc_long());
    let toggle = kit::find_by_id(&shell, "DetailPage::desc-toggle").unwrap();
    assert_eq!(toggle.accessible_label().unwrap().as_str(), "Más");
    kit::click(&toggle);
    kit::mock_advance(Duration::from_millis(50));
    assert_eq!(toggle.accessible_label().unwrap().as_str(), "Menos");
    kit::click(&toggle);
    kit::mock_advance(Duration::from_millis(50));
    assert_eq!(toggle.accessible_label().unwrap().as_str(), "Más");
    assert_eq!(shell.get_detail_tracks().row_count(), 5);
    assert_eq!(shell.get_detail_albums().row_count(), 10);
    assert_eq!(
        shell
            .get_detail_tracks()
            .row_data(0)
            .unwrap()
            .title
            .as_str(),
        "Let It Be (Remastered 2009)"
    );
    assert_eq!(
        shell
            .get_detail_albums()
            .row_data(0)
            .unwrap()
            .title
            .as_str(),
        "Rubber Soul (Super Deluxe)"
    );
    assert_eq!(shell.get_detail_tab(), 0);
    click_nth(&shell, "DetailPage::artist-chip", 1);
    assert_eq!(shell.get_detail_tab(), 1);
    click_nth(&shell, "DetailPage::artist-chip", 0);
    assert_eq!(shell.get_detail_tab(), 0);
    click_nth(&shell, "DetailPage::detail-back", 0);

    click_nth(&shell, "SearchPage::filter-chip", 4);
    click_nth(&shell, "SearchPage::hit-row", 0);
    assert_eq!(opened.borrow().last().unwrap().0, DetailKind::Playlist);
    assert!(is_detail_open(&shell));
    assert_eq!(
        shell.get_detail_title().as_str(),
        "The Beatles Greatest Hits"
    );
    assert_eq!(shell.get_detail_tracks().row_count(), 78);
    assert_eq!(
        shell
            .get_detail_tracks()
            .row_data(0)
            .unwrap()
            .title
            .as_str(),
        "Love Me Do (Mono Version / Remastered 2009)"
    );
    assert!(dispatch_key(&shell, "Escape"));
    assert!(!is_detail_open(&shell));
    assert_eq!(*closed.borrow(), 5);
    assert!(dispatch_key(&shell, "Escape"));
    assert!(shell.get_sidebar_focused());

    apply_home(&shell, &feed);
    apply_browse_detail(&shell, &madrigal_innertube::CatalogClient::parse_browse(&explore_src).unwrap(), "Browse");
    assert_eq!(shell.get_detail_sections().row_count(), 3);

    close_detail(&shell);
    assert!(!is_detail_open(&shell));

    load_album_fixture(&shell, ALBUM_ID, &album_src).unwrap();
    shell.set_detail_stack(1);
    assert!(is_detail_open(&shell));
    click_nth(&shell, "SideNav::nav-explore", 0);
    assert!(!is_detail_open(&shell), "side menu closes the detail page");
    assert_eq!(shell.get_page(), 2);

    begin_detail_load(
        &shell,
        &DetailRef {
            kind: DetailKind::Artist,
            id: ARTIST_ID.into(),
            title: "The Beatles".into(),
        },
    );
    assert!(shell.get_detail_loading());
    assert_eq!(shell.get_detail_title().as_str(), "The Beatles");
    assert_eq!(shell.get_detail_tracks().row_count(), 0);
    shell.set_detail_stack(1);
    assert!(kit::find_by_id(&shell, "DetailPage::detail-loading").is_ok());
    let retried = Rc::new(RefCell::new(0));
    {
        let retried = retried.clone();
        shell.on_detail_retried(move || {
            *retried.borrow_mut() += 1;
        });
    }
    apply_detail_error(&shell, "boom");
    assert!(!shell.get_detail_loading());
    assert_eq!(shell.get_detail_error().as_str(), "boom");
    assert!(kit::find_by_id(&shell, "DetailPage::detail-retry").is_ok());
    kit::click_by_id(&shell, "DetailPage::detail-retry").unwrap();
    assert_eq!(*retried.borrow(), 1);
    close_detail(&shell);
    assert!(!is_detail_open(&shell));

    kit::mock_advance(Duration::from_millis(600));
    shell.show().unwrap();
    shell.set_page(0);
    kit::lint_layout_at_sizes(&shell, "home");
    for (width, height) in kit::WINDOW_SIZES {
        kit::resize(&shell, width, height);
        kit::mock_advance(Duration::from_millis(50));
        let clearance = kit::bottom_text_clearance(&shell, "HomePage::page-flick");
        assert!(
            clearance >= 90.0,
            "home bottom clearance {clearance} at {width}x{height}"
        );
    }
    kit::resize(&shell, 1280.0, 800.0);
    snapshot(&shell, &snapshot_dir, "home.png");    shell.set_page(1);
    shell.set_search_query("zzz-no-match".into());
    madrigal_ui::apply_search_results(&shell, Vec::new());
    shell.invoke_query_changed("zzz-no-match".into());
    assert_eq!(shell.get_search_results().row_count(), 0);
    snapshot(&shell, &snapshot_dir, "search.png");
    shell.set_page(2);
    madrigal_ui::apply_moods(&shell, &listing);
    assert!(dispatch_key(&shell, "/"));
    let mut mood_focused = false;
    for _ in 0..30 {
        shell.window().dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Tab.into(),
        });
        if kit::find_by_id(&shell, "M3MoodTile::focus-ring").is_ok() {
            mood_focused = true;
            break;
        }
    }
    assert!(mood_focused, "mood tile never received keyboard focus");
    shell.window().dispatch_event(slint::platform::WindowEvent::KeyPressed {
        text: slint::platform::Key::Return.into(),
    });
    assert_eq!(
        opened.borrow().last().unwrap().0,
        DetailKind::Browse,
        "mood tile is not reachable with Tab"
    );
    click_nth(&shell, "DetailPage::detail-back", 0);
    assert!(!is_detail_open(&shell));
    kit::lint_layout_at_sizes(&shell, "explore");
    for (width, height) in kit::WINDOW_SIZES {
        kit::resize(&shell, width, height);
        kit::mock_advance(Duration::from_millis(50));
        let clearance = kit::bottom_text_clearance(&shell, "ExplorePage::page-flick");
        assert!(
            clearance >= 90.0,
            "explore bottom clearance {clearance} at {width}x{height}"
        );
    }
    kit::resize(&shell, 1280.0, 800.0);
    snapshot(&shell, &snapshot_dir, "explore.png");
    load_album_fixture(&shell, ALBUM_ID, &album_src).unwrap();
    shell.set_detail_stack(1);
    for (width, height) in kit::WINDOW_SIZES {
        kit::resize(&shell, width, height);
        let page = kit::find_by_id(&shell, "PageHost::detail-view").unwrap();
        let violations = kit::lint_layout_of(&page, (width, height));
        assert!(
            violations.is_empty(),
            "detail at {width}x{height}: {violations:#?}"
        );
    }
    kit::resize(&shell, 1280.0, 800.0);
    snapshot(&shell, &snapshot_dir, "detail.png");
}
