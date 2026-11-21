use madrigal_testkit as kit;
use madrigal_ui::{
    dispatch_key, wire_shell_i18n, CatalogItem, DetailKind, LibSongRow, MainShell, QueueRow,
    TrackMenuEntry, TrackMenuRequest,
};
use slint::{ComponentHandle, ModelRc, VecModel};

fn request(anchor_x: f32, anchor_y: f32) -> TrackMenuRequest {
    TrackMenuRequest {
        id: "beat1".into(),
        title: "Come Together".into(),
        artist: "The Beatles".into(),
        album: "Abbey Road".into(),
        duration_secs: 259,
        anchor_x,
        anchor_y,
    }
}

fn search_item() -> CatalogItem {
    CatalogItem {
        kind: DetailKind::Album,
        id: "beat1".into(),
        title: "Come Together".into(),
        subtitle: "The Beatles · Abbey Road".into(),
        duration: "4:19".into(),
        playable: true,
        artist: "The Beatles".into(),
        album: "Abbey Road".into(),
        thumbnail: "".into(),
        duration_secs: 259,
    }
}

#[test]
fn ui_actions() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", tmp.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", tmp.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", tmp.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", tmp.path().join("state"));

    kit::init_headless();
    let shell = MainShell::new().unwrap();
    let _lang = wire_shell_i18n(&shell, madrigal_i18n::Lang::Es);

    let trl = |key: &str| -> String {
        shell
            .global::<madrigal_ui::I18n>()
            .invoke_trl("es".into(), key.into())
            .as_str()
            .to_owned()
    };
    assert_eq!(trl("action.playNext"), "Reproducir a continuación");
    assert_eq!(trl("action.unlike"), "Quitar me gusta");
    assert_eq!(trl("downloads.downloaded"), "Descargada");
    assert_eq!(trl("action.goToArtist"), "Ir al artista");
    assert_eq!(trl("trackMenu.title"), "Acciones de la canción");
    let en: slint::SharedString = shell
        .global::<madrigal_ui::I18n>()
        .invoke_trl("en".into(), "downloads.emptyHint".into());
    assert!(en.contains("song menu and choose Download"));

    shell.set_page(1);
    shell.set_search_results(ModelRc::new(VecModel::from_slice(&[search_item()])));
    shell.show().unwrap();
    kit::mock_advance(std::time::Duration::from_millis(200));
    let snapshot_dir = kit::snapshot_dir(tmp.path());

    kit::click_by_id(&shell, "M3ListItem::menu-button").unwrap();
    assert!(shell.get_track_menu_open());
    assert_eq!(shell.get_track_menu_track().id.as_str(), "beat1");
    assert_eq!(shell.get_track_menu_track().artist.as_str(), "The Beatles");
    assert_eq!(shell.get_track_menu_index(), 0);

    assert!(dispatch_key(&shell, "Down"));
    assert_eq!(shell.get_track_menu_index(), 1);
    assert!(dispatch_key(&shell, "Escape"));
    assert!(!shell.get_track_menu_open());
    assert_eq!(shell.get_track_menu_action_count(), 0);

    let row = kit::find_by_id(&shell, "M3ListItem::item-touch").unwrap();
    row.mock_single_click(slint::platform::PointerEventButton::Right);
    assert!(shell.get_track_menu_open());
    kit::click_by_id(&shell, "TrackMenu::menu-scrim").unwrap();
    assert!(!shell.get_track_menu_open());

    for (element, action) in [
        ("menu-next", "play-next"),
        ("menu-queue", "queue"),
        ("menu-like", "like"),
        ("menu-download", "download"),
        ("menu-artist", "artist"),
        ("menu-album", "album"),
    ] {
        shell.invoke_track_menu_requested(request(300.0, 200.0));
        assert!(shell.get_track_menu_open());
        kit::click_by_id(&shell, &format!("TrackMenu::{element}")).unwrap();
        assert_eq!(
            shell.get_track_menu_last_action().as_str(),
            action,
            "{element}"
        );
        assert!(!shell.get_track_menu_open());
    }
    assert_eq!(shell.get_track_menu_action_count(), 6);

    shell.set_track_menu_playlists(ModelRc::new(VecModel::from_slice(&[
        TrackMenuEntry {
            id: "local:1".into(),
            title: "Prueba".into(),
        },
        TrackMenuEntry {
            id: "local:2".into(),
            title: "Viaje".into(),
        },
    ])));
    shell.invoke_track_menu_requested(request(300.0, 200.0));
    kit::click_by_id(&shell, "TrackMenu::menu-playlist").unwrap();
    assert_eq!(shell.get_track_menu_view(), 1);
    assert!(shell.get_track_menu_open());
    assert!(dispatch_key(&shell, "\n"));
    assert_eq!(shell.get_track_menu_last_action().as_str(), "playlist");
    assert_eq!(shell.get_track_menu_last_arg().as_str(), "local:1");

    shell.invoke_track_menu_requested(request(300.0, 200.0));
    kit::click_by_id(&shell, "TrackMenu::menu-playlist").unwrap();
    assert_eq!(shell.get_track_menu_view(), 1);
    assert!(dispatch_key(&shell, "Left"));
    assert_eq!(shell.get_track_menu_view(), 0);
    assert_eq!(shell.get_track_menu_index(), 4);

    shell.invoke_track_menu_requested(request(300.0, 200.0));
    kit::click_by_id(&shell, "TrackMenu::menu-playlist").unwrap();
    assert!(dispatch_key(&shell, "Down"));
    assert!(dispatch_key(&shell, "Down"));
    assert_eq!(shell.get_track_menu_index(), 2);
    assert!(dispatch_key(&shell, "\n"));
    assert_eq!(shell.get_track_menu_last_action().as_str(), "playlist-new");

    let without_album = TrackMenuRequest {
        album: "".into(),
        ..request(300.0, 200.0)
    };
    shell.invoke_track_menu_requested(without_album);
    assert!(dispatch_key(&shell, "Up"));
    assert_eq!(shell.get_track_menu_index(), 5);
    assert!(dispatch_key(&shell, "Down"));
    assert_eq!(shell.get_track_menu_index(), 0);
    assert!(dispatch_key(&shell, "Escape"));
    assert!(!shell.get_track_menu_open());

    shell.set_page(3);
    shell.set_lib_chips(ModelRc::new(VecModel::from_slice(&[
        slint::SharedString::from("library"),
        slint::SharedString::from("playlists"),
        slint::SharedString::from("songs"),
        slint::SharedString::from("artists"),
        slint::SharedString::from("albums"),
        slint::SharedString::from("history"),
        slint::SharedString::from("stats"),
    ])));
    shell.set_lib_tab(2);
    shell.set_lib_songs(ModelRc::new(VecModel::from_slice(&[LibSongRow {
        id: "local:/music/alpha.flac".into(),
        title: "Alpha".into(),
        subtitle: "Alice".into(),
        liked: true,
        selected: false,
    }])));
    kit::mock_advance(std::time::Duration::from_millis(100));
    let song = kit::find_by_id(&shell, "LibraryPage::song-touch").unwrap();
    song.mock_single_click(slint::platform::PointerEventButton::Right);
    assert!(shell.get_track_menu_open());
    assert_eq!(
        shell.get_track_menu_track().id.as_str(),
        "local:/music/alpha.flac"
    );
    kit::click_by_id(&shell, "TrackMenu::menu-scrim").unwrap();
    kit::click_by_id(&shell, "LibraryPage::song-menu-button").unwrap();
    assert!(shell.get_track_menu_open());
    kit::click_by_id(&shell, "TrackMenu::menu-scrim").unwrap();
    shell.set_lib_tab(1);
    kit::mock_advance(std::time::Duration::from_millis(100));
    kit::click_by_id(&shell, "LibraryPage::new-playlist").unwrap();
    assert_eq!(shell.get_lib_last_action().as_str(), "new-playlist");
    shell.set_page(1);

    shell.set_queue_open(true);
    shell.set_queue_all(ModelRc::new(VecModel::from_slice(&[QueueRow {
        key: "beat1".into(),
        title: "Come Together".into(),
        artist: "The Beatles".into(),
        album: "Abbey Road".into(),
        current: true,
    }])));
    kit::mock_advance(std::time::Duration::from_millis(100));
    let queue_row = kit::find_by_id(&shell, "QueueDrawer::row-touch").unwrap();
    queue_row.mock_single_click(slint::platform::PointerEventButton::Right);
    assert!(shell.get_track_menu_open());
    assert_eq!(shell.get_track_menu_track().id.as_str(), "beat1");
    kit::click_by_id(&shell, "TrackMenu::menu-scrim").unwrap();
    kit::click_by_id(&shell, "QueueDrawer::row-menu-button").unwrap();
    assert!(shell.get_track_menu_open());
    kit::click_by_id(&shell, "TrackMenu::menu-scrim").unwrap();
    shell.set_queue_open(false);

    shell.set_has_track(true);
    shell.set_track_title("Come Together".into());
    shell.set_track_artist("The Beatles".into());
    shell.set_track_album("Abbey Road".into());
    shell.set_track_id("beat1".into());
    shell.set_full_player_open(true);
    kit::mock_advance(std::time::Duration::from_millis(100));
    kit::click_by_id(&shell, "FullPlayer::full-menu").unwrap();
    assert!(shell.get_track_menu_open());
    assert_eq!(shell.get_track_menu_track().id.as_str(), "beat1");
    assert_eq!(
        shell.get_track_menu_track().album.as_str(),
        "Abbey Road"
    );
    let player_shot = snapshot_dir.join("full-player-menu.png");
    kit::snapshot_to(shell.window(), &player_shot).unwrap();
    kit::click_by_id(&shell, "TrackMenu::menu-scrim").unwrap();
    shell.set_full_player_open(false);

    shell.invoke_track_menu_requested(request(620.0, 460.0));
    kit::mock_advance(std::time::Duration::from_millis(100));
    for (width, height) in kit::WINDOW_SIZES {
        kit::resize(&shell, width, height);
        let card = kit::find_by_id(&shell, "TrackMenu::menu-card").unwrap();
        let violations = kit::lint_layout_of(&card, (width, height));
        assert!(
            violations.is_empty(),
            "track menu at {width}x{height}: {violations:#?}"
        );
    }
    kit::resize(&shell, 1280.0, 800.0);

    shell.invoke_track_menu_requested(request(320.0, 180.0));
    kit::mock_advance(std::time::Duration::from_millis(100));
    let menu_shot = snapshot_dir.join("track-menu.png");
    kit::snapshot_to(shell.window(), &menu_shot).unwrap();
    let bytes = std::fs::read(&menu_shot).unwrap();
    assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    assert!(bytes.len() > 4096, "snapshot too small: {}", bytes.len());

    kit::click_by_id(&shell, "TrackMenu::menu-playlist").unwrap();
    kit::mock_advance(std::time::Duration::from_millis(100));
    let lists_shot = snapshot_dir.join("track-menu-playlists.png");
    kit::snapshot_to(shell.window(), &lists_shot).unwrap();

    shell.invoke_track_menu_requested(request(320.0, 180.0));
    kit::click_by_id(&shell, "TrackMenu::menu-scrim").unwrap();
    shell.set_page(3);
    shell.set_lib_chips(ModelRc::new(VecModel::from_slice(&[
        slint::SharedString::from("library"),
        slint::SharedString::from("playlists"),
        slint::SharedString::from("songs"),
        slint::SharedString::from("artists"),
        slint::SharedString::from("albums"),
        slint::SharedString::from("history"),
        slint::SharedString::from("stats"),
    ])));
    shell.set_lib_tab(1);
    shell.set_lib_playlists(ModelRc::new(VecModel::from_slice(&[])));
    shell.set_lib_tags(ModelRc::new(VecModel::from_slice(&[])));
    shell.set_lib_songs(ModelRc::new(VecModel::from_slice(&[])));
    shell.set_lib_sort_options(ModelRc::new(VecModel::from_slice(&[
        slint::SharedString::from("Recientes"),
        slint::SharedString::from("Nombre"),
        slint::SharedString::from("Artista"),
        slint::SharedString::from("Reproducciones"),
        slint::SharedString::from("Tiempo"),
    ])));
    kit::mock_advance(std::time::Duration::from_millis(100));
    kit::lint_layout_at_sizes(&shell, "library-empty-playlists");
    let empty_shot = snapshot_dir.join("library-empty-playlists.png");
    kit::resize(&shell, 1280.0, 800.0);
    kit::snapshot_to(shell.window(), &empty_shot).unwrap();

    shell.set_page(4);
    shell.set_dl_rows(ModelRc::new(VecModel::from(Vec::new())));
    shell.set_dl_failed(ModelRc::new(VecModel::from(Vec::new())));
    kit::mock_advance(std::time::Duration::from_millis(100));
    let downloads_shot = snapshot_dir.join("downloads-empty.png");
    kit::snapshot_to(shell.window(), &downloads_shot).unwrap();
}
