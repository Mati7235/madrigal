use std::env;
use std::fs;

use madrigal_core::{Track, TrackIdentity};
use madrigal_db::{fetch_song, latest_version, open, upsert_song, user_version, SQLITE_MAGIC};

fn sample_track() -> Track {
    Track {
        identity: TrackIdentity::youtube("dQw4w9WgXcQ"),
        title: "Never Gonna Give You Up".to_owned(),
        artists: vec!["Rick Astley".to_owned()],
        album: None,
        duration_secs: Some(212),
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

fn quarantine_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .starts_with("library.sqlite3.corrupt-")
        })
        .collect()
}

#[test]
fn damaged_header_is_quarantined_and_fresh_db_takes_over() {
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    madrigal_paths::ensure_dirs().unwrap();
    let path = madrigal_paths::db_path();

    let conn = open(&path).expect("open fresh");
    upsert_song(&conn, &sample_track()).unwrap();
    drop(conn);

    let mut bytes = fs::read(&path).unwrap();
    assert!(bytes.len() > 512);
    bytes[..16].copy_from_slice(b"GARBAGE-HEADER!!");
    bytes.truncate(512);
    fs::write(&path, &bytes).unwrap();

    let conn = open(&path).expect("open recovers");
    assert_eq!(user_version(&conn).unwrap(), latest_version() as i64);
    upsert_song(&conn, &sample_track()).unwrap();
    assert!(fetch_song(&conn, &sample_track().identity).unwrap().is_some());
    drop(conn);

    let dir = path.parent().unwrap().to_path_buf();
    let moved: Vec<_> = quarantine_files(&dir);
    assert_eq!(moved.len(), 1, "corrupt file must be quarantined");
    let name = moved[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(name.starts_with("library.sqlite3.corrupt-"), "{name}");
    assert!(moved[0].metadata().unwrap().len() as usize >= bytes.len());
}

#[test]
fn empty_and_healthy_files_are_not_quarantined() {
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    madrigal_paths::ensure_dirs().unwrap();
    let path = madrigal_paths::db_path();

    fs::write(&path, b"").unwrap();
    let conn = open(&path).expect("empty file opens without quarantine");
    assert_eq!(user_version(&conn).unwrap(), latest_version() as i64);
    upsert_song(&conn, &sample_track()).unwrap();
    drop(conn);
    assert!(
        quarantine_files(path.parent().unwrap()).is_empty(),
        "empty file must not be quarantined"
    );

    let before = fs::read(&path).unwrap();
    assert_eq!(&before[..16], SQLITE_MAGIC);
    let conn = open(&path).expect("healthy db reopens");
    assert!(fetch_song(&conn, &sample_track().identity).unwrap().is_some());
    drop(conn);
    assert!(
        quarantine_files(path.parent().unwrap()).is_empty(),
        "healthy db must not be quarantined"
    );
}
