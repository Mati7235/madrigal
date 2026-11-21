use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use madrigal_app::actions::{wire_track_actions, TrackActions};
use madrigal_app::downloads::{downloads_dir_for, register_with, MediaRequest, UrlResolver};
use madrigal_app::playback::Playback;
use madrigal_core::{PlaylistId, TrackIdentity};
use madrigal_downloads::ManagerConfig;
use madrigal_library::{Library, PlaylistListFilter};
use madrigal_player::engine::EngineConfig;
use madrigal_settings::{Settings, Storage};
use madrigal_testkit as kit;
use madrigal_ui::{DetailKind, MainShell, TrackMenuRequest};
use slint::Model;

struct NoResolver;

impl UrlResolver for NoResolver {
    fn resolve(&self, identity: &TrackIdentity) -> Result<MediaRequest, String> {
        Err(format!("no stream for {}", identity.id))
    }
}

fn request(id: &str, title: &str, artist: &str, album: &str) -> TrackMenuRequest {
    TrackMenuRequest {
        id: id.into(),
        title: title.into(),
        artist: artist.into(),
        album: album.into(),
        duration_secs: 200,
        anchor_x: 300.0,
        anchor_y: 200.0,
    }
}

fn library(path: &Path) -> rusqlite::Connection {
    madrigal_db::open(path).expect("db")
}

fn playlist_id(conn: &rusqlite::Connection, title: &str) -> PlaylistId {
    let lib = Library::new(conn, Settings::default());
    lib.list_playlists(&PlaylistListFilter {
        tag: Some(String::new()),
        include_hidden: false,
        kind: None,
    })
    .unwrap()
    .into_iter()
    .find(|playlist| playlist.title == title)
    .expect("playlist exists")
    .id
}

fn playlist_keys(conn: &rusqlite::Connection, title: &str) -> Vec<String> {
    let lib = Library::new(conn, Settings::default());
    let id = playlist_id(conn, title);
    lib.default_playlist_tracks(&id)
        .unwrap()
        .into_iter()
        .map(|track| track.identity.to_storage_string())
        .collect()
}

fn queue_keys(core: &Arc<Mutex<Playback>>) -> Vec<String> {
    core.lock()
        .unwrap()
        .snapshot()
        .queue
        .all
        .iter()
        .map(|row| row.key.clone())
        .collect()
}

#[test]
fn app_actions() {
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, root.path());
    std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", root.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", root.path().join("state"));
    kit::init_headless();

    let settings = Settings::default();
    let db_path = root.path().join("actions.sqlite3");
    let check = library(&db_path);
    let shell = MainShell::new().expect("shell");
    let _lang = madrigal_ui::wire_shell_i18n(&shell, madrigal_i18n::Lang::Es);

    let storage = Storage {
        download_concurrency: 1,
        ..Storage::default()
    };
    let config = ManagerConfig::with_storage(
        root.path().join("downloads"),
        db_path.clone(),
        &storage,
    );
    let downloads = Rc::new(register_with(&shell, Arc::new(NoResolver), config));

    let playback = Playback::open(
        EngineConfig {
            vo: "null".to_owned(),
            ao: "null".to_owned(),
            gapless_audio: false,
        },
        &settings.player,
        madrigal_app::streams::orchestrator().expect("streams"),
    )
    .expect("playback");
    let core = Arc::new(Mutex::new(playback));

    wire_track_actions(
        &shell,
        TrackActions {
            conn: Rc::new(RefCell::new(library(&db_path))),
            settings: settings.clone(),
            downloads: Rc::clone(&downloads),
            core: Some(Arc::clone(&core)),
        },
    );

    let alpha = request("aaa", "Alpha", "Alice", "Hits");
    shell.invoke_track_menu_requested(alpha.clone());
    assert!(shell.get_track_menu_open());
    assert!(!shell.get_track_menu_liked());
    assert_eq!(shell.get_track_menu_playlists().row_count(), 0);

    shell.invoke_track_menu_action("like".into(), "".into());
    assert!(shell.get_track_menu_liked());
    let lib = Library::new(&check, Settings::default());
    assert!(lib.is_liked(&TrackIdentity::youtube("aaa")).unwrap());
    shell.invoke_track_menu_requested(alpha.clone());
    assert!(shell.get_track_menu_liked());
    shell.invoke_track_menu_action("like".into(), "".into());
    assert!(!shell.get_track_menu_liked());
    assert!(!lib.is_liked(&TrackIdentity::youtube("aaa")).unwrap());

    shell.invoke_track_menu_requested(alpha.clone());
    shell.invoke_track_menu_action("play-next".into(), "".into());
    assert_eq!(queue_keys(&core), ["aaa"]);
    let charlie = request("ccc", "Charlie", "Alice", "Hits");
    shell.invoke_track_menu_requested(charlie);
    shell.invoke_track_menu_action("queue".into(), "".into());
    let bravo = request("bbb", "Bravo", "Bob", "Deep");
    shell.invoke_track_menu_requested(bravo.clone());
    shell.invoke_track_menu_action("play-next".into(), "".into());
    assert_eq!(queue_keys(&core), ["aaa", "bbb", "ccc"]);
    let queue = core.lock().unwrap().snapshot().queue.all;
    assert_eq!(queue[1].title, "Bravo");
    assert_eq!(queue[1].artist, "Bob");

    shell.invoke_track_menu_requested(alpha.clone());
    shell.invoke_track_menu_action("playlist-new".into(), "".into());
    assert_eq!(shell.get_prompt_purpose().as_str(), "track-menu-playlist");
    assert_eq!(shell.get_overlay(), 2);
    shell.invoke_prompt_accepted("Prueba".into());
    assert_eq!(shell.get_overlay(), 0);
    assert_eq!(shell.get_prompt_purpose().as_str(), "");
    assert_eq!(playlist_keys(&check, "Prueba"), ["aaa"]);

    shell.invoke_track_menu_requested(bravo.clone());
    assert_eq!(shell.get_track_menu_playlists().row_count(), 1);
    assert_eq!(
        shell
            .get_track_menu_playlists()
            .row_data(0)
            .unwrap()
            .title
            .as_str(),
        "Prueba"
    );
    let id = playlist_id(&check, "Prueba");
    shell.invoke_track_menu_action("playlist".into(), id.as_str().into());
    assert_eq!(playlist_keys(&check, "Prueba"), ["aaa", "bbb"]);

    shell.invoke_lib_new_playlist_requested();
    assert_eq!(shell.get_prompt_purpose().as_str(), "library");
    shell.invoke_prompt_accepted("Vacía".into());
    assert!(playlist_keys(&check, "Vacía").is_empty());
    assert_eq!(shell.get_lib_last_action().as_str(), "menu-refresh");

    shell.invoke_track_menu_requested(alpha.clone());
    shell.invoke_track_menu_action("artist".into(), "".into());
    assert_eq!(shell.get_detail_stack(), 1);
    assert!(shell.get_detail_ref().kind == DetailKind::Artist);
    assert_eq!(shell.get_detail_ref().id.as_str(), "Alice");
    shell.invoke_detail_closed();
    shell.invoke_track_menu_action("album".into(), "".into());
    assert!(shell.get_detail_ref().kind == DetailKind::Album);
    assert_eq!(shell.get_detail_ref().id.as_str(), "Hits");

    let music = root.path().join("music");
    std::env::set_var("XDG_MUSIC_DIR", &music);
    assert_eq!(
        downloads_dir_for(&Settings::default()),
        music.join("Madrigal")
    );
    let mut custom = Settings::default();
    custom.storage.downloads_dir = "/tmp/madrigal-downloads".to_owned();
    assert_eq!(
        downloads_dir_for(&custom),
        PathBuf::from("/tmp/madrigal-downloads")
    );
    std::env::remove_var("XDG_MUSIC_DIR");

    assert!(downloads.state(&TrackIdentity::youtube("aaa")).is_none());
    println!("OK app_actions");
}
