use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use madrigal_testkit as kit;
use madrigal_ui::{
    album_cards, artist_cards, chip_order, default_chip_order, download_models, event_years,
    history_rows, load_downloads_fixture, load_library_fixture, move_chip, playlist_rows,
    save_chip_order, song_rows, song_sort_for_settings, stats_streaks, stats_totals, top_album_rows,
    top_artist_rows, top_track_rows, year_index, year_model, MainShell,
};
use slint::{ComponentHandle, Model, VecModel};

static PROFILE_LOCK: Mutex<()> = Mutex::new(());

fn lock_profile() -> MutexGuard<'static, ()> {
    PROFILE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn ui_library() {
    let _guard = lock_profile();
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", tmp.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", tmp.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", tmp.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", tmp.path().join("state"));

    let fx = load_library_fixture();
    let lib = fx.library();
    assert_eq!(default_chip_order().len(), 7);

    kit::init_headless();
    let shell = MainShell::new().unwrap();
    let lang = madrigal_ui::wire_shell_i18n(&shell, madrigal_i18n::Lang::Es);

    let trl = |code: &str, key: &str| -> String {
        shell
            .global::<madrigal_ui::I18n>()
            .invoke_trl(code.into(), key.into())
            .as_str()
            .to_owned()
    };
    assert_eq!(trl("es", "library.tabs.songs"), "Canciones");
    assert_eq!(trl("en", "library.tabs.songs"), "Songs");
    assert_eq!(trl("es", "library.tabs.albums"), "Álbumes");
    assert_eq!(trl("en", "library.tabs.albums"), "Albums");
    assert_eq!(trl("es", "library.tags.title"), "Etiquetas");
    assert_eq!(trl("en", "library.tags.title"), "Tags");
    assert_eq!(trl("es", "library.sort.title"), "Ordenar");
    assert_eq!(trl("en", "library.sort.title"), "Sort");
    assert_eq!(trl("es", "downloads.failed"), "Fallidas");
    assert_eq!(trl("en", "downloads.failed"), "Failed");
    assert_eq!(trl("es", "library.history.clear"), "Borrar historial");
    assert_eq!(trl("en", "library.history.clear"), "Clear history");
    madrigal_ui::set_shell_lang(&shell, &lang, madrigal_i18n::Lang::En);
    madrigal_ui::set_shell_lang(&shell, &lang, madrigal_i18n::Lang::Es);

    let order = chip_order(&fx.settings.borrow());
    assert_eq!(order, default_chip_order());
    shell.set_lib_chips(
        Rc::new(VecModel::from(
            order.iter().map(slint::SharedString::from).collect::<Vec<_>>(),
        ))
        .into(),
    );
    assert_eq!(shell.get_lib_chips().row_count(), 7);

    let sort_labels: Vec<slint::SharedString> =
        ["Recent", "Name", "Artist", "Plays"].into_iter().map(Into::into).collect();
    shell.set_lib_sort_options(Rc::new(VecModel::from(sort_labels)).into());
    shell.set_lib_sort(0);

    let sync_songs = |sort: madrigal_library::SongSort,
                      dir: madrigal_library::SortDir,
                      selected: &HashSet<String>| {
        let rows = song_rows(&lib, sort, dir, selected);
        shell.set_lib_songs(Rc::new(VecModel::from(rows)).into());
    };
    let empty_selected = HashSet::new();
    let (default_sort, default_dir) = song_sort_for_settings(&fx.settings.borrow());
    sync_songs(default_sort, default_dir, &empty_selected);
    assert_eq!(shell.get_lib_songs().row_count(), 5);

    shell.set_page(3);
    kit::click_by_id(&shell, "LibraryPage::lib-chip-2").unwrap();
    assert_eq!(shell.get_lib_tab(), 2);
    assert_eq!(shell.get_lib_action_count(), 1);
    assert_eq!(shell.get_lib_last_action().as_str(), "chip-selected");
    let songs_model = shell.get_lib_songs();
    assert_eq!(songs_model.row_data(0).unwrap().title.as_str(), "Delta");

    let mut moved = order.clone();
    assert!(move_chip(&mut moved, 2, 0));
    assert!(!move_chip(&mut moved, 0, 99));
    save_chip_order(&mut fx.settings.borrow_mut(), &moved);
    assert_eq!(chip_order(&fx.settings.borrow())[0], "songs");
    let settings_path = tmp.path().join("settings.json");
    fx.settings.borrow().save_to(&settings_path).unwrap();
    let reloaded = madrigal_settings::Settings::load_from(&settings_path);
    assert_eq!(chip_order(&reloaded)[0], "songs");
    shell.set_lib_chips(
        Rc::new(VecModel::from(
            moved.iter().map(slint::SharedString::from).collect::<Vec<_>>(),
        ))
        .into(),
    );
    assert_eq!(shell.get_lib_chips().row_data(0).unwrap().as_str(), "songs");
    shell.invoke_lib_chip_selected(0);
    assert_eq!(shell.get_lib_tab(), 0);

    shell
        .invoke_lib_tag_selected("rock".into());
    assert_eq!(shell.get_lib_active_tag().as_str(), "rock");
    assert_eq!(shell.get_lib_last_action().as_str(), "tag-selected");
    let rock_rows = playlist_rows(&lib, Some("rock"));
    assert_eq!(rock_rows.len(), 1);
    assert_eq!(rock_rows[0].title.as_str(), "Rock night");
    assert!(rock_rows[0].tags.as_str().contains("rock"));
    shell.set_lib_playlists(Rc::new(VecModel::from(rock_rows)).into());
    assert_eq!(shell.get_lib_playlists().row_count(), 1);
    let all_rows = playlist_rows(&lib, None);
    assert_eq!(all_rows.len(), 2);
    shell.set_lib_playlists(Rc::new(VecModel::from(all_rows)).into());
    assert_eq!(shell.get_lib_playlists().row_count(), 2);

    let delta_id = "ddd111".to_string();
    let songs = song_rows(&lib, madrigal_library::SongSort::DateAdded, madrigal_library::SortDir::Asc, &empty_selected);
    let delta = songs.iter().find(|row| row.id.as_str() == delta_id).unwrap();
    assert!(delta.liked);
    lib.set_liked(&madrigal_core::TrackIdentity::youtube("ddd111"), false)
        .unwrap();
    let songs = song_rows(&lib, madrigal_library::SongSort::DateAdded, madrigal_library::SortDir::Asc, &empty_selected);
    assert!(!songs.iter().find(|row| row.id.as_str() == delta_id).unwrap().liked);
    lib.set_liked(&madrigal_core::TrackIdentity::youtube("ddd111"), true)
        .unwrap();
    shell.invoke_lib_song_toggled(delta_id.into());
    assert_eq!(shell.get_lib_last_action().as_str(), "song-toggled");

    shell.invoke_lib_multiselect_toggled();
    assert!(shell.get_lib_multiselect());
    assert_eq!(shell.get_lib_last_action().as_str(), "multiselect-toggled");
    let selected: HashSet<String> = ["aaa111".to_string(), "bbb111".to_string()].into_iter().collect();
    let songs = song_rows(&lib, madrigal_library::SongSort::DateAdded, madrigal_library::SortDir::Asc, &selected);
    shell.set_lib_songs(Rc::new(VecModel::from(songs)).into());
    shell.set_lib_selected_count(selected.len() as i32);
    assert_eq!(shell.get_lib_selected_count(), 2);
    assert!(shell.get_lib_songs().row_data(1).unwrap().selected);
    shell.invoke_lib_selection_cleared();
    assert_eq!(shell.get_lib_last_action().as_str(), "selection-cleared");
    shell.invoke_lib_multiselect_toggled();
    assert!(!shell.get_lib_multiselect());

    kit::click_by_id(&shell, "LibraryPage::sort-touch").unwrap();
    assert!(shell.get_lib_sort_open());
    shell.invoke_lib_sort_selected(1);
    assert_eq!(shell.get_lib_sort(), 1);
    assert!(!shell.get_lib_sort_open());
    assert_eq!(shell.get_lib_last_action().as_str(), "sort-selected");
    sync_songs(madrigal_library::SongSort::Name, madrigal_library::SortDir::Asc, &empty_selected);
    let titles: Vec<String> = (0..shell.get_lib_songs().row_count())
        .map(|i| shell.get_lib_songs().row_data(i).unwrap().title.as_str().to_owned())
        .collect();
    assert_eq!(titles, ["Alpha", "Bravo", "Charlie", "Delta", "Echo"]);
    sync_songs(madrigal_library::SongSort::PlayTime, madrigal_library::SortDir::Asc, &empty_selected);
    let titles: Vec<String> = (0..shell.get_lib_songs().row_count())
        .map(|i| shell.get_lib_songs().row_data(i).unwrap().title.as_str().to_owned())
        .collect();
    assert_eq!(titles, ["Bravo", "Alpha", "Charlie", "Echo", "Delta"]);

    let cards = artist_cards(&lib);
    assert_eq!(cards.len(), 3);
    shell.set_lib_cards(Rc::new(VecModel::from(cards)).into());
    assert_eq!(shell.get_lib_cards().row_count(), 3);
    let albums = album_cards(&lib);
    assert_eq!(albums.len(), 2);
    shell.invoke_lib_playlist_opened("Rock night".into());
    assert_eq!(shell.get_lib_last_action().as_str(), "playlist-opened");

    assert_eq!(stats_totals(&lib), "12 plays · 40m 00s listened");
    assert_eq!(stats_streaks(&lib), "streak 3d · best 3d");
    shell.set_lib_stats_totals(stats_totals(&lib).into());
    shell.set_lib_stats_streaks(stats_streaks(&lib).into());
    let tops = top_track_rows(&lib, madrigal_settings::TopRange::AllTime);
    assert_eq!(tops[0].title.as_str(), "Delta");
    assert_eq!(tops[0].value.as_str(), "5");
    shell.set_lib_stat_tracks(Rc::new(VecModel::from(tops)).into());
    let artists = top_artist_rows(&lib, madrigal_settings::TopRange::AllTime);
    assert_eq!(artists[0].title.as_str(), "Alice");
    assert_eq!(artists[0].value.as_str(), "8");
    shell.set_lib_stat_artists(Rc::new(VecModel::from(artists)).into());
    let albums = top_album_rows(&lib, madrigal_settings::TopRange::AllTime);
    assert_eq!(albums[0].title.as_str(), "Hits");
    assert_eq!(albums[0].value.as_str(), "8");
    shell.set_lib_stat_albums(Rc::new(VecModel::from(albums)).into());

    let years = event_years(&lib);
    assert!(!years.is_empty());
    let current: i32 = years[0].parse().unwrap();
    shell.set_lib_years(Rc::new(VecModel::from(
        years.iter().map(|year| year.clone().into()).collect::<Vec<slint::SharedString>>(),
    )).into());
    shell.set_lib_year(year_index(&years, current) as i32);
    let (tracks, yartists, yalbums) = year_model(&lib, current);
    assert_eq!(tracks[0].title.as_str(), "Delta");
    assert_eq!(tracks[0].value.as_str(), "5");
    shell.set_lib_stat_tracks(Rc::new(VecModel::from(tracks)).into());
    shell.set_lib_stat_artists(Rc::new(VecModel::from(yartists)).into());
    shell.set_lib_stat_albums(Rc::new(VecModel::from(yalbums)).into());
    let (empty_tracks, _, _) = year_model(&lib, current - 1);
    assert!(empty_tracks.is_empty());
    let before = shell.get_lib_year();
    shell.invoke_lib_year_prev();
    shell.invoke_lib_year_next();
    assert_eq!(shell.get_lib_year(), before);
    assert_eq!(shell.get_lib_last_action().as_str(), "year-next");

    let history = history_rows(&lib);
    assert_eq!(history.len(), 12);
    assert_eq!(history[0].title.as_str(), "Charlie");
    shell.set_lib_history(Rc::new(VecModel::from(history)).into());
    shell.set_lib_history_paused(fx.settings.borrow().privacy.pause_listen_history);
    assert!(!shell.get_lib_history_paused());
    assert_eq!(shell.get_lib_history().row_count(), 12);
    shell.invoke_lib_chip_selected(5);
    kit::click_by_id(&shell, "LibraryPage::history-clear-touch").unwrap();
    assert_eq!(shell.get_lib_last_action().as_str(), "history-cleared");
    lib.clear_listen_history().unwrap();
    let history = history_rows(&lib);
    assert!(history.is_empty());
    shell.set_lib_history(Rc::new(VecModel::from(history)).into());
    assert_eq!(shell.get_lib_history().row_count(), 0);
    fx.settings.borrow_mut().privacy.pause_listen_history = true;
    shell.set_lib_history_paused(fx.settings.borrow().privacy.pause_listen_history);
    assert!(shell.get_lib_history_paused());
    fx.settings.borrow_mut().privacy.pause_listen_history = false;

    shell.invoke_lib_tags_opened();
    assert!(shell.get_lib_tags_open());
    shell.set_lib_tag_input("focus".into());
    shell.invoke_lib_tag_created("focus".into());
    assert_eq!(shell.get_lib_last_action().as_str(), "tag-created");
    let rock_id = lib
        .list_playlists(&madrigal_library::PlaylistListFilter {
            tag: Some(String::new()),
            include_hidden: false,
            kind: None,
        })
        .unwrap()
        .into_iter()
        .find(|playlist| playlist.title == "Rock night")
        .unwrap()
        .id;
    lib.assign_tag(&rock_id, "focus").unwrap();
    let tags = lib.all_tags().unwrap();
    assert!(tags.contains(&"focus".to_string()));
    shell.set_lib_tags(Rc::new(VecModel::from(
        tags.iter().map(|tag| tag.clone().into()).collect::<Vec<slint::SharedString>>(),
    )).into());
    assert!(shell.get_lib_tags().row_count() >= 3);
    lib.remove_tag(&rock_id, "focus").unwrap();
    assert!(!lib.playlist_tags(&rock_id).unwrap().contains(&"focus".to_string()));
    shell.invoke_lib_tag_removed("focus".into());
    assert_eq!(shell.get_lib_last_action().as_str(), "tag-removed");
    shell.invoke_lib_tags_closed();
    assert!(!shell.get_lib_tags_open());
    shell.invoke_lib_chip_selected(2);
    kit::click_by_id(&shell, "LibraryPage::manage-touch").unwrap();
    assert!(shell.get_lib_tags_open());
    shell.invoke_lib_tags_closed();

    let items = load_downloads_fixture();
    let (active, failed) = download_models(&items);
    assert_eq!(active.len(), 4);
    assert_eq!(failed.len(), 2);
    assert_eq!(active[1].progress, 0.5);
    assert_eq!(active[1].speed.as_str(), "256 KB/s");
    assert_eq!(active[0].state, 0);
    assert_eq!(failed[0].state, 4);
    shell.set_dl_rows(Rc::new(VecModel::from(active)).into());
    shell.set_dl_failed(Rc::new(VecModel::from(failed)).into());
    assert_eq!(shell.get_dl_rows().row_count(), 4);
    assert_eq!(shell.get_dl_failed().row_count(), 2);

    shell.invoke_dl_pause("dl0".into());
    assert_eq!(shell.get_dl_last_action().as_str(), "dl-pause");
    shell.invoke_dl_resume("dl2".into());
    assert_eq!(shell.get_dl_last_action().as_str(), "dl-resume");
    shell.invoke_dl_cancel("dl0".into());
    assert_eq!(shell.get_dl_last_action().as_str(), "dl-cancel");
    shell.invoke_dl_retry("dl4".into());
    assert_eq!(shell.get_dl_last_action().as_str(), "dl-retry");
    assert_eq!(shell.get_dl_action_count(), 4);
    shell.set_page(4);
    kit::click_by_id(&shell, "DownloadsPage::retry-all-touch").unwrap();
    assert_eq!(shell.get_dl_last_action().as_str(), "dl-retry-all");

    kit::mock_advance(Duration::from_millis(600));

    shell.set_page(3);
    shell.show().unwrap();
    let violations = kit::lint_layout(&shell, (1920.0, 1080.0));
    assert!(violations.is_empty(), "layout violations: {violations:#?}");
    let snapshot_dir = kit::snapshot_dir(tmp.path());
    let library_shot = snapshot_dir.join("library.png");
    kit::snapshot_to(shell.window(), &library_shot).unwrap();
    let bytes = std::fs::read(&library_shot).unwrap();
    assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    assert!(bytes.len() > 4096, "snapshot too small: {}", bytes.len());

    shell.set_page(4);
    kit::mock_advance(Duration::from_millis(600));
    let violations = kit::lint_layout(&shell, (1920.0, 1080.0));
    assert!(violations.is_empty(), "layout violations: {violations:#?}");
    let downloads_shot = snapshot_dir.join("downloads.png");
    kit::snapshot_to(shell.window(), &downloads_shot).unwrap();
    let bytes = std::fs::read(&downloads_shot).unwrap();
    assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    assert!(bytes.len() > 4096, "snapshot too small: {}", bytes.len());
}
