use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use madrigal_player::engine::{Engine, EngineConfig, EngineEvent};
use madrigal_player::stream_info::{PlaybackState, StreamInfo, StreamMeta};

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

#[test]
fn snapshot_idle_is_none() {
    if !have_libmpv() {
        println!("SKIP player_stream_info idle (missing libmpv)");
        return;
    }
    let engine = Engine::open(&EngineConfig::default()).expect("open engine");
    assert!(StreamInfo::snapshot(&engine, &StreamMeta::new("none")).is_none());
}

#[test]
fn snapshot_on_playing_wav() {
    if !have_ffmpeg() || !have_libmpv() {
        println!("SKIP player_stream_info playing (missing ffmpeg or libmpv)");
        return;
    }
    let root = tempfile::tempdir().expect("tempdir");
    let wav = make_wav(root.path(), "a.wav", 440, 6);
    let engine = Engine::open(&EngineConfig::default()).expect("open engine");
    engine
        .load(wav.to_string_lossy().as_ref())
        .expect("load wav");
    let events = collect_until(&engine, Instant::now() + Duration::from_secs(10), |events| {
        events.contains(&EngineEvent::StartFile) && events.contains(&EngineEvent::FileLoaded)
    });
    assert!(events.contains(&EngineEvent::StartFile), "playback started");
    std::thread::sleep(Duration::from_millis(600));

    let meta = StreamMeta::with_loudness("wav-a", -14.0);
    let info = StreamInfo::snapshot(&engine, &meta).expect("snapshot while playing");
    assert_eq!(info.media_id, "wav-a");
    assert_eq!(info.loudness_hint, Some(-14.0));
    assert_eq!(info.codec.as_deref(), Some("pcm_s16le"));
    assert_eq!(info.sample_rate, Some(44100));
    assert!(
        info.abr_kbps.is_some_and(|abr| (700.0..=710.0).contains(&abr)),
        "abr sane: {:?}",
        info.abr_kbps
    );
    assert!(
        info.buffer_health.is_some_and(|health| (0.0..=100.0).contains(&health)),
        "buffer sane: {:?}",
        info.buffer_health
    );
    assert!((info.speed - 1.0).abs() < 1e-6);
    assert_eq!(info.state, PlaybackState::Playing);

    engine.stop().expect("stop");
    let t0 = Instant::now();
    while StreamInfo::snapshot(&engine, &meta).is_some() && t0.elapsed() < Duration::from_secs(5)
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        StreamInfo::snapshot(&engine, &meta).is_none(),
        "idle snapshot is None after stop"
    );
}
