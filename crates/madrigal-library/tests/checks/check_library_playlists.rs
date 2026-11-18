use std::cell::RefCell;
use std::env;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};

use madrigal_core::{PlaylistId, Track, TrackIdentity};
use madrigal_library::{
    AddReport, AutoPlaylist, DuplicatePolicy, Library, PlaylistListFilter, SongSort, SortDir,
};
use madrigal_settings::Settings;

static PROFILE_LOCK: Mutex<()> = Mutex::new(());

fn lock_profile() -> MutexGuard<'static, ()> {
    PROFILE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

struct Fixture {
    _root: tempfile::TempDir,
    conn: rusqlite::Connection,
    settings: Rc<RefCell<Settings>>,
}

impl Fixture {
    fn setup() -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
        madrigal_paths::ensure_dirs().expect("dirs");
        let conn = madrigal_db::open(&madrigal_paths::db_path()).expect("open");
        Self {
            _root: root,
            conn,
            settings: Rc::new(RefCell::new(Settings::default())),
        }
    }

    fn library(&self) -> Library<'_> {
        Library::with_settings_source(&self.conn, || self.settings.borrow().clone())
    }
}

fn track(id: &str, title: &str, artists: &[&str]) -> Track {
    Track {
        identity: TrackIdentity::youtube(id),
        title: title.to_owned(),
        artists: artists.iter().map(|name| name.to_string()).collect(),
        album: None,
        duration_secs: Some(200),
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

fn keys(tracks: &[Track]) -> Vec<String> {
    tracks
        .iter()
        .map(|track| track.identity.to_storage_string())
        .collect()
}

#[test]
fn playlists_tags_cover_hidden_and_reorder() {
    let _guard = lock_profile();
    let fx = Fixture::setup();
    let lib = fx.library();

    let road = lib.create_playlist("Road Trip").expect("create");
    let gym = lib.create_playlist("Gym").expect("create");
    assert!(lib.create_playlist("   ").is_err());

    lib.assign_tag(&road.id, "rock").expect("tag");
    lib.assign_tag(&road.id, "driving").expect("tag");
    lib.assign_tag(&gym.id, "workout").expect("tag");
    assert!(lib.assign_tag(&road.id, "   ").is_err());
    assert_eq!(
        lib.playlist_tags(&road.id).expect("tags"),
        vec!["driving".to_owned(), "rock".to_owned()]
    );
    assert_eq!(
        lib.all_tags().expect("all tags"),
        vec![
            "driving".to_owned(),
            "rock".to_owned(),
            "workout".to_owned()
        ]
    );

    let tagged = lib
        .list_playlists(&PlaylistListFilter {
            tag: Some("rock".to_owned()),
            ..Default::default()
        })
        .expect("filter");
    assert_eq!(tagged.len(), 1);
    assert_eq!(tagged[0].id, road.id);
    assert_eq!(
        lib.list_playlists(&PlaylistListFilter::default())
            .expect("list")
            .len(),
        2
    );

    lib.remove_tag(&road.id, "driving").expect("untag");
    assert_eq!(
        lib.playlist_tags(&road.id).expect("tags"),
        vec!["rock".to_owned()]
    );

    assert!(lib.set_cover(&road.id, Some("/covers/road.jpg")).expect("cover"));
    assert_eq!(
        lib.get_playlist(&road.id)
            .expect("get")
            .expect("exists")
            .thumbnail
            .as_deref(),
        Some("/covers/road.jpg")
    );
    assert!(!lib.set_cover(&PlaylistId::new("missing"), Some("x")).expect("cover"));

    assert!(lib.set_hidden(&gym.id, true).expect("hide"));
    assert!(lib.is_hidden(&gym.id).expect("hidden"));
    assert!(!lib.is_hidden(&road.id).expect("hidden"));
    let visible = lib
        .list_playlists(&PlaylistListFilter::default())
        .expect("visible");
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, road.id);
    let with_hidden = lib
        .list_playlists(&PlaylistListFilter {
            include_hidden: true,
            ..Default::default()
        })
        .expect("with hidden");
    assert_eq!(with_hidden.len(), 2);
    assert!(lib.set_hidden(&gym.id, false).expect("unhide"));
    assert!(!lib.is_hidden(&gym.id).expect("hidden"));

    let first = track("aaa111", "Alpha", &["Zed"]);
    let second = track("bbb222", "Beta", &["Amy"]);
    let third = track("ccc333", "Gamma", &["Mid"]);
    let report = lib
        .add_tracks(&road.id, &[first.clone(), second.clone(), third.clone()], DuplicatePolicy::Skip)
        .expect("add");
    assert_eq!(
        report,
        AddReport {
            added: vec![
                first.identity.clone(),
                second.identity.clone(),
                third.identity.clone()
            ],
            skipped: Vec::new(),
        }
    );
    assert_eq!(
        keys(&lib.playlist_tracks(&road.id, SongSort::Custom, SortDir::Asc).expect("tracks")),
        vec!["aaa111".to_owned(), "bbb222".to_owned(), "ccc333".to_owned()]
    );
    assert_eq!(
        keys(&lib.playlist_tracks(&road.id, SongSort::Name, SortDir::Asc).expect("tracks")),
        vec!["aaa111".to_owned(), "bbb222".to_owned(), "ccc333".to_owned()]
    );
    assert_eq!(
        keys(&lib.playlist_tracks(&road.id, SongSort::Artist, SortDir::Asc).expect("tracks")),
        vec!["bbb222".to_owned(), "ccc333".to_owned(), "aaa111".to_owned()]
    );

    assert!(lib.move_track(&road.id, 0, 2).expect("move"));
    assert_eq!(
        keys(&lib.playlist_tracks(&road.id, SongSort::Custom, SortDir::Asc).expect("tracks")),
        vec!["bbb222".to_owned(), "ccc333".to_owned(), "aaa111".to_owned()]
    );
    assert!(!lib.move_track(&road.id, 0, 0).expect("noop"));
    assert!(!lib.move_track(&road.id, 0, 9).expect("oob"));
    assert!(!lib.move_track(&road.id, 9, 0).expect("oob"));

    assert!(lib.rename_playlist(&road.id, "Road Trip 2").expect("rename"));
    assert!(lib.create_playlist("x").is_ok());
    assert!(lib.rename_playlist(&road.id, "  ").is_err());
    assert_eq!(
        lib.get_playlist(&road.id).expect("get").expect("exists").title,
        "Road Trip 2"
    );
    assert!(lib.set_playlist_description(&road.id, Some("miles")).expect("desc"));
    assert_eq!(
        lib.get_playlist(&road.id)
            .expect("get")
            .expect("exists")
            .description
            .as_deref(),
        Some("miles")
    );

    assert!(lib.delete_playlist(&gym.id).expect("delete"));
    assert!(!lib.delete_playlist(&gym.id).expect("delete again"));
    assert!(lib.get_playlist(&gym.id).expect("get").is_none());
}

#[test]
fn auto_playlists_liked_and_downloaded_seam() {
    let _guard = lock_profile();
    let fx = Fixture::setup();
    let lib = fx.library();

    let first = track("aaa111", "Alpha", &["Zed"]);
    let second = track("bbb222", "Beta", &["Amy"]);
    madrigal_db::upsert_song(&fx.conn, &first).expect("seed");
    madrigal_db::upsert_song(&fx.conn, &second).expect("seed");

    assert!(lib.auto_tracks(AutoPlaylist::Liked).expect("liked").is_empty());
    lib.set_liked(&first.identity, true).expect("like");
    lib.set_liked(&second.identity, true).expect("like");
    lib.set_liked(&first.identity, true).expect("like again");
    assert!(lib.is_liked(&first.identity).expect("liked"));
    assert_eq!(
        keys(&lib.auto_tracks(AutoPlaylist::Liked).expect("liked")),
        vec!["aaa111".to_owned(), "bbb222".to_owned()]
    );

    let liked_scope = lib
        .songs(madrigal_library::SongScope::Liked, SongSort::Name, SortDir::Asc)
        .expect("liked scope");
    assert_eq!(keys(&liked_scope), vec!["aaa111".to_owned(), "bbb222".to_owned()]);
    let library_scope = lib
        .songs(madrigal_library::SongScope::Library, SongSort::Name, SortDir::Asc)
        .expect("library scope");
    assert_eq!(keys(&library_scope), vec!["aaa111".to_owned(), "bbb222".to_owned()]);

    lib.set_liked(&first.identity, false).expect("unlike");
    assert!(!lib.is_liked(&first.identity).expect("liked"));
    assert_eq!(
        keys(&lib.auto_tracks(AutoPlaylist::Liked).expect("liked")),
        vec!["bbb222".to_owned()]
    );

    assert!(lib.auto_tracks(AutoPlaylist::Downloaded).expect("downloaded").is_empty());
}

#[test]
fn duplicate_policy_skip_add_anyway_with_undo() {
    let _guard = lock_profile();
    let fx = Fixture::setup();
    let lib = fx.library();

    let playlist = lib.create_playlist("Singles").expect("create");
    let first = track("aaa111", "Alpha", &["Zed"]);
    let second = track("bbb222", "Beta", &["Amy"]);

    let added = lib
        .add_tracks(&playlist.id, &[first.clone(), second.clone()], DuplicatePolicy::Skip)
        .expect("add");
    assert_eq!(keys(&lib.playlist_tracks(&playlist.id, SongSort::Custom, SortDir::Asc).expect("tracks")), vec!["aaa111".to_owned(), "bbb222".to_owned()]);
    assert_eq!(added.skipped.len(), 0);

    let skipped = lib
        .add_tracks(&playlist.id, std::slice::from_ref(&first), DuplicatePolicy::Skip)
        .expect("skip");
    assert_eq!(skipped.added.len(), 0);
    assert_eq!(skipped.skipped, vec![first.identity.clone()]);
    assert_eq!(keys(&lib.playlist_tracks(&playlist.id, SongSort::Custom, SortDir::Asc).expect("tracks")), vec!["aaa111".to_owned(), "bbb222".to_owned()]);

    let forced = lib
        .add_tracks(&playlist.id, std::slice::from_ref(&first), DuplicatePolicy::AddAnyway)
        .expect("add anyway");
    assert_eq!(forced.added, vec![first.identity.clone()]);
    assert_eq!(keys(&lib.playlist_tracks(&playlist.id, SongSort::Custom, SortDir::Asc).expect("tracks")), vec!["bbb222".to_owned(), "aaa111".to_owned()]);

    assert!(lib.undo_last_add().expect("undo"));
    assert_eq!(keys(&lib.playlist_tracks(&playlist.id, SongSort::Custom, SortDir::Asc).expect("tracks")), vec!["aaa111".to_owned(), "bbb222".to_owned()]);
    assert!(lib.undo_last_add().expect("undo"));
    assert_eq!(keys(&lib.playlist_tracks(&playlist.id, SongSort::Custom, SortDir::Asc).expect("tracks")), vec!["aaa111".to_owned(), "bbb222".to_owned()]);
    assert!(lib.undo_last_add().expect("undo"));
    assert!(lib.playlist_tracks(&playlist.id, SongSort::Custom, SortDir::Asc).expect("tracks").is_empty());
    assert!(!lib.undo_last_add().expect("undo"));

    assert!(
        lib.add_tracks(&PlaylistId::new("missing"), &[first], DuplicatePolicy::Skip).is_err()
    );
}
