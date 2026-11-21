use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use madrigal_player::engine::{EndReason, Engine, EngineConfig, EngineEvent};

fn have_ffmpeg() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|out| out.status.success())
}

fn have_libmpv() -> bool {
    if PathBuf::from("/usr/lib/libmpv.so").exists() {
        return true;
    }
    Command::new("ldconfig").arg("-p").output().is_ok_and(|out| {
        String::from_utf8_lossy(&out.stdout).contains("libmpv.so")
    })
}

fn make_wav(dir: &Path, name: &str, freq: u32, secs: u32) -> PathBuf {
    let out = dir.join(name);
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency={freq}:duration={secs}"),
            "-ar",
            "44100",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&out)
        .output()
        .expect("run ffmpeg");
    assert!(status.status.success(), "ffmpeg: {status:?}");
    assert!(out.is_file());
    assert!(out.metadata().expect("stat").len() > 44100);
    out
}

fn collect_until(
    engine: &Engine,
    deadline: Instant,
    mut done: impl FnMut(&[EngineEvent]) -> bool,
) -> Vec<EngineEvent> {
    let mut all = Vec::new();
    while Instant::now() < deadline {
        match engine.events().recv_timeout(Duration::from_millis(200)) {
            Ok(event) => {
                all.push(event);
                if done(&all) {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if done(&all) {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    all
}

fn count_starts(events: &[EngineEvent]) -> usize {
    events
        .iter()
        .filter(|e| **e == EngineEvent::StartFile)
        .count()
}

fn count_eofs(events: &[EngineEvent]) -> usize {
    events
        .iter()
        .filter(|e| **e == EngineEvent::EndFile(EndReason::Eof))
        .count()
}

#[test]
fn engine_properties_pause_and_gapless_sequence() {
    if !have_ffmpeg() || !have_libmpv() {
        println!("SKIP player_engine native (missing ffmpeg or libmpv)");
        return;
    }
    let root = tempfile::tempdir().expect("tempdir");
    let wav1 = make_wav(root.path(), "a.wav", 440, 8);
    let wav2 = make_wav(root.path(), "b.wav", 660, 8);
    let wav3 = make_wav(root.path(), "c.wav", 880, 8);

    let config = EngineConfig {
        vo: "null".to_owned(),
        ao: "null".to_owned(),
        gapless_audio: true,
    };
    let engine = Engine::open(&config).expect("open engine");

    engine.set_volume(42.0).expect("set volume");
    assert_eq!(engine.volume().expect("get volume"), 42.0);
    engine.set_speed(1.25).expect("set speed");
    assert_eq!(engine.speed().expect("get speed"), 1.25);
    engine.set_speed(1.0).expect("reset speed");

    engine
        .load(wav1.to_string_lossy().as_ref())
        .expect("load wav1");
    engine
        .append(wav2.to_string_lossy().as_ref())
        .expect("append wav2");
    engine
        .append(wav3.to_string_lossy().as_ref())
        .expect("append wav3");
    assert_eq!(engine.playlist_len().expect("playlist len"), 3);

    let mut all = collect_until(&engine, Instant::now() + Duration::from_secs(10), |events| {
        count_starts(events) >= 1 && events.contains(&EngineEvent::FileLoaded)
    });
    assert!(count_starts(&all) >= 1, "first StartFile");

    engine.seek(1.0).expect("seek");
    std::thread::sleep(Duration::from_millis(400));
    let pos = engine.position().expect("time-pos after seek");
    assert!((pos - 1.0).abs() < 1.5, "pos {pos}");

    engine.set_paused(true).expect("pause");
    assert!(engine.paused().expect("paused prop"));
    std::thread::sleep(Duration::from_millis(150));
    let frozen = engine.position().expect("pos while paused");
    std::thread::sleep(Duration::from_millis(600));
    let still = engine.position().expect("pos while paused");
    assert!((still - frozen).abs() < 0.2, "frozen {frozen} -> {still}");
    engine.set_paused(false).expect("resume");
    std::thread::sleep(Duration::from_millis(400));
    let moving = engine.position().expect("pos after resume");
    assert!(moving > still, "resumed {still} -> {moving}");

    engine
        .apply_af("equalizer=f=1000:t=o:w=1:g=-3")
        .expect("valid af");
    let bad = engine.apply_af("equalizer=f=1000:t=width-type:o:w=1:g=0");
    assert!(bad.is_err(), "bad af must fail via log, got {bad:?}");
    engine.apply_af("").expect("clear af");

    for last in [false, false, true] {
        std::thread::sleep(Duration::from_millis(300));
        engine.seek(6.8).expect("seek near end");
        let chunk = collect_until(&engine, Instant::now() + Duration::from_secs(15), |events| {
            count_eofs(events) >= 1
        });
        assert_eq!(count_eofs(&chunk), 1, "EndFile(Eof)");
        all.extend(chunk);
        if !last {
            let chunk = collect_until(
                &engine,
                Instant::now() + Duration::from_secs(15),
                |events| count_starts(events) >= 1,
            );
            assert_eq!(count_starts(&chunk), 1, "next StartFile");
            all.extend(chunk);
        }
    }

    let kinds: Vec<String> = all
        .iter()
        .filter_map(|event| match event {
            EngineEvent::StartFile => Some("start".to_owned()),
            EngineEvent::EndFile(EndReason::Eof) => Some("eof".to_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, vec!["start", "eof", "start", "eof", "start", "eof"]);

    engine.stop().expect("stop");
}

#[test]
fn auto_ao_starts_playback() {
    if !have_ffmpeg() || !have_libmpv() {
        println!("SKIP player_engine auto ao (missing ffmpeg or libmpv)");
        return;
    }
    let root = tempfile::tempdir().expect("tempdir");
    let wav = make_wav(root.path(), "auto.wav", 440, 8);
    let config = EngineConfig {
        vo: "null".to_owned(),
        ao: "auto".to_owned(),
        gapless_audio: false,
    };
    let engine = Engine::open(&config).expect("open engine");
    engine.set_volume(0.0).expect("mute");
    engine
        .load(wav.to_string_lossy().as_ref())
        .expect("load wav");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut loaded = false;
    let mut position = 0.0;
    while Instant::now() < deadline {
        for event in engine.drain() {
            if event == EngineEvent::FileLoaded {
                loaded = true;
            }
        }
        position = engine.position().unwrap_or(0.0);
        if loaded && position > 0.2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(loaded, "no FileLoaded with ao=auto");
    assert!(
        position > 0.2,
        "ao=auto did not start playback (position {position}):\n{}",
        engine.logs().join("\n")
    );
}
