use std::env;
use std::fs;

use madrigal_streams::cache::now_epoch;
use madrigal_streams::{AudioStream, StreamCache};

fn sample(video_id: &str, expires_at: Option<f64>) -> AudioStream {
    AudioStream {
        video_id: video_id.to_string(),
        url: "https://rr1---fake.googlevideo.com/videoplayback?expire=9999999999".to_string(),
        user_agent: Some("fake-agent/1.0".to_string()),
        title: Some("Fake Title".to_string()),
        duration: Some(200.0),
        ext: Some("m4a".to_string()),
        acodec: Some("mp4a.40.2".to_string()),
        abr: Some(128.0),
        client: "default".to_string(),
        expires_at,
        resolution_path: "default".to_string(),
    }
}

#[test]
fn cache_honors_ttl_prunes_and_writes_atomically() {
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    let dir = root.path().join("streams");
    let cache = StreamCache::with_dir(&dir, 60.0, 300.0);
    let now = now_epoch();

    let fresh = sample("VALIDID0001", Some(now + 3600.0));
    assert!(cache.store("VALIDID0001", None, &fresh, now).expect("store"));
    assert_eq!(
        cache.load("VALIDID0001", None, now).expect("hit").url,
        fresh.url
    );

    let stale = sample("STALEID0002", Some(now - 10.0));
    assert!(!cache.store("STALEID0002", None, &stale, now).expect("store"));
    assert!(cache.load("STALEID0002", None, now).is_none());

    let default_ttl = sample("NOTTLID0003", None);
    assert!(cache.store("NOTTLID0003", None, &default_ttl, now).expect("store"));
    assert!(cache.load("NOTTLID0003", None, now).is_some());
    assert!(cache.load("NOTTLID0003", None, now + 301.0).is_none());

    let raw = serde_json::json!({
        "stream": sample("OLDID000004", Some(now + 3600.0)),
        "expires_at": now - 5.0,
    });
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("OLDID000004__default.json"),
        serde_json::to_vec(&raw).unwrap(),
    )
    .unwrap();
    fs::write(dir.join("garbage.json"), b"not json").unwrap();

    let removed = cache.prune(now);
    assert_eq!(removed, 2);
    assert!(cache.load("VALIDID0001", None, now).is_some());
    assert!(cache.load("NOTTLID0003", None, now).is_some());
    assert!(!dir.join("OLDID000004__default.json").exists());
    assert!(!dir.join("garbage.json").exists());

    let leftovers: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        leftovers.iter().all(|n| !n.ends_with(".tmp") && !n.starts_with('.')),
        "no partial files: {leftovers:?}"
    );
    let stored: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("VALIDID0001__default.json")).unwrap()).unwrap();
    assert_eq!(stored["stream"]["video_id"], "VALIDID0001");

    let other = sample("VALIDID0001", Some(now + 3600.0));
    assert!(cache.store("VALIDID0001", Some("bestaudio/best"), &other, now).expect("store"));
    assert!(dir.join("VALIDID0001__bestaudio_best.json").exists());
    assert!(cache.load("VALIDID0001", Some("bestaudio/best"), now).is_some());
}
