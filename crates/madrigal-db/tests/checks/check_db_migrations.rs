use std::env;

use madrigal_core::{Playlist, PlaylistId, Track, TrackIdentity};
use madrigal_db::{fetch_song, latest_version, open, upsert_song, user_version};

fn sample_track() -> Track {
    Track {
        identity: TrackIdentity::youtube("dQw4w9WgXcQ"),
        title: "Never Gonna Give You Up".to_owned(),
        artists: vec!["Rick Astley".to_owned()],
        album: Some("Whenever You Need Somebody".to_owned()),
        duration_secs: Some(212),
        thumbnail: None,
        track_number: Some(1),
        local_path: None,
        set_video_id: None,
    }
}

#[test]
fn fresh_db_migrates_to_latest_and_reopen_is_idempotent() {
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    madrigal_paths::ensure_dirs().unwrap();
    let path = madrigal_paths::db_path();
    assert!(!path.exists());

    let conn = open(&path).expect("open fresh");
    assert_eq!(user_version(&conn).unwrap(), latest_version() as i64);
    assert_eq!(latest_version(), 3);
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    for expected in [
        "songs",
        "artists",
        "albums",
        "playlists",
        "playlist_tracks",
        "tags",
        "playlist_tags",
        "events",
        "downloads",
        "lyrics_cache",
        "search_history",
        "recognitions",
    ] {
        assert!(tables.contains(&expected.to_owned()), "missing {expected}");
    }
    upsert_song(&conn, &sample_track()).unwrap();
    drop(conn);

    let again = open(&path).expect("reopen");
    assert_eq!(user_version(&again).unwrap(), latest_version() as i64);
    let fetched = fetch_song(&again, &sample_track().identity)
        .unwrap()
        .expect("song survives reopen");
    assert_eq!(fetched, sample_track());
    drop(again);

    let playlist = Playlist {
        id: PlaylistId::new("PLtest"),
        kind: madrigal_core::PlaylistKind::Local,
        title: "Mix".to_owned(),
        author: None,
        description: None,
        thumbnail: None,
        track_count: None,
        owned: true,
        privacy: None,
        author_id: None,
    };
    let conn = open(&path).expect("third open");
    madrigal_db::upsert_playlist(&conn, &playlist).unwrap();
    madrigal_db::set_playlist_tracks(
        &conn,
        &playlist.id,
        &[sample_track().identity],
        1_700_000_000,
    )
    .unwrap();
    let tracks = madrigal_db::fetch_playlist_tracks(&conn, &playlist.id).unwrap();
    assert_eq!(tracks, vec![sample_track()]);
    madrigal_db::record_event(
        &conn,
        &madrigal_core::Event {
            track: sample_track().identity,
            played_at: 1_700_000_001,
            duration_secs: Some(200),
            completed: true,
        },
    )
    .unwrap();
    assert_eq!(madrigal_db::recent_events(&conn, 10).unwrap().len(), 1);
}
