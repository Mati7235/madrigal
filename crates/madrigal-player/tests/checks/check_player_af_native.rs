use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use madrigal_core::{TrackIdentity, TrackSource};
use madrigal_player::audio::{build_af, equalizer_af, LOUDNORM_AF, SKIP_SILENCE_AF};
use madrigal_player::engine::{EndReason, Engine, EngineConfig, EngineEvent};
use madrigal_player::player::Player;
use madrigal_player::sleep::SleepMode;
use madrigal_player::source::{ResolvedStream, StreamSource};
use madrigal_settings::{Normalization, Player as PlayerConfig};

struct MapSource {
    dir: PathBuf,
}

impl StreamSource for MapSource {
    fn resolve(&self, track: &TrackIdentity) -> Result<ResolvedStream, String> {
        let path = self.dir.join(format!("{}.wav", track.id));
        if path.is_file() {
            Ok(ResolvedStream::new(path.to_string_lossy().into_owned()))
        } else {
            Err(format!("no fixture for {}", track.id))
        }
    }
}

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

fn make_wav(dir: &Path, name: &str, freq: u32, secs: u32) {
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
}

fn track(name: &str) -> TrackIdentity {
    TrackIdentity::new(TrackSource::Local, name)
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

fn full_config() -> PlayerConfig {
    PlayerConfig {
        volume: 70,
        playback_speed: 1.25,
        pitch_semitones: 1.0,
        equalizer_enabled: true,
        equalizer_gains: vec![5.0, 5.0, 4.0, 3.0, 1.0, 0.0, 0.0, -1.0, -1.0, -1.0],
        normalization: Normalization::Dynamic,
        skip_silence: true,
        ..PlayerConfig::default()
    }
}

#[test]
fn af_chain_sleep_and_autoskip() {
    if !have_ffmpeg() || !have_libmpv() {
        println!("SKIP player_af_native (missing ffmpeg or libmpv)");
        return;
    }
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    make_wav(root.path(), "a.wav", 440, 2);
    make_wav(root.path(), "b.wav", 660, 2);
    make_wav(root.path(), "good.wav", 880, 3);
    std::fs::write(root.path().join("corrupt.wav"), b"not audio at all").expect("write");

    let config = EngineConfig::default();
    let engine = Engine::open(&config).expect("open engine");

    let gains = vec![5.0, 5.0, 4.0, 3.0, 1.0, 0.0, 0.0, -1.0, -1.0, -1.0];
    let eq = equalizer_af(&gains).expect("eq chain");
    engine.apply_af(&eq).expect("equalizer");
    engine.apply_af(LOUDNORM_AF).expect("loudnorm");
    engine.apply_af(SKIP_SILENCE_AF).expect("silenceremove");
    engine.apply_af("scaletempo2").expect("scaletempo2");
    engine
        .apply_af("rubberband=pitch-scale=1.059463")
        .expect("rubberband");
    let full = build_af(&full_config());
    assert!(full.contains("equalizer="));
    engine.apply_af(&full).expect("full chain");
    engine.apply_af("").expect("clear af");

    {
        let engine = Engine::open(&config).expect("open engine");
        engine
            .load(root.path().join("a.wav").to_string_lossy().as_ref())
            .expect("load a");
        let started = collect_until(&engine, Instant::now() + Duration::from_secs(10), |events| {
            events.contains(&EngineEvent::StartFile)
        });
        assert!(started.contains(&EngineEvent::StartFile));
        let bad = engine.apply_af("equalizer=f=1000:badopt=1");
        assert!(bad.is_err(), "bad af must fail closed, got {bad:?}");
    }

    let source = MapSource {
        dir: root.path().to_path_buf(),
    };
    let mut player = Player::new(engine, source);
    player.apply_settings(&full_config()).expect("apply settings");
    assert_eq!(player.engine().speed().expect("speed"), 1.25);
    assert!(player.engine().audio_pitch_correction().expect("correction"));

    player.set_volume(42).expect("set volume");
    assert_eq!(player.engine().volume().expect("volume"), 42.0);
    player.set_volume(0).expect("zero volume");
    assert!(player.engine().paused().expect("paused at zero"));
    assert_eq!(player.engine().volume().expect("muted"), 0.0);
    player.resume(&PlayerConfig::default()).expect("resume");
    assert!(!player.engine().paused().expect("resumed"));
    assert_eq!(player.engine().volume().expect("restored"), 42.0);

    let (a, b) = (track("a"), track("b"));
    assert!(player.queue_mut().replace(vec![a.clone(), b.clone()], 0));
    player.play().expect("play a");
    player.arm_sleep(SleepMode::EndOfSong, Instant::now());
    let events = collect_until(
        player.engine(),
        Instant::now() + Duration::from_secs(15),
        |events| events.contains(&EngineEvent::EndFile(EndReason::Eof)),
    );
    let eof = events
        .iter()
        .find(|e| **e == EngineEvent::EndFile(EndReason::Eof))
        .cloned()
        .expect("real eof on short wav");
    assert!(!player.handle_event(&eof).expect("sleep fires"));
    assert_eq!(player.queue().current(), Some(&a));
    assert!(player.engine().paused().expect("paused by sleep"));
    assert!(player.sleep_remaining(Instant::now()).is_none());
    let again = player
        .handle_event(&EngineEvent::EndFile(EndReason::Eof))
        .expect("advance after sleep");
    assert!(again);
    assert_eq!(player.queue().current(), Some(&b));
    player.engine().set_paused(false).expect("resume for b");
    let events = collect_until(
        player.engine(),
        Instant::now() + Duration::from_secs(10),
        |events| events.contains(&EngineEvent::EndFile(EndReason::Eof)),
    );
    let eof = events
        .iter()
        .find(|e| **e == EngineEvent::EndFile(EndReason::Eof))
        .cloned()
        .expect("eof of b");
    assert!(!player.handle_event(&eof).expect("queue exhausted"));

    let (corrupt, good) = (track("corrupt"), track("good"));
    assert!(player
        .queue_mut()
        .replace(vec![corrupt.clone(), good.clone()], 0));
    player.play().expect("play corrupt");
    std::thread::sleep(Duration::from_millis(2600));
    assert_eq!(player.engine().playlist_pos().expect("dead load"), -1);
    assert!(player.poll_failed_load(Instant::now()).expect("auto-skip"));
    assert_eq!(player.queue().current(), Some(&good));
    assert_eq!(player.consecutive_skips(), 1);
    let events = collect_until(
        player.engine(),
        Instant::now() + Duration::from_secs(10),
        |events| events.contains(&EngineEvent::FileLoaded),
    );
    assert!(events.contains(&EngineEvent::FileLoaded));
    player
        .handle_event(&EngineEvent::FileLoaded)
        .expect("confirm good");
    assert_eq!(player.consecutive_skips(), 0);

    assert!(player
        .queue_mut()
        .replace(vec![track("good"), track("b")], 0));
    let routed = player
        .handle_event(&EngineEvent::EndFile(EndReason::Error))
        .expect("route error like eof");
    assert!(routed);
    assert_eq!(player.queue().current(), Some(&track("b")));
    assert_eq!(player.consecutive_skips(), 1);
    let events = collect_until(
        player.engine(),
        Instant::now() + Duration::from_secs(10),
        |events| events.contains(&EngineEvent::StartFile),
    );
    assert!(events.contains(&EngineEvent::StartFile));

    let engine = Engine::open(&config).expect("open engine");
    let source = MapSource {
        dir: root.path().to_path_buf(),
    };
    let mut preloader = Player::new(engine, source);
    assert!(preloader
        .queue_mut()
        .replace(vec![track("a"), track("b")], 0));
    preloader.play().expect("play a");
    assert!(preloader.preload_next().expect("preload"));
    assert_eq!(preloader.engine().playlist_len().expect("len"), 2);
    assert!(preloader
        .queue_mut()
        .replace(vec![track("a"), track("missing")], 0));
    assert!(!preloader.preload_next().expect("failed resolve"));
}
