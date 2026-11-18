use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use madrigal_core::TrackSource;
use madrigal_localmedia::{
    ScanConfig, artwork_dir, cache_size_bytes, prune_artwork, scan, store_artwork, sync_to_db,
};

fn ffmpeg(args: &[&str]) {
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "ffmpeg failed for {args:?}");
}

fn tone(path: &Path, freq: u32, secs: u32, codec: &[&str], tags: &[&str]) {
    let spec = format!("sine=frequency={freq}:duration={secs}");
    let mut args: Vec<&str> = vec!["-f", "lavfi", "-i", &spec];
    for tag in tags {
        args.push("-metadata");
        args.push(tag);
    }
    args.extend_from_slice(codec);
    args.push(path.to_str().unwrap());
    ffmpeg(&args);
}

fn attach_cover(audio: &Path, cover: &Path, out: &Path, extra: &[&str]) {
    let mut args: Vec<&str> = vec![
        "-i",
        audio.to_str().unwrap(),
        "-i",
        cover.to_str().unwrap(),
        "-map",
        "0",
        "-map",
        "1",
        "-c",
        "copy",
    ];
    args.extend_from_slice(extra);
    args.push("-disposition:v");
    args.push("attached_pic");
    args.push(out.to_str().unwrap());
    ffmpeg(&args);
}

fn find_result<'a>(
    results: &'a [madrigal_localmedia::ScanResult],
    name: &str,
) -> &'a madrigal_localmedia::ScanResult {
    results
        .iter()
        .find(|r| {
            r.track
                .local_path
                .as_deref()
                .is_some_and(|p| p.ends_with(name))
        })
        .unwrap_or_else(|| panic!("missing result for {name}"))
}

#[test]
fn local_folder_scan_upserts_and_prunes() {
    if Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        println!("SKIP localmedia_scan native (ffmpeg missing)");
        return;
    }
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    madrigal_paths::ensure_dirs().unwrap();

    let music = root.path().join("music");
    let excluded = music.join("skipme");
    fs::create_dir_all(&excluded).unwrap();

    let cover = root.path().join("cover.png");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "color=color=red:size=64x64:rate=1:duration=1",
        "-frames:v",
        "1",
        cover.to_str().unwrap(),
    ]);

    let bare_mp3 = root.path().join("bare.mp3");
    tone(
        &bare_mp3,
        440,
        5,
        &["-c:a", "libmp3lame", "-b:a", "128k", "-ar", "44100"],
        &[
            "title=Local Song",
            "artist=Local Artist",
            "album=Local Album",
            "track=3",
            "lyrics=first line\nsecond line",
        ],
    );
    let song_mp3 = music.join("song.mp3");
    attach_cover(&bare_mp3, &cover, &song_mp3, &["-id3v2_version", "3"]);

    let bare_flac = root.path().join("bare.flac");
    tone(
        &bare_flac,
        660,
        5,
        &["-c:a", "flac"],
        &[
            "title=Flac Song",
            "artist=Flac Artist;Guest Singer",
            "album=Flac Album",
            "track=7",
            "lyrics=flac lyric line",
        ],
    );
    attach_cover(&bare_flac, &cover, &music.join("song.flac"), &[]);

    tone(
        &music.join("song.wav"),
        880,
        5,
        &["-c:a", "pcm_s16le", "-ar", "44100"],
        &[
            "title=Wav Song",
            "artist=Wav Artist",
            "album=Wav Album",
            "track=2",
        ],
    );
    let bare_m4a = root.path().join("bare.m4a");
    tone(
        &bare_m4a,
        550,
        5,
        &["-c:a", "aac", "-b:a", "128k"],
        &[
            "title=M4a Song",
            "artist=M4a Artist",
            "album=M4a Album",
            "track=4",
            "lyrics=m4a lyric",
        ],
    );
    attach_cover(&bare_m4a, &cover, &music.join("song.m4a"), &[]);
    tone(
        &music.join("song.ogg"),
        520,
        5,
        &["-c:a", "libvorbis"],
        &[
            "title=Ogg Song",
            "artist=Ogg Artist",
            "album=Ogg Album",
            "track=5",
        ],
    );

    fs::copy(&song_mp3, excluded.join("song.mp3")).unwrap();
    tone(
        &music.join("short.wav"),
        990,
        1,
        &["-c:a", "pcm_s16le", "-ar", "44100"],
        &["title=Short Song"],
    );
    let raw = fs::read(&song_mp3).unwrap();
    fs::write(music.join("corrupt.mp3"), &raw[..raw.len().min(256)]).unwrap();
    fs::write(music.join("notes.txt"), "not audio\n").unwrap();

    let mut settings = madrigal_settings::Settings::default();
    settings.storage.local_songs_included_folders =
        vec![music.to_string_lossy().into_owned()];
    settings.storage.local_songs_excluded_folders =
        vec![excluded.to_string_lossy().into_owned()];
    settings.storage.max_image_cache_mb = 64;
    settings.content.local_songs_min_duration_secs = 2;
    settings.content.artist_separators = ";".to_owned();
    let config = ScanConfig::from_settings(&settings);
    assert_eq!(config.min_duration_secs, 2);
    assert_eq!(config.artwork_cache_limit_bytes, 64 * 1024 * 1024);
    assert_eq!(config.artist_separators, ";");
    assert_eq!(config.include_dirs, vec![music.clone()]);
    assert_eq!(config.exclude_dirs, vec![excluded.clone()]);

    let outcome = scan(&config);
    assert_eq!(outcome.results.len(), 5);
    assert_eq!(outcome.errors.len(), 1);
    assert!(outcome.errors[0].path.ends_with("corrupt.mp3"));
    assert_eq!(outcome.skipped_excluded, 1);
    assert_eq!(outcome.skipped_short, 1);

    let mp3 = find_result(&outcome.results, "music/song.mp3");
    assert_eq!(mp3.track.title, "Local Song");
    assert_eq!(mp3.track.artists, vec!["Local Artist"]);
    assert_eq!(mp3.track.album.as_deref(), Some("Local Album"));
    assert_eq!(mp3.track.track_number, Some(3));
    assert!(matches!(mp3.track.duration_secs, Some(d) if (4..=6).contains(&d)));
    assert_eq!(mp3.lyrics.as_deref(), Some("first line\nsecond line"));
    let art = mp3.artwork_path.as_ref().expect("mp3 artwork");
    assert!(art.starts_with(artwork_dir()));
    assert!(art.is_file());
    assert_eq!(
        mp3.track.thumbnail.as_deref(),
        Some(art.to_string_lossy().as_ref())
    );

    let flac = find_result(&outcome.results, "music/song.flac");
    assert_eq!(flac.track.title, "Flac Song");
    assert_eq!(flac.track.artists, vec!["Flac Artist", "Guest Singer"]);
    assert_eq!(flac.track.album.as_deref(), Some("Flac Album"));
    assert_eq!(flac.track.track_number, Some(7));
    assert!(matches!(flac.track.duration_secs, Some(d) if (4..=6).contains(&d)));
    assert_eq!(flac.lyrics.as_deref(), Some("flac lyric line"));
    assert!(flac.artwork_path.as_ref().is_some_and(|p| p.is_file()));

    let wav = find_result(&outcome.results, "music/song.wav");
    assert_eq!(wav.track.title, "Wav Song");
    assert_eq!(wav.track.artists, vec!["Wav Artist"]);
    assert_eq!(wav.track.album.as_deref(), Some("Wav Album"));
    assert_eq!(wav.track.track_number, Some(2));
    assert_eq!(wav.track.duration_secs, Some(5));
    assert!(wav.artwork_path.is_none());

    let m4a = find_result(&outcome.results, "music/song.m4a");
    assert_eq!(m4a.track.title, "M4a Song");
    assert_eq!(m4a.track.track_number, Some(4));
    assert!(matches!(m4a.track.duration_secs, Some(d) if (4..=6).contains(&d)));
    assert_eq!(m4a.lyrics.as_deref(), Some("m4a lyric"));
    assert!(m4a.artwork_path.as_ref().is_some_and(|p| p.is_file()));

    let ogg = find_result(&outcome.results, "music/song.ogg");
    assert_eq!(ogg.track.title, "Ogg Song");
    assert_eq!(ogg.track.track_number, Some(5));
    assert!(matches!(ogg.track.duration_secs, Some(d) if (4..=6).contains(&d)));
    assert!(ogg.artwork_path.is_none());

    for result in &outcome.results {
        assert_eq!(result.track.source(), TrackSource::Local);
        assert!(result.track.local_path.is_some());
        assert!(result.track.is_valid());
    }

    let conn = madrigal_db::open(&madrigal_paths::db_path()).expect("open db");
    assert_eq!(sync_to_db(&conn, &outcome.results).unwrap(), 5);
    assert_eq!(sync_to_db(&conn, &outcome.results).unwrap(), 5);
    assert_eq!(madrigal_db::list_songs(&conn).unwrap().len(), 5);
    for result in &outcome.results {
        let stored = madrigal_db::fetch_song(&conn, &result.track.identity)
            .unwrap()
            .expect("stored song");
        assert_eq!(stored.source(), TrackSource::Local);
        assert_eq!(stored.title, result.track.title);
        assert_eq!(stored.local_path, result.track.local_path);
    }

    let dir = artwork_dir();
    let total = cache_size_bytes(&dir);
    assert!(total > 0);
    assert_eq!(prune_artwork(&dir, total), 0);
    assert!(cache_size_bytes(&dir) <= total.max(1));
    let before: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(!before.is_empty());
    let extra = store_artwork(b"tiny-payload", "image/png", u64::MAX).unwrap();
    assert!(extra.is_file());
    let removed = prune_artwork(&dir, 0);
    assert!(removed > before.len());
    assert_eq!(cache_size_bytes(&dir), 0);
}
