use std::collections::HashSet;

use madrigal_backup::{
    TrackEntry, diff_against, export_csv, export_json, load_csv, load_json, parse_csv, preview_csv,
    preview_file, preview_json, serialize_csv, serialize_json, validate_csv, validate_json,
};

fn entries() -> Vec<TrackEntry> {
    vec![
        TrackEntry {
            position: 0,
            title: "Never Gonna Give You Up".to_owned(),
            artists: vec!["Rick Astley".to_owned()],
            video_id: Some("dQw4w9WgXcQ".to_owned()),
            local_path: None,
            album: Some("Whenever You Need Somebody".to_owned()),
            duration: Some(212),
            thumbnail: Some("https://img/rick.jpg".to_owned()),
        },
        TrackEntry {
            position: 1,
            title: "Song, with \"quotes\" and Ünïcödé".to_owned(),
            artists: vec!["A & B".to_owned(), "C".to_owned()],
            video_id: None,
            local_path: Some("music/garage.mp3".to_owned()),
            album: None,
            duration: None,
            thumbnail: None,
        },
        TrackEntry {
            position: 2,
            title: "=cmd|evil".to_owned(),
            artists: vec![],
            video_id: Some("oHg5SJYRHA0".to_owned()),
            local_path: None,
            album: None,
            duration: None,
            thumbnail: None,
        },
    ]
}

fn playlist_meta() -> serde_json::Value {
    serde_json::json!({
        "id": "PLmix",
        "name": "Mix",
        "description": "seed",
    })
}

#[test]
fn json_roundtrip_is_byte_faithful() {
    let text = serialize_json(Some(&playlist_meta()), &entries(), None).expect("serialize");
    let parsed = madrigal_backup::parse_json(&text).expect("parse");
    assert_eq!(parsed["format"], "madrigal-playlist");
    assert_eq!(parsed["version"], 1);
    assert_eq!(parsed["tracks"].as_array().unwrap().len(), 3);
    let track = &parsed["tracks"][0];
    assert_eq!(track["videoId"], "dQw4w9WgXcQ");
    assert_eq!(track["artists"], serde_json::json!(["Rick Astley"]));
    assert_eq!(track["album"], "Whenever You Need Somebody");
    assert_eq!(track["duration"], 212);

    let report = validate_json(&parsed, false).expect("validate");
    assert!(report.valid, "{:?}", report.issues);
    assert_eq!(report.track_count, 3);
    assert_eq!(report.entries.len(), 3);

    let again = serialize_json(Some(&playlist_meta()), &report.entries, None).expect("reserialize");
    assert_eq!(text, again, "byte-faithful roundtrip");

    let legacy = text.replace("madrigal-playlist", "upstream-playlist");
    let legacy_report =
        validate_json(&madrigal_backup::parse_json(&legacy).unwrap(), false).unwrap();
    assert!(legacy_report.valid);
}

#[test]
fn csv_roundtrip_preserves_cells() {
    let text = serialize_csv(&entries()).expect("serialize");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "position,title,artists,videoId,localPath");
    assert_eq!(lines.len(), 4);
    assert!(lines[3].contains("'=cmd|evil"), "{lines:?}");

    let back = parse_csv(&text).expect("parse");
    assert_eq!(back.len(), 3);
    assert_eq!(back[0].video_id.as_deref(), Some("dQw4w9WgXcQ"));
    assert_eq!(back[1].title, "Song, with \"quotes\" and Ünïcödé");
    assert_eq!(back[1].artists, vec!["A & B".to_owned(), "C".to_owned()]);
    assert_eq!(back[1].local_path.as_deref(), Some("music/garage.mp3"));
    assert_eq!(back[2].title, "=cmd|evil");

    let aliased = "pos,titulo,artista,id,ruta\n0,Hola,X,abc,music/a.mp3\n";
    let parsed = parse_csv(aliased).expect("aliases parse");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].title, "Hola");
    assert_eq!(parsed[0].video_id.as_deref(), Some("abc"));

    let report = validate_csv(parse_csv(&text).unwrap(), false);
    assert!(report.valid, "{:?}", report.issues);
}

#[test]
fn preview_reports_problems_and_diff_without_writing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let json_path = dir.path().join("list.json");
    let csv_path = dir.path().join("list.csv");

    let payload = serde_json::json!({
        "format": "madrigal-playlist",
        "version": 1,
        "playlist": {"id": "PLx", "name": "Mix"},
        "tracks": [
            {"position": 0, "title": "Good", "artists": ["A"], "videoId": "vid1"},
            {"position": 1, "title": "Dup", "artists": ["A"], "videoId": "vid1"},
            {"position": 2, "title": "", "artists": ["B"], "videoId": "vid2"},
            {"position": 3, "title": "NoIdentity", "artists": ["C"]},
        ],
    });
    let report = validate_json(&payload, false).expect("validate");
    assert!(!report.valid);
    let codes: Vec<&str> = report
        .issues
        .iter()
        .map(|i| i.code.as_str())
        .collect();
    assert!(codes.contains(&"duplicate_video"), "{codes:?}");
    assert!(codes.contains(&"missing_title"), "{codes:?}");
    assert!(codes.contains(&"missing_identity"), "{codes:?}");

    let preview = preview_json(&serde_json::to_string(&payload).unwrap(), false).unwrap();
    assert_eq!(preview.track_count, 4);

    let existing: HashSet<String> = ["video:vid1".to_owned()].into_iter().collect();
    let diff = diff_against(&preview.entries, &existing);
    assert_eq!(diff.updates.len(), 2);
    assert_eq!(diff.adds.len(), 1);
    assert_eq!(diff.skipped.len(), 1);

    let csv_preview = preview_csv("position,title,artists,videoId,localPath\n0,Hola,X,abc,\n", false).unwrap();
    assert!(csv_preview.valid);
    assert_eq!(csv_preview.track_count, 1);

    export_json(Some(&playlist_meta()), &entries(), &json_path, None).expect("export json");
    export_csv(&entries(), &csv_path).expect("export csv");
    let from_file = preview_file(&json_path, false).expect("preview file json");
    assert!(from_file.valid);
    assert_eq!(from_file.entries.len(), 3);
    let from_csv = preview_file(&csv_path, false).expect("preview file csv");
    assert!(from_csv.valid);
    let loaded_json = load_json(&json_path).expect("load json");
    assert_eq!(loaded_json["tracks"].as_array().unwrap().len(), 3);
    let loaded_csv = load_csv(&csv_path).expect("load csv");
    assert_eq!(loaded_csv.len(), 3);

    let stray: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(stray.len(), 2, "{stray:?}");
}
