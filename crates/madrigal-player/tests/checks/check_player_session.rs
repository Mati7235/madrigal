use std::path::PathBuf;
use std::process::Command;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use madrigal_core::{TrackIdentity, TrackSource};
use madrigal_player::engine::{EndReason, Engine, EngineConfig, EngineEvent};
use madrigal_player::player::Player;
use madrigal_player::session;
use madrigal_player::source::{ResolvedStream, StreamSource};

struct MapSource {
    dir: PathBuf,
    resolves: AtomicUsize,
}

impl StreamSource for MapSource {
    fn resolve(&self, track: &TrackIdentity) -> Result<ResolvedStream, String> {
        self.resolves.fetch_add(1, Ordering::SeqCst);
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

fn make_wav(dir: &std::path::Path, name: &str, freq: u32, secs: u32) {
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

#[test]
fn session_roundtrip_restores_paused_and_defers_resolve() {
    if !have_ffmpeg() || !have_libmpv() {
        println!("SKIP player_session native (missing ffmpeg or libmpv)");
        return;
    }
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    make_wav(root.path(), "a.wav", 440, 4);
    make_wav(root.path(), "b.wav", 660, 4);
    let (a, b) = (track("a"), track("b"));

    let config = EngineConfig::default();
    {
        let engine = Engine::open(&config).expect("open engine");
        let source = MapSource {
            dir: root.path().to_path_buf(),
            resolves: AtomicUsize::new(0),
        };
        let mut player = Player::new(engine, source);
        assert!(player.queue_mut().replace(vec![a.clone(), b.clone()], 1));
        player.engine().set_paused(true).expect("pause");
        let state = player.snapshot().expect("snapshot");
        assert_eq!(state.tracks, vec![a.clone(), b.clone()]);
        assert_eq!(state.position, Some(1));
        assert!(state.paused);
        let path = session::save(&state).expect("save");
        assert!(path.starts_with(root.path()));
        assert!(path.is_file());
    }

    let engine = Engine::open(&config).expect("open engine");
    let source = MapSource {
        dir: root.path().to_path_buf(),
        resolves: AtomicUsize::new(0),
    };
    let mut player = Player::new(engine, source);
    let state = session::load().expect("load").expect("session file");
    assert_eq!(state.tracks, vec![a.clone(), b.clone()]);
    assert_eq!(state.position, Some(1));
    assert!(player.restore(&state).expect("restore"));
    assert_eq!(player.queue().current(), Some(&b));
    assert_eq!(
        player
            .queue()
            .current_index()
            .and_then(|i| player.queue().items().get(i)),
        Some(&b)
    );
    assert!(player.engine().paused().expect("paused restored"));
    assert_eq!(
        player.source().resolves.load(Ordering::SeqCst),
        0,
        "no resolve before first play"
    );

    player.play().expect("play");
    assert_eq!(player.source().resolves.load(Ordering::SeqCst), 1);
    let events = collect_until(
        player.engine(),
        Instant::now() + Duration::from_secs(10),
        |events| events.contains(&EngineEvent::StartFile),
    );
    assert!(
        events.contains(&EngineEvent::StartFile),
        "playback started"
    );
    let t0 = Instant::now();
    while player.engine().position().is_none() && t0.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(player.engine().position().is_some(), "time advances");
    assert_eq!(player.queue().current(), Some(&b));

    let events = collect_until(
        player.engine(),
        Instant::now() + Duration::from_secs(15),
        |events| events.contains(&EngineEvent::EndFile(EndReason::Eof)),
    );
    let eof = events
        .iter()
        .find(|e| **e == EngineEvent::EndFile(EndReason::Eof))
        .cloned()
        .expect("eof");
    assert!(!player.handle_event(&eof).expect("handle eof"));
    assert!(player.engine().position().is_none());
}
