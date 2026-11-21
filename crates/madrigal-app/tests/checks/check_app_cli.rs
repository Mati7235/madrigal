use std::fs;
use std::path::{Path, PathBuf};

use madrigal_app::cli;
use madrigal_app::playback::track_from_file;
use madrigal_core::TrackSource;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .join("madrigal-lyrics/tests/fixtures")
        .join(name)
}

fn frame(id: &[u8; 4], text: &str) -> Vec<u8> {
    let mut body = vec![0u8];
    body.extend(text.bytes());
    let mut out = id.to_vec();
    out.extend((body.len() as u32).to_be_bytes());
    out.extend([0u8, 0u8]);
    out.extend(body);
    out
}

fn id3v23(title: &str, artist: &str, album: &str) -> Vec<u8> {
    let mut frames = Vec::new();
    frames.extend(frame(b"TIT2", title));
    frames.extend(frame(b"TPE1", artist));
    frames.extend(frame(b"TALB", album));
    let size = frames.len();
    let mut out = b"ID3\x03\x00\x00".to_vec();
    out.extend([
        ((size >> 21) & 0x7f) as u8,
        ((size >> 14) & 0x7f) as u8,
        ((size >> 7) & 0x7f) as u8,
        (size & 0x7f) as u8,
    ]);
    out.extend(frames);
    out
}

#[test]
fn app_cli() {
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, root.path());
    std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", root.path().join("cache"));

    let opts = cli::parse([
        "a.mp3",
        "--smoke-quit-after-ms",
        "50",
        "b.flac",
        "--profile=work",
    ])
    .expect("parse");
    assert_eq!(opts.files, vec!["a.mp3".to_owned(), "b.flac".to_owned()]);
    assert_eq!(opts.smoke_quit_after_ms, Some(50));
    assert_eq!(opts.profile.as_deref(), Some("work"));
    assert!(cli::parse(["--unknown"]).is_err());
    assert!(cli::parse(["--smoke-quit-after-ms"]).is_err());

    let clip = fixture("embedded_uslt.mp3");
    let tags = madrigal_localmedia::parse_file(&clip).expect("parse clip");
    assert_eq!(tags.lyrics.as_deref(), Some("First line\nSecond line"));
    assert_eq!(tags.duration_secs, Some(1));

    let (identity, meta) = track_from_file(&clip).expect("track from clip");
    assert_eq!(identity.source, TrackSource::Local);
    assert_eq!(identity.id, clip.to_string_lossy());
    assert_eq!(meta.title, "embedded_uslt");
    assert_eq!(meta.artist, "");
    assert_eq!(meta.album, "");
    assert_eq!(meta.duration_secs, Some(1));

    let audio = fs::read(&clip).expect("read clip");
    let tagged = root.path().join("tagged.mp3");
    let mut bytes = id3v23("Fixture Title", "Fixture Artist", "Fixture Album");
    bytes.extend(audio);
    fs::write(&tagged, bytes).expect("write tagged");
    let (_, meta) = track_from_file(&tagged).expect("track from tagged");
    assert_eq!(meta.title, "Fixture Title");
    assert_eq!(meta.artist, "Fixture Artist");
    assert_eq!(meta.album, "Fixture Album");
    assert_eq!(meta.duration_secs, Some(1));

    let fallback = root.path().join("Sin Etiquetas.mp3");
    fs::write(&fallback, fs::read(&clip).expect("read clip")).expect("write fallback");
    let (_, meta) = track_from_file(&fallback).expect("track fallback");
    assert_eq!(meta.title, "Sin Etiquetas");

    assert!(track_from_file(&root.path().join("missing.mp3")).is_err());

    println!("OK app_cli");
}
