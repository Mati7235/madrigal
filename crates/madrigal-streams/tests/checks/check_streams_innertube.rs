use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use madrigal_core::TrackIdentity;
use madrigal_streams::cache::now_epoch;
use madrigal_streams::{
    InnertubeResolver, InnertubeStream, Orchestrator, PoConfig, StreamConfig, INNERTUBE_LABEL,
};

static ENV: Mutex<()> = Mutex::new(());

struct FakeInnertube {
    calls: AtomicUsize,
    fail: bool,
}

impl FakeInnertube {
    fn new(fail: bool) -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            fail,
        })
    }
}

impl InnertubeResolver for FakeInnertube {
    fn resolve(&self, video_id: &str) -> Result<InnertubeStream, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err("visionos rejected".to_string());
        }
        Ok(InnertubeStream {
            url: format!("https://innertube.test/stream?v={video_id}&expire=9999999999"),
            user_agent: Some("visionos-agent/1.0".to_string()),
            ext: Some("m4a".to_string()),
            acodec: Some("mp4a.40.2".to_string()),
            expires_at: Some(now_epoch() + 3600.0),
        })
    }
}

fn setup() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    let bin = root.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_ytdlp.sh");
    let shim = bin.join("yt-dlp");
    fs::copy(&source, &shim).unwrap();
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
    let log = root.path().join("args.log");
    env::set_var("FAKE_YTDLP_LOG", &log);
    env::remove_var("FAKE_MODE");
    let cache_dir = root.path().join("cache");
    fs::create_dir_all(&cache_dir).unwrap();
    (root, shim, log, cache_dir)
}

fn config(shim: &Path) -> StreamConfig {
    StreamConfig {
        ytdlp_bin: shim.to_path_buf(),
        timeout_secs: 10,
        po: PoConfig {
            enabled: false,
            ..PoConfig::default()
        },
        innertube: false,
        ..StreamConfig::default()
    }
}

#[test]
fn innertube_wins_then_caches_and_falls_back() {
    let _guard = ENV.lock().expect("env lock");
    let (_root, shim, log, cache_dir) = setup();

    let fast = FakeInnertube::new(false);
    let client = Orchestrator::with_innertube(
        config(&shim),
        &cache_dir,
        Arc::clone(&fast) as Arc<dyn InnertubeResolver>,
    );
    let identity = TrackIdentity::youtube("INNERTUBE01");
    let (stream, label) = client.resolve(&identity, None).expect("innertube resolve");
    assert_eq!(label, INNERTUBE_LABEL);
    assert_eq!(stream.resolution_path, INNERTUBE_LABEL);
    assert_eq!(stream.client, "visionos");
    assert_eq!(stream.user_agent.as_deref(), Some("visionos-agent/1.0"));
    assert_eq!(fast.calls.load(Ordering::SeqCst), 1);
    assert!(!log.exists(), "yt-dlp ran despite innertube hit");

    let (cached, cached_label) = client.resolve(&identity, None).expect("cached");
    assert_eq!(cached_label, "cache");
    assert_eq!(cached.url, stream.url);
    assert_eq!(fast.calls.load(Ordering::SeqCst), 1);
    assert!(!log.exists(), "cache hit reached innertube or yt-dlp");

    let slow = FakeInnertube::new(true);
    let dir = cache_dir.join("fallback");
    fs::create_dir_all(&dir).unwrap();
    let fallback = Orchestrator::with_innertube(
        config(&shim),
        &dir,
        Arc::clone(&slow) as Arc<dyn InnertubeResolver>,
    );
    let (stream, label) = fallback
        .resolve(&TrackIdentity::youtube("INNERTUBE02"), None)
        .expect("fallback resolve");
    assert_eq!(label, "default");
    assert_eq!(slow.calls.load(Ordering::SeqCst), 1);
    assert!(stream.url.starts_with("https://"));
    let logged = fs::read_to_string(&log).expect("yt-dlp log");
    assert!(logged.contains("INNERTUBE02"), "yt-dlp args: {logged}");
}

#[test]
fn disabled_innertube_goes_straight_to_ytdlp() {
    let _guard = ENV.lock().expect("env lock");
    let (_root, shim, log, cache_dir) = setup();
    let client = Orchestrator::with_cache_dir(config(&shim), &cache_dir);
    let (_, label) = client
        .resolve(&TrackIdentity::youtube("DIRECTID001"), None)
        .expect("yt-dlp resolve");
    assert_eq!(label, "default");
    let logged = fs::read_to_string(&log).expect("yt-dlp log");
    assert!(logged.contains("DIRECTID001"), "yt-dlp args: {logged}");
}
