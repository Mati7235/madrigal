use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use madrigal_core::{
    Album, AlbumId, Artist, ArtistId, Event, Playlist, PlaylistId, PlaylistKind, SearchHistoryItem,
    Track, TrackIdentity,
};
use madrigal_settings::Settings;

static PROFILE_LOCK: Mutex<()> = Mutex::new(());

fn lock_profile() -> MutexGuard<'static, ()> {
    PROFILE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn isolate() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    env::remove_var("XDG_CONFIG_HOME");
    env::remove_var("XDG_DATA_HOME");
    env::remove_var("XDG_CACHE_HOME");
    root
}

fn song(video: &str, title: &str) -> Track {
    Track {
        identity: TrackIdentity::youtube(video),
        title: title.to_owned(),
        artists: vec!["Rick Astley".to_owned()],
        album: Some("Whenever You Need Somebody".to_owned()),
        duration_secs: Some(212),
        thumbnail: None,
        track_number: Some(1),
        local_path: None,
        set_video_id: None,
    }
}

fn local_song(id: &str, title: &str, path: &str) -> Track {
    Track {
        identity: TrackIdentity::local(id),
        title: title.to_owned(),
        artists: vec!["Local Band".to_owned()],
        album: None,
        duration_secs: Some(180),
        thumbnail: None,
        track_number: None,
        local_path: Some(path.to_owned()),
        set_video_id: None,
    }
}

fn seed_profile(cover_bytes: &[u8]) -> (Settings, HashMap<String, PathBuf>, PathBuf) {
    madrigal_paths::ensure_dirs().unwrap();
    let db_path = madrigal_paths::db_path();
    let conn = madrigal_db::open(&db_path).expect("open seed db");

    let first = song("dQw4w9WgXcQ", "Never Gonna Give You Up");
    let second = song("oHg5SJYRHA0", "Crab Rave");
    let local = local_song("track01", "Garage Take", "music/garage.mp3");
    for track in [&first, &second, &local] {
        madrigal_db::upsert_song(&conn, track).unwrap();
    }
    madrigal_db::upsert_artist(
        &conn,
        &Artist {
            id: ArtistId::new("UCtest"),
            name: "Rick Astley".to_owned(),
            subscribers: Some("1M".to_owned()),
            thumbnail: None,
            description: None,
        },
    )
    .unwrap();
    madrigal_db::upsert_album(
        &conn,
        &Album {
            id: AlbumId::new("MPREtest"),
            title: "Whenever You Need Somebody".to_owned(),
            artists: vec!["Rick Astley".to_owned()],
            artist_id: Some(ArtistId::new("UCtest")),
            year: Some("1987".to_owned()),
            thumbnail: None,
            track_count: Some(10),
            audio_playlist_id: None,
        },
    )
    .unwrap();

    let playlist = Playlist {
        id: PlaylistId::new("PLmix"),
        kind: PlaylistKind::Local,
        title: "Mix".to_owned(),
        author: Some("yo".to_owned()),
        description: Some("seed".to_owned()),
        thumbnail: None,
        track_count: Some(2),
        owned: true,
        privacy: None,
        author_id: None,
    };
    madrigal_db::upsert_playlist(&conn, &playlist).unwrap();
    madrigal_db::set_playlist_tracks(
        &conn,
        &playlist.id,
        &[first.identity.clone(), local.identity.clone()],
        1_700_000_000,
    )
    .unwrap();
    madrigal_db::tag_playlist(&conn, &playlist.id, "favoritas").unwrap();
    madrigal_db::record_event(
        &conn,
        &Event {
            track: first.identity.clone(),
            played_at: 1_700_000_001,
            duration_secs: Some(200),
            completed: true,
        },
    )
    .unwrap();
    madrigal_db::upsert_download(
        &conn,
        &local.identity,
        PathBuf::from("music/garage.mp3").as_path(),
        &local,
        Some(4096),
        1_700_000_002,
    )
    .unwrap();
    madrigal_db::cache_lyrics(&conn, &first.identity, "[00:01.00] hello", 1_700_000_003).unwrap();
    madrigal_db::record_search(
        &conn,
        &SearchHistoryItem {
            query: "rick astley".to_owned(),
            searched_at: 1_700_000_004,
        },
    )
    .unwrap();
    drop(conn);

    let mut settings = Settings::default();
    settings.player.volume = 42;
    settings.save().expect("save seed settings");

    let cover_path = madrigal_paths::data_dir().join("seed-cover.png");
    std::fs::write(&cover_path, cover_bytes).unwrap();
    let mut covers = HashMap::new();
    covers.insert("PLmix".to_owned(), cover_path);
    (settings, covers, db_path)
}

#[test]
fn snapshot_restore_roundtrip_matches() {
    let _profile = lock_profile();
    let _seed_root = isolate();
    let cover_bytes = b"\x89PNG seed cover bytes";
    let (settings, covers, db_path) = seed_profile(cover_bytes);

    let archive = _seed_root.path().join("backup.json");
    madrigal_backup::snapshot_to(&db_path, &settings, &covers, &archive).expect("snapshot");
    assert!(archive.exists());
    let text = std::fs::read_to_string(&archive).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["format"], "madrigal-backup");
    assert_eq!(parsed["version"], 1);

    let _fresh_root = isolate();
    let report = madrigal_backup::restore(
        &archive,
        &madrigal_backup::RestoreOptions::default(),
    )
    .expect("restore");
    assert_eq!(report.songs_added, 3);
    assert_eq!(report.playlists_added, 1);
    assert_eq!(report.events_added, 1);
    assert_eq!(report.covers_restored.len(), 1);

    let conn = madrigal_db::open(&madrigal_paths::db_path()).expect("open restored");
    let songs = madrigal_db::list_songs(&conn).unwrap();
    assert_eq!(songs.len(), 3);
    for want in [
        song("dQw4w9WgXcQ", "Never Gonna Give You Up"),
        song("oHg5SJYRHA0", "Crab Rave"),
        local_song("track01", "Garage Take", "music/garage.mp3"),
    ] {
        let got = madrigal_db::fetch_song(&conn, &want.identity)
            .unwrap()
            .expect("song restored");
        assert_eq!(got, want);
    }
    let tracks =
        madrigal_db::fetch_playlist_tracks(&conn, &PlaylistId::new("PLmix")).unwrap();
    assert_eq!(
        tracks,
        vec![
            song("dQw4w9WgXcQ", "Never Gonna Give You Up"),
            local_song("track01", "Garage Take", "music/garage.mp3"),
        ]
    );
    let tags: Vec<String> = conn
        .prepare("SELECT tag FROM playlist_tags WHERE playlist_id = 'PLmix'")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(tags, vec!["favoritas".to_owned()]);
    assert_eq!(madrigal_db::recent_events(&conn, 10).unwrap().len(), 1);
    assert!(
        madrigal_db::fetch_lyrics(&conn, &song("dQw4w9WgXcQ", "").identity)
            .unwrap()
            .is_some()
    );
    let restored_cover = &report.covers_restored[0];
    assert_eq!(std::fs::read(restored_cover).unwrap(), cover_bytes);
    drop(conn);

    let back = Settings::load();
    assert_eq!(back.player.volume, 42);
}

#[test]
fn restore_keeps_local_entries_on_conflict() {
    let _profile = lock_profile();
    let _seed_root = isolate();
    let (settings, covers, db_path) = seed_profile(b"cover");
    let archive = _seed_root.path().join("backup.json");
    madrigal_backup::snapshot_to(&db_path, &settings, &covers, &archive).expect("snapshot");

    let _fresh_root = isolate();
    madrigal_paths::ensure_dirs().unwrap();
    let conn = madrigal_db::open(&madrigal_paths::db_path()).expect("open target");
    let mut rival = song("dQw4w9WgXcQ", "Never Gonna Give You Up");
    rival.title = "Local Rival Title".to_owned();
    madrigal_db::upsert_song(&conn, &rival).unwrap();
    let rival_playlist = Playlist {
        id: PlaylistId::new("PLmix"),
        kind: PlaylistKind::Local,
        title: "Local Mix".to_owned(),
        author: None,
        description: None,
        thumbnail: None,
        track_count: None,
        owned: true,
        privacy: None,
        author_id: None,
    };
    madrigal_db::upsert_playlist(&conn, &rival_playlist).unwrap();
    madrigal_db::set_playlist_tracks(
        &conn,
        &rival_playlist.id,
        &[song("oHg5SJYRHA0", "Crab Rave").identity],
        1_700_000_010,
    )
    .unwrap();
    drop(conn);

    let report = madrigal_backup::restore(
        &archive,
        &madrigal_backup::RestoreOptions::default(),
    )
    .expect("restore");
    assert_eq!(report.songs_kept, 1);
    assert_eq!(report.songs_added, 2);
    assert_eq!(report.playlists_kept, 1);

    let conn = madrigal_db::open(&madrigal_paths::db_path()).expect("reopen");
    let kept = madrigal_db::fetch_song(&conn, &rival.identity)
        .unwrap()
        .expect("rival survives");
    assert_eq!(kept.title, "Local Rival Title");
    let tracks = madrigal_db::fetch_playlist_tracks(&conn, &PlaylistId::new("PLmix")).unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].title, "Crab Rave");
    assert!(madrigal_db::fetch_song(&conn, &song("oHg5SJYRHA0", "").identity).unwrap().is_some());
}

#[test]
fn restore_refuses_newer_major_version() {
    let _profile = lock_profile();
    let _seed_root = isolate();
    let (settings, covers, db_path) = seed_profile(b"cover");
    let archive = _seed_root.path().join("backup.json");
    madrigal_backup::snapshot_to(&db_path, &settings, &covers, &archive).expect("snapshot");

    let mut tampered: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&archive).unwrap()).unwrap();
    tampered["version"] = serde_json::json!(madrigal_backup::BACKUP_VERSION + 1);
    madrigal_files::atomic_write_text(
        &archive,
        &serde_json::to_string_pretty(&tampered).unwrap(),
    )
    .unwrap();

    let _fresh_root = isolate();
    let err = madrigal_backup::restore(&archive, &madrigal_backup::RestoreOptions::default())
        .expect_err("newer version must fail");
    assert!(
        matches!(err, madrigal_backup::Error::UnsupportedVersion(_)),
        "unexpected: {err}"
    );
}
