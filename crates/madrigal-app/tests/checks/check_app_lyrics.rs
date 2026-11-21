use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use madrigal_app::lyrics::{self, EmbeddedFirstCache};
use madrigal_core::TrackIdentity;
use madrigal_testkit as kit;
use madrigal_ui::{
    lyrics_delivery, wire_shell_i18n, CachedLyrics, FetchTask, LyricsCache, LyricsFetcher,
    LyricsSource, MainShell,
};
use slint::Model;

type Reply = Result<Option<madrigal_lyrics::chain::ChainOutcome>, String>;

struct PendingSource {
    pending: Mutex<Sender<(String, Sender<Reply>)>>,
}

impl LyricsSource for PendingSource {
    fn fetch(&self, task: &FetchTask) -> Reply {
        let (respond, reply) = channel();
        self.pending
            .lock()
            .unwrap()
            .send((task.track.identity.id.clone(), respond))
            .map_err(|err| err.to_string())?;
        reply.recv().map_err(|err| err.to_string())?
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .join("madrigal-lyrics/tests/fixtures")
        .join(name)
}

fn plain_lyrics() -> madrigal_lyrics::Lyrics {
    let text = std::fs::read_to_string(fixture("plain.lrc")).expect("plain lrc fixture");
    madrigal_lyrics::lrc::parse(&text)
}

fn outcome(lyrics: &madrigal_lyrics::Lyrics) -> Reply {
    Ok(Some(madrigal_lyrics::chain::ChainOutcome {
        lyrics: lyrics.clone(),
        provider: "fake",
    }))
}

fn pump_until(timeout: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    loop {
        kit::mock_advance(Duration::from_millis(20));
        let _ = slint::quit_event_loop();
        slint::run_event_loop().unwrap();
        if done() {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for lyrics");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn await_fetch(shell: &MainShell, count: i32) {
    pump_until(Duration::from_secs(5), || shell.get_lyrics_fetch_count() >= count);
}

fn settle() {
    for _ in 0..10 {
        kit::mock_advance(Duration::from_millis(20));
        let _ = slint::quit_event_loop();
        slint::run_event_loop().unwrap();
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn wait_for_request(requests: &Receiver<(String, Sender<Reply>)>, id: &str) -> Sender<Reply> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok((sent, respond)) = requests.try_recv() {
            assert_eq!(sent, id);
            return respond;
        }
        assert!(Instant::now() < deadline, "no provider request for {id}");
        kit::mock_advance(Duration::from_millis(20));
        let _ = slint::quit_event_loop();
        slint::run_event_loop().unwrap();
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn set_track(shell: &MainShell, source: i32, id: &str, index: i32) {
    shell.set_track_id(id.into());
    shell.set_track_title("Titulo".into());
    shell.set_track_artist("Artista".into());
    shell.set_track_album("Album".into());
    shell.set_track_source(source);
    shell.set_duration_ms(180_000);
    shell.set_queue_current_index(index);
}

#[test]
fn app_lyrics() {
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, root.path());
    std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", root.path().join("cache"));

    let db = root.path().join("lyrics.sqlite3");
    let cache = EmbeddedFirstCache::new(&db);
    let uslt = fixture("embedded_uslt.mp3");
    let sylt = fixture("embedded_sylt.mp3");

    match cache.lookup(&TrackIdentity::local(uslt.to_string_lossy())) {
        Some(CachedLyrics::Hit(found)) => {
            assert!(!found.synced, "uslt lyrics reported as synced");
            assert_eq!(found.plain_text(), "First line\nSecond line");
        }
        other => panic!("uslt embedded lyrics missing: {other:?}"),
    }
    match cache.lookup(&TrackIdentity::local(sylt.to_string_lossy())) {
        Some(CachedLyrics::Hit(found)) => {
            assert!(found.synced, "sylt lyrics lost the timings");
            assert_eq!(found.lines.len(), 2);
            assert_eq!(found.lines[0].start_ms, 1_200);
            assert_eq!(found.lines[0].text, "Hola");
            assert_eq!(found.lines[1].start_ms, 3_400);
        }
        other => panic!("sylt embedded lyrics missing: {other:?}"),
    }

    let stored = TrackIdentity::local(fixture("words.ttml").to_string_lossy());
    assert!(
        cache.lookup(&stored).is_none(),
        "a lyric-less local file invented lyrics"
    );
    let seeded = plain_lyrics();
    let conn = madrigal_db::open(&db).expect("open lyrics db");
    madrigal_db::cache_lyrics(
        &conn,
        &stored,
        &madrigal_lyrics::chain::encode_cached(&seeded),
        1,
    )
    .expect("seed lyrics cache");
    drop(conn);
    match cache.lookup(&stored) {
        Some(CachedLyrics::Hit(found)) => assert_eq!(found.plain_text(), seeded.plain_text()),
        other => panic!("db lyrics cache not consulted: {other:?}"),
    }

    kit::init_headless();
    let shell = MainShell::new().expect("shell");
    wire_shell_i18n(&shell, madrigal_i18n::Lang::Es);

    let (pending, requests) = channel();
    let fetcher = std::rc::Rc::new(LyricsFetcher::new(
        Box::new(EmbeddedFirstCache::new(&db)),
        Box::new(PendingSource {
            pending: Mutex::new(pending),
        }),
        lyrics_delivery(&shell),
    ));
    lyrics::wire_with(&shell, fetcher);

    set_track(&shell, 2, &uslt.to_string_lossy(), 0);
    pump_until(Duration::from_secs(5), || shell.get_lyrics_state() == 2);
    assert_eq!(shell.get_lyric_rows().row_count(), 2);
    assert!(!shell.get_lyrics_synced());
    assert_eq!(shell.get_active_line(), -1, "plain lyrics got a highlight");
    assert_eq!(shell.get_lyrics_fetch_count(), 1);
    assert!(
        requests.try_recv().is_err(),
        "embedded lyrics hit the provider chain"
    );

    set_track(&shell, 2, &sylt.to_string_lossy(), 1);
    await_fetch(&shell, 2);
    pump_until(Duration::from_secs(5), || shell.get_lyrics_state() == 2);
    assert_eq!(shell.get_lyric_rows().row_count(), 2);
    assert!(shell.get_lyrics_synced());
    assert_eq!(shell.get_lyrics_fetch_count(), 2);
    assert!(
        requests.try_recv().is_err(),
        "embedded sylt lyrics hit the provider chain"
    );

    set_track(&shell, 0, "NrgmdOz227I", 2);
    await_fetch(&shell, 3);
    assert_eq!(shell.get_lyric_rows().row_count(), 0);
    assert_eq!(shell.get_lyrics_fetch_count(), 3);
    let respond = wait_for_request(&requests, "NrgmdOz227I");
    let chain_lyrics = plain_lyrics();
    respond.send(outcome(&chain_lyrics)).expect("respond chain");
    pump_until(Duration::from_secs(5), || shell.get_lyrics_state() == 2);
    assert!(shell.get_lyrics_synced());
    assert_eq!(
        shell.get_lyric_rows().row_count(),
        chain_lyrics.lines.len(),
        "chain lyrics not rendered"
    );

    set_track(&shell, 0, "missingTrack", 3);
    await_fetch(&shell, 4);
    let respond = wait_for_request(&requests, "missingTrack");
    respond.send(Ok(None)).expect("respond miss");
    pump_until(Duration::from_secs(5), || shell.get_lyrics_state() == 3);
    assert_eq!(shell.get_lyric_rows().row_count(), 0);
    assert_eq!(shell.get_lyrics_error().as_str(), "");

    set_track(&shell, 0, "brokenTrack", 4);
    await_fetch(&shell, 5);
    let respond = wait_for_request(&requests, "brokenTrack");
    respond.send(Err("provider timeout".to_owned())).expect("respond error");
    pump_until(Duration::from_secs(5), || shell.get_lyrics_state() == 4);
    assert_eq!(shell.get_lyrics_error().as_str(), "provider timeout");
    assert_eq!(shell.get_lyric_rows().row_count(), 0);

    let fetches = shell.get_lyrics_fetch_count();
    shell.invoke_lyrics_retry_requested();
    assert_eq!(shell.get_lyrics_retry_count(), 1);
    assert_eq!(shell.get_lyrics_state(), 1);
    assert_eq!(shell.get_lyrics_fetch_count(), fetches + 1);
    let respond = wait_for_request(&requests, "brokenTrack");
    respond.send(outcome(&chain_lyrics)).expect("respond retry");
    pump_until(Duration::from_secs(5), || shell.get_lyrics_state() == 2);
    assert!(shell.get_lyric_rows().row_count() > 0);

    set_track(&shell, 0, "staleTrack", 5);
    await_fetch(&shell, 7);
    let respond_stale = wait_for_request(&requests, "staleTrack");
    set_track(&shell, 0, "freshTrack", 6);
    await_fetch(&shell, 8);
    let respond_fresh = wait_for_request(&requests, "freshTrack");
    respond_stale
        .send(outcome(&madrigal_lyrics::lrc::parse("Estribillo repetido")))
        .expect("respond stale");
    settle();
    assert_eq!(shell.get_lyrics_state(), 1, "stale lyrics changed the state");
    assert_eq!(shell.get_lyric_rows().row_count(), 0, "stale lyrics rendered");
    respond_fresh.send(outcome(&chain_lyrics)).expect("respond fresh");
    pump_until(Duration::from_secs(5), || shell.get_lyrics_state() == 2);
    assert_eq!(shell.get_lyric_rows().row_count(), chain_lyrics.lines.len());

    println!("OK app_lyrics");
}
