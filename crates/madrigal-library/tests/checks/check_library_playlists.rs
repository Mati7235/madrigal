use std::cell::RefCell;
use std::env;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};

use madrigal_core::{PlaylistId, Track, TrackIdentity};
use madrigal_library::{
    AddReport, AutoPlaylist, DuplicatePolicy, Library, PlaylistListFilter, PlaylistOrder,
    PlaylistOriginFilter, PlaylistQuery, PlaylistSummary, SongSort, SortDir,
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

    assert!(lib
        .set_cover(&road.id, Some("/covers/road.jpg"))
        .expect("cover"));
    assert_eq!(
        lib.get_playlist(&road.id)
            .expect("get")
            .expect("exists")
            .thumbnail
            .as_deref(),
        Some("/covers/road.jpg")
    );
    assert!(!lib
        .set_cover(&PlaylistId::new("missing"), Some("x"))
        .expect("cover"));

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
        .add_tracks(
            &road.id,
            &[first.clone(), second.clone(), third.clone()],
            DuplicatePolicy::Skip,
        )
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
        keys(
            &lib.playlist_tracks(&road.id, SongSort::Custom, SortDir::Asc)
                .expect("tracks")
        ),
        vec![
            "aaa111".to_owned(),
            "bbb222".to_owned(),
            "ccc333".to_owned()
        ]
    );
    assert_eq!(
        keys(
            &lib.playlist_tracks(&road.id, SongSort::Name, SortDir::Asc)
                .expect("tracks")
        ),
        vec![
            "aaa111".to_owned(),
            "bbb222".to_owned(),
            "ccc333".to_owned()
        ]
    );
    assert_eq!(
        keys(
            &lib.playlist_tracks(&road.id, SongSort::Artist, SortDir::Asc)
                .expect("tracks")
        ),
        vec![
            "bbb222".to_owned(),
            "ccc333".to_owned(),
            "aaa111".to_owned()
        ]
    );

    assert!(lib.move_track(&road.id, 0, 2).expect("move"));
    assert_eq!(
        keys(
            &lib.playlist_tracks(&road.id, SongSort::Custom, SortDir::Asc)
                .expect("tracks")
        ),
        vec![
            "bbb222".to_owned(),
            "ccc333".to_owned(),
            "aaa111".to_owned()
        ]
    );
    assert!(!lib.move_track(&road.id, 0, 0).expect("noop"));
    assert!(!lib.move_track(&road.id, 0, 9).expect("oob"));
    assert!(!lib.move_track(&road.id, 9, 0).expect("oob"));

    assert!(lib
        .rename_playlist(&road.id, "Road Trip 2")
        .expect("rename"));
    assert!(lib.create_playlist("x").is_ok());
    assert!(lib.rename_playlist(&road.id, "  ").is_err());
    assert_eq!(
        lib.get_playlist(&road.id)
            .expect("get")
            .expect("exists")
            .title,
        "Road Trip 2"
    );
    assert!(lib
        .set_playlist_description(&road.id, Some("miles"))
        .expect("desc"));
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

    assert!(lib
        .auto_tracks(AutoPlaylist::Liked)
        .expect("liked")
        .is_empty());
    lib.set_liked(&first.identity, true).expect("like");
    lib.set_liked(&second.identity, true).expect("like");
    lib.set_liked(&first.identity, true).expect("like again");
    assert!(lib.is_liked(&first.identity).expect("liked"));
    assert_eq!(
        keys(&lib.auto_tracks(AutoPlaylist::Liked).expect("liked")),
        vec!["aaa111".to_owned(), "bbb222".to_owned()]
    );

    let liked_scope = lib
        .songs(
            madrigal_library::SongScope::Liked,
            SongSort::Name,
            SortDir::Asc,
        )
        .expect("liked scope");
    assert_eq!(
        keys(&liked_scope),
        vec!["aaa111".to_owned(), "bbb222".to_owned()]
    );
    let library_scope = lib
        .songs(
            madrigal_library::SongScope::Library,
            SongSort::Name,
            SortDir::Asc,
        )
        .expect("library scope");
    assert_eq!(
        keys(&library_scope),
        vec!["aaa111".to_owned(), "bbb222".to_owned()]
    );

    lib.set_liked(&first.identity, false).expect("unlike");
    assert!(!lib.is_liked(&first.identity).expect("liked"));
    assert_eq!(
        keys(&lib.auto_tracks(AutoPlaylist::Liked).expect("liked")),
        vec!["bbb222".to_owned()]
    );

    assert!(lib
        .auto_tracks(AutoPlaylist::Downloaded)
        .expect("downloaded")
        .is_empty());
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
        .add_tracks(
            &playlist.id,
            &[first.clone(), second.clone()],
            DuplicatePolicy::Skip,
        )
        .expect("add");
    assert_eq!(
        keys(
            &lib.playlist_tracks(&playlist.id, SongSort::Custom, SortDir::Asc)
                .expect("tracks")
        ),
        vec!["aaa111".to_owned(), "bbb222".to_owned()]
    );
    assert_eq!(added.skipped.len(), 0);

    let skipped = lib
        .add_tracks(
            &playlist.id,
            std::slice::from_ref(&first),
            DuplicatePolicy::Skip,
        )
        .expect("skip");
    assert_eq!(skipped.added.len(), 0);
    assert_eq!(skipped.skipped, vec![first.identity.clone()]);
    assert_eq!(
        keys(
            &lib.playlist_tracks(&playlist.id, SongSort::Custom, SortDir::Asc)
                .expect("tracks")
        ),
        vec!["aaa111".to_owned(), "bbb222".to_owned()]
    );

    let forced = lib
        .add_tracks(
            &playlist.id,
            std::slice::from_ref(&first),
            DuplicatePolicy::AddAnyway,
        )
        .expect("add anyway");
    assert_eq!(forced.added, vec![first.identity.clone()]);
    assert_eq!(
        keys(
            &lib.playlist_tracks(&playlist.id, SongSort::Custom, SortDir::Asc)
                .expect("tracks")
        ),
        vec!["bbb222".to_owned(), "aaa111".to_owned()]
    );

    assert!(lib.undo_last_add().expect("undo"));
    assert_eq!(
        keys(
            &lib.playlist_tracks(&playlist.id, SongSort::Custom, SortDir::Asc)
                .expect("tracks")
        ),
        vec!["aaa111".to_owned(), "bbb222".to_owned()]
    );
    assert!(lib.undo_last_add().expect("undo"));
    assert_eq!(
        keys(
            &lib.playlist_tracks(&playlist.id, SongSort::Custom, SortDir::Asc)
                .expect("tracks")
        ),
        vec!["aaa111".to_owned(), "bbb222".to_owned()]
    );
    assert!(lib.undo_last_add().expect("undo"));
    assert!(lib
        .playlist_tracks(&playlist.id, SongSort::Custom, SortDir::Asc)
        .expect("tracks")
        .is_empty());
    assert!(!lib.undo_last_add().expect("undo"));

    assert!(lib
        .add_tracks(&PlaylistId::new("missing"), &[first], DuplicatePolicy::Skip)
        .is_err());
}

fn summaries(lib: &Library<'_>, query: PlaylistQuery) -> Vec<PlaylistSummary> {
    lib.list_playlist_summaries(&query).expect("summaries")
}

fn names(rows: &[PlaylistSummary]) -> Vec<String> {
    rows.iter().map(|row| row.name.clone()).collect()
}

fn count(lib: &Library<'_>, id: &PlaylistId) -> Option<u32> {
    lib.get_playlist(id)
        .expect("get")
        .expect("exists")
        .track_count
}

#[test]
fn playlist_summaries_track_count_tracks_membership() {
    let _guard = lock_profile();
    let fx = Fixture::setup();
    let lib = fx.library();

    let list = lib.create_playlist("Mix").expect("create");
    let first = track("aaa111", "Alpha", &["Zed"]);
    let second = track("bbb222", "Beta", &["Amy"]);
    let third = track("ccc333", "Gamma", &["Mid"]);
    lib.add_tracks(
        &list.id,
        &[first.clone(), second.clone(), third.clone()],
        DuplicatePolicy::Skip,
    )
    .expect("add");
    assert_eq!(count(&lib, &list.id), Some(3));

    assert_eq!(
        lib.remove_tracks(&list.id, std::slice::from_ref(&second.identity))
            .expect("remove"),
        1
    );
    assert_eq!(count(&lib, &list.id), Some(2));

    assert!(lib.undo_last_add().expect("undo remove"));
    assert_eq!(count(&lib, &list.id), Some(3));

    assert!(lib.move_track(&list.id, 0, 2).expect("move"));
    assert_eq!(count(&lib, &list.id), Some(3));
    assert!(lib
        .remove_tracks(
            &PlaylistId::new("missing"),
            std::slice::from_ref(&first.identity),
        )
        .is_err());

    let copy = lib.duplicate_playlist(&list.id, "Mix Copy").expect("copy");
    assert_eq!(copy.track_count, Some(3));
    assert_eq!(count(&lib, &copy.id), Some(3));
    assert_eq!(
        keys(
            &lib.playlist_tracks(&copy.id, SongSort::Custom, SortDir::Asc)
                .expect("tracks")
        ),
        keys(
            &lib.playlist_tracks(&list.id, SongSort::Custom, SortDir::Asc)
                .expect("tracks")
        )
    );

    lib.add_tracks(
        &copy.id,
        std::slice::from_ref(&first),
        DuplicatePolicy::AddAnyway,
    )
    .expect("add anyway");
    assert_eq!(count(&lib, &copy.id), Some(3));

    madrigal_db::set_playlist_tracks(
        &fx.conn,
        &list.id,
        std::slice::from_ref(&third.identity),
        1_700_000_000,
    )
    .expect("import");
    assert_eq!(count(&lib, &list.id), Some(1));
    assert_eq!(
        lib.get_playlist(&list.id)
            .expect("get")
            .expect("exists")
            .track_count,
        Some(1)
    );

    let summary = summaries(&lib, PlaylistQuery::default())
        .into_iter()
        .find(|row| row.id == list.id)
        .expect("summary");
    assert_eq!(summary.track_count, 1);
    assert_eq!(summary.updated_at, 1_700_000_000_000);
}

#[test]
fn playlist_summaries_order_name_tracks_and_recent() {
    let _guard = lock_profile();
    let fx = Fixture::setup();
    let lib = fx.library();

    let alpha = lib.create_playlist("alpha").expect("create");
    let mango = lib.create_playlist("Mango").expect("create");
    let zulu = lib.create_playlist("Zulu").expect("create");

    let first = track("aaa111", "Alpha", &["Zed"]);
    let second = track("bbb222", "Beta", &["Amy"]);
    lib.add_tracks(
        &alpha.id,
        &[first.clone(), second.clone()],
        DuplicatePolicy::Skip,
    )
    .expect("add");
    lib.add_tracks(
        &zulu.id,
        std::slice::from_ref(&first),
        DuplicatePolicy::Skip,
    )
    .expect("add");

    let by_name = summaries(
        &lib,
        PlaylistQuery {
            order: PlaylistOrder::Name,
            ..Default::default()
        },
    );
    assert_eq!(names(&by_name), vec!["alpha", "Mango", "Zulu"]);

    let by_tracks = summaries(
        &lib,
        PlaylistQuery {
            order: PlaylistOrder::Tracks,
            ..Default::default()
        },
    );
    let ordered: Vec<&PlaylistId> = by_tracks.iter().map(|row| &row.id).collect();
    assert_eq!(ordered, vec![&alpha.id, &zulu.id, &mango.id]);
    assert_eq!(by_tracks[0].track_count, 2);

    fx.conn
        .execute(
            "UPDATE playlists SET updated_at = 1 WHERE id = ?1",
            rusqlite::params![alpha.id.as_str()],
        )
        .expect("age alpha");
    let recent = summaries(&lib, PlaylistQuery::default());
    assert_eq!(recent[0].id, zulu.id);

    lib.add_tracks(
        &alpha.id,
        std::slice::from_ref(&first),
        DuplicatePolicy::Skip,
    )
    .expect("add");
    let recent = summaries(&lib, PlaylistQuery::default());
    assert_eq!(recent[0].id, alpha.id);

    assert!(lib
        .set_playlist_description(&alpha.id, Some("morning"))
        .expect("desc"));
    assert!(lib
        .set_cover(&alpha.id, Some("/covers/alpha.jpg"))
        .expect("cover"));
    let summary = summaries(&lib, PlaylistQuery::default())
        .into_iter()
        .find(|row| row.id == alpha.id)
        .expect("summary");
    assert_eq!(summary.subtitle.as_deref(), Some("morning"));
    assert_eq!(summary.cover.as_deref(), Some("/covers/alpha.jpg"));
    assert_eq!(summary.origin, madrigal_library::PlaylistOrigin::Local);
    assert!(summary.capabilities.rename && summary.capabilities.export);
}

#[test]
fn playlist_summaries_search_and_origin_filter() {
    let _guard = lock_profile();
    let fx = Fixture::setup();
    let lib = fx.library();

    let alpine = lib.create_playlist("Alpine Drive").expect("create");
    let alpha = lib.create_playlist("Alpha").expect("create");
    let _zulu = lib.create_playlist("Zulu").expect("create");

    let searched = summaries(
        &lib,
        PlaylistQuery {
            text: Some("  ALP  ".to_owned()),
            ..Default::default()
        },
    );
    assert_eq!(searched.len(), 2);
    assert!(searched.iter().all(|row| row.name.starts_with("Alp")));

    let empty = summaries(
        &lib,
        PlaylistQuery {
            text: Some("nothing".to_owned()),
            ..Default::default()
        },
    );
    assert!(empty.is_empty());

    let local = summaries(
        &lib,
        PlaylistQuery {
            origin: PlaylistOriginFilter::Local,
            ..Default::default()
        },
    );
    assert_eq!(local.len(), 3);
    let account = summaries(
        &lib,
        PlaylistQuery {
            origin: PlaylistOriginFilter::Account,
            ..Default::default()
        },
    );
    assert!(account.is_empty());
    let all = summaries(&lib, PlaylistQuery::default());
    assert_eq!(all.len(), 3);
    let paged = summaries(
        &lib,
        PlaylistQuery {
            limit: Some(2),
            offset: 1,
            ..Default::default()
        },
    );
    assert_eq!(paged.len(), 2);
    assert_eq!(paged[0].id, all[1].id);
    assert_eq!(paged[1].id, all[2].id);
    assert!(all.iter().any(|row| row.id == alpine.id));
    assert!(all.iter().any(|row| row.id == alpha.id));
}

#[test]
fn migration_adds_updated_at_and_backfills_counts() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = root.path().join("legacy.sqlite3");
    let mut conn = rusqlite::Connection::open(&path).expect("open");
    madrigal_db::migrate_to(&mut conn, 1).expect("v1");
    assert_eq!(madrigal_db::user_version(&conn).expect("version"), 1);
    conn.execute(
        "INSERT INTO playlists (id, kind, title, owned) VALUES ('local-old', 'local', 'Old Mix', 1)",
        [],
    )
    .expect("legacy playlist");
    conn.execute(
        "INSERT INTO playlist_tracks (playlist_id, song_key, position, added_at) VALUES
         ('local-old', 'youtube:aaa111', 0, 1000),
         ('local-old', 'youtube:bbb222', 1, 1000)",
        [],
    )
    .expect("legacy tracks");

    madrigal_db::migrate_to(&mut conn, madrigal_db::latest_version()).expect("v2");
    assert_eq!(
        madrigal_db::user_version(&conn).expect("version"),
        madrigal_db::latest_version() as i64
    );
    let (updated_at, track_count): (i64, i64) = conn
        .query_row(
            "SELECT updated_at, track_count FROM playlists WHERE id = 'local-old'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("migrated row");
    assert!(updated_at > 0);
    assert_eq!(track_count, 2);

    let lib = Library::new(&conn, Settings::default());
    let summary = summaries(
        &lib,
        PlaylistQuery {
            text: Some("old".to_owned()),
            ..Default::default()
        },
    )
    .into_iter()
    .next()
    .expect("summary");
    assert_eq!(summary.track_count, 2);
    assert_eq!(summary.updated_at, updated_at);
    assert_eq!(summary.origin, madrigal_library::PlaylistOrigin::Local);
}
