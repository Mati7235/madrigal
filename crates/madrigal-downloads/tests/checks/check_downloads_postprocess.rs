use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use madrigal_downloads::{cut_segments, probe_duration, read_chapters, Chapter};

fn tool(bin: &str) -> PathBuf {
    PathBuf::from(bin)
}

fn run(cmd: &mut Command) {
    let out = cmd.output().expect("spawn tool");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn make_source(dir: &Path) -> PathBuf {
    let wav = dir.join("source.wav");
    run(Command::new(tool("ffmpeg").as_os_str()).args([
        "-y",
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=20",
        "-ar",
        "44100",
        "-ac",
        "1",
        wav.to_str().unwrap(),
    ]));
    let meta = dir.join("chapters.txt");
    fs::write(
        &meta,
        ";FFMETADATA1\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=0\nEND=5000\ntitle=Intro\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=5000\nEND=15000\ntitle=Main\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=15000\nEND=20000\ntitle=Outro\n",
    )
    .unwrap();
    let tagged = dir.join("tagged.m4a");
    run(Command::new(tool("ffmpeg").as_os_str())
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(&wav)
        .arg("-i")
        .arg(&meta)
        .arg("-map")
        .arg("0:a")
        .arg("-c:a")
        .arg("aac")
        .arg("-map_metadata")
        .arg("1")
        .arg("-map_chapters")
        .arg("1")
        .arg(&tagged));
    tagged
}

#[test]
fn cut_two_segments_and_rewrite_chapters() {
    for bin in ["ffmpeg", "ffprobe"] {
        assert!(
            Command::new(bin).arg("-version").output().is_ok_and(|o| o.status.success()),
            "{bin} is required for the native postprocess check"
        );
    }
    let root = tempfile::tempdir().expect("tempdir");
    let input = make_source(root.path());
    let duration = probe_duration(&tool("ffprobe"), &input).expect("probe");
    assert!((duration - 20.0).abs() < 0.6, "duration {duration}");
    let before = read_chapters(&tool("ffprobe"), &input).expect("chapters");
    assert_eq!(before.len(), 3);

    let chapters = vec![
        Chapter {
            title: "Intro".to_string(),
            start_secs: 0.0,
            end_secs: 5.0,
        },
        Chapter {
            title: "Main".to_string(),
            start_secs: 5.0,
            end_secs: 15.0,
        },
        Chapter {
            title: "Outro".to_string(),
            start_secs: 15.0,
            end_secs: 20.0,
        },
    ];
    let cut = cut_segments(
        &input,
        &[(2.0, 4.0), (10.0, 13.0)],
        &chapters,
        &tool("ffmpeg"),
        &tool("ffprobe"),
    )
    .expect("cut");
    let produced = probe_duration(&tool("ffprobe"), &cut).expect("probe cut");
    assert!(
        (produced - 15.0).abs() < 0.8,
        "expected ~15s after removing 5s, got {produced}"
    );
    let after = read_chapters(&tool("ffprobe"), &cut).expect("cut chapters");
    assert_eq!(after.len(), 3, "chapters {after:?}");
    let titles: Vec<&str> = after.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, vec!["Intro", "Main", "Outro"]);
    let windows: Vec<(f64, f64)> = after.iter().map(|c| (c.start_secs, c.end_secs)).collect();
    assert!((windows[0].0 - 0.0).abs() < 0.6, "intro start {windows:?}");
    assert!((windows[0].1 - 3.0).abs() < 0.6, "intro end {windows:?}");
    assert!((windows[1].0 - 3.0).abs() < 0.6, "main start {windows:?}");
    assert!((windows[1].1 - 10.0).abs() < 0.6, "main end {windows:?}");
    assert!((windows[2].0 - 10.0).abs() < 0.6, "outro start {windows:?}");
    assert!((windows[2].1 - 15.0).abs() < 0.6, "outro end {windows:?}");
}

#[test]
fn empty_segments_are_rejected() {
    let root = tempfile::tempdir().expect("tempdir");
    let input = make_source(root.path());
    let result = cut_segments(&input, &[], &[], &tool("ffmpeg"), &tool("ffprobe"));
    assert!(result.is_err());
}
