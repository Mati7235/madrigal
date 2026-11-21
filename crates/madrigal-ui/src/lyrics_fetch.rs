use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use madrigal_core::{Track, TrackIdentity};
use madrigal_lyrics::chain::{self, ChainConfig, ChainOutcome};
use madrigal_lyrics::providers::Query;
use madrigal_lyrics::Lyrics;

pub const MISS_ENTRY: &str = "v1:miss";

#[derive(Debug)]
pub struct FetchTask {
    pub track: Track,
    pub query: Query,
    pub config: ChainConfig,
}

#[derive(Clone, Debug)]
pub enum CachedLyrics {
    Hit(Lyrics),
    Miss,
}

pub trait LyricsCache: Send + Sync {
    fn lookup(&self, identity: &TrackIdentity) -> Option<CachedLyrics>;
}

pub trait LyricsSource: Send + Sync {
    fn fetch(&self, task: &FetchTask) -> Result<Option<ChainOutcome>, String>;
}

#[derive(Clone, Debug)]
pub enum LyricsEvent {
    Found { request_id: u64, lyrics: Lyrics, provider: String },
    NotFound { request_id: u64 },
    Failed { request_id: u64, message: String },
}

impl LyricsEvent {
    pub fn request_id(&self) -> u64 {
        match self {
            LyricsEvent::Found { request_id, .. }
            | LyricsEvent::NotFound { request_id }
            | LyricsEvent::Failed { request_id, .. } => *request_id,
        }
    }
}

#[derive(Clone, Default)]
pub struct Generation(Arc<AtomicU64>);

impl Generation {
    pub fn current(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }

    pub fn is_current(&self, request_id: u64) -> bool {
        self.current() == request_id
    }

    fn begin(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn invalidate(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

pub struct LyricsFetcher {
    generation: Generation,
    cache: Arc<dyn LyricsCache>,
    source: Arc<dyn LyricsSource>,
    deliver: Arc<dyn Fn(LyricsEvent) + Send + Sync>,
}

impl LyricsFetcher {
    pub fn new(
        cache: Box<dyn LyricsCache>,
        source: Box<dyn LyricsSource>,
        deliver: Arc<dyn Fn(LyricsEvent) + Send + Sync>,
    ) -> Self {
        Self {
            generation: Generation::default(),
            cache: cache.into(),
            source: source.into(),
            deliver,
        }
    }

    pub fn generation(&self) -> Generation {
        self.generation.clone()
    }

    pub fn request(&self, track: Track, config: ChainConfig) -> u64 {
        let request_id = self.generation.begin();
        let generation = self.generation.clone();
        let cache = self.cache.clone();
        let source = self.source.clone();
        let deliver = self.deliver.clone();
        let task = FetchTask { query: chain::query_for(&track), track, config };
        std::thread::spawn(move || {
            let event = resolve(cache.as_ref(), source.as_ref(), request_id, &task);
            if generation.is_current(request_id) {
                deliver(event);
            }
        });
        request_id
    }

    pub fn cancel(&self) {
        self.generation.invalidate();
    }
}

fn resolve(
    cache: &dyn LyricsCache,
    source: &dyn LyricsSource,
    request_id: u64,
    task: &FetchTask,
) -> LyricsEvent {
    match cache.lookup(&task.track.identity) {
        Some(CachedLyrics::Hit(lyrics)) => LyricsEvent::Found {
            request_id,
            lyrics,
            provider: "cache".to_owned(),
        },
        Some(CachedLyrics::Miss) => LyricsEvent::NotFound { request_id },
        None => match source.fetch(task) {
            Ok(Some(outcome)) => LyricsEvent::Found {
                request_id,
                lyrics: outcome.lyrics,
                provider: outcome.provider.to_owned(),
            },
            Ok(None) => LyricsEvent::NotFound { request_id },
            Err(message) => LyricsEvent::Failed { request_id, message },
        },
    }
}

pub struct DbLyricsCache {
    path: PathBuf,
}

impl DbLyricsCache {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn default_profile() -> Self {
        Self::new(madrigal_paths::db_path())
    }
}

impl LyricsCache for DbLyricsCache {
    fn lookup(&self, identity: &TrackIdentity) -> Option<CachedLyrics> {
        let conn = madrigal_db::open(&self.path).ok()?;
        let raw = madrigal_db::fetch_lyrics(&conn, identity).ok()??;
        if raw == MISS_ENTRY {
            return Some(CachedLyrics::Miss);
        }
        chain::decode_cached(&raw).map(CachedLyrics::Hit)
    }
}

pub struct ChainSource {
    path: PathBuf,
}

impl ChainSource {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn default_profile() -> Self {
        Self::new(madrigal_paths::db_path())
    }
}

impl LyricsSource for ChainSource {
    fn fetch(&self, task: &FetchTask) -> Result<Option<ChainOutcome>, String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| err.to_string())?;
        let conn = madrigal_db::open(&self.path).map_err(|err| err.to_string())?;
        runtime
            .block_on(chain::fetch_track(
                &madrigal_lyrics::providers::http_client(),
                &conn,
                &task.track,
                &task.config,
            ))
            .map_err(|err| err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use madrigal_core::TrackIdentity;
    use madrigal_lyrics::{LyricLine, Lyrics};
    use madrigal_settings::LyricsProvider;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::Mutex;
    use std::time::Duration;

    const TIMEOUT: Duration = Duration::from_secs(5);

    struct NoCache;

    impl LyricsCache for NoCache {
        fn lookup(&self, _identity: &TrackIdentity) -> Option<CachedLyrics> {
            None
        }
    }

    struct FixedCache(CachedLyrics);

    impl LyricsCache for FixedCache {
        fn lookup(&self, _identity: &TrackIdentity) -> Option<CachedLyrics> {
            Some(self.0.clone())
        }
    }

    type Reply = Result<Option<ChainOutcome>, String>;
    type Pending = (String, Query, Sender<Reply>);

    struct PendingSource {
        pending: Mutex<Sender<Pending>>,
    }

    impl LyricsSource for PendingSource {
        fn fetch(&self, task: &FetchTask) -> Result<Option<ChainOutcome>, String> {
            let (respond, reply) = channel();
            self.pending
                .lock()
                .unwrap()
                .send((task.track.identity.id.clone(), task.query.clone(), respond))
                .map_err(|err| err.to_string())?;
            reply.recv().map_err(|err| err.to_string())?
        }
    }

    struct CountingSource(Arc<AtomicUsize>);

    impl LyricsSource for CountingSource {
        fn fetch(&self, _task: &FetchTask) -> Result<Option<ChainOutcome>, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }
    }

    fn config() -> ChainConfig {
        ChainConfig {
            order: vec![LyricsProvider::Lrclib],
            disabled: Vec::new(),
            paxsenix_api_key: String::new(),
            youtube_subtitle_enabled: false,
            bases: chain::EndpointBases::default(),
        }
    }

    fn track(id: &str) -> Track {
        Track {
            identity: TrackIdentity::youtube(id),
            title: format!("Song {id}"),
            artists: vec!["Artist".to_owned()],
            album: None,
            duration_secs: Some(120),
            thumbnail: None,
            track_number: None,
            local_path: None,
            set_video_id: None,
        }
    }

    fn lyrics(text: &str, synced: bool) -> Lyrics {
        Lyrics {
            lines: vec![LyricLine {
                start_ms: 0,
                end_ms: None,
                text: text.to_owned(),
                words: None,
                markers: Default::default(),
            }],
            synced,
        }
    }

    fn outcome(text: &str) -> ChainOutcome {
        ChainOutcome { lyrics: lyrics(text, true), provider: "fake" }
    }

    fn receiver() -> (Arc<dyn Fn(LyricsEvent) + Send + Sync>, Receiver<LyricsEvent>) {
        let (tx, rx) = channel();
        let deliver: Arc<dyn Fn(LyricsEvent) + Send + Sync> = Arc::new(move |event| {
            let _ = tx.send(event);
        });
        (deliver, rx)
    }

    #[test]
    fn stale_response_is_dropped() {
        let (pending_tx, pending_rx) = channel();
        let (deliver, events) = receiver();
        let fetcher = LyricsFetcher::new(
            Box::new(NoCache),
            Box::new(PendingSource { pending: Mutex::new(pending_tx) }),
            deliver,
        );
        let first = fetcher.request(track("A"), config());
        let (first_id, first_query, respond_first) =
            pending_rx.recv_timeout(TIMEOUT).expect("first request");
        assert_eq!(first_id, "A");
        assert_eq!(first_query.title, "Song A");
        assert_eq!(first_query.video_id.as_deref(), Some("A"));
        let second = fetcher.request(track("B"), config());
        assert!(second > first);
        let (second_id, second_query, respond_second) =
            pending_rx.recv_timeout(TIMEOUT).expect("second request");
        assert_eq!(second_id, "B");
        assert_eq!(second_query.video_id.as_deref(), Some("B"));
        respond_first.send(Ok(Some(outcome("stale")))).unwrap();
        assert!(
            events.recv_timeout(Duration::from_millis(500)).is_err(),
            "stale result reached the ui"
        );
        assert!(!fetcher.generation().is_current(first));
        assert!(fetcher.generation().is_current(second));
        respond_second.send(Ok(Some(outcome("fresh")))).unwrap();
        match events.recv_timeout(TIMEOUT).expect("fresh result") {
            LyricsEvent::Found { request_id, lyrics, provider } => {
                assert_eq!(request_id, second);
                assert_eq!(provider, "fake");
                assert_eq!(lyrics.plain_text(), "fresh");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn cached_hit_skips_source() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (deliver, events) = receiver();
        let fetcher = LyricsFetcher::new(
            Box::new(FixedCache(CachedLyrics::Hit(lyrics("cached", true)))),
            Box::new(CountingSource(calls.clone())),
            deliver,
        );
        let id = fetcher.request(track("A"), config());
        match events.recv_timeout(TIMEOUT).expect("cached result") {
            LyricsEvent::Found { request_id, lyrics, provider } => {
                assert_eq!(request_id, id);
                assert_eq!(provider, "cache");
                assert_eq!(lyrics.plain_text(), "cached");
            }
            other => panic!("unexpected event: {other:?}"),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0, "cache hit hit the network");
    }

    #[test]
    fn negative_cache_skips_source() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (deliver, events) = receiver();
        let fetcher = LyricsFetcher::new(
            Box::new(FixedCache(CachedLyrics::Miss)),
            Box::new(CountingSource(calls.clone())),
            deliver,
        );
        let id = fetcher.request(track("A"), config());
        match events.recv_timeout(TIMEOUT).expect("cache result") {
            LyricsEvent::NotFound { request_id } => assert_eq!(request_id, id),
            other => panic!("unexpected event: {other:?}"),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0, "negative cache hit the network");
    }

    #[test]
    fn source_error_is_reported() {
        let (pending_tx, pending_rx) = channel();
        let (deliver, events) = receiver();
        let fetcher = LyricsFetcher::new(
            Box::new(NoCache),
            Box::new(PendingSource { pending: Mutex::new(pending_tx) }),
            deliver,
        );
        let id = fetcher.request(track("A"), ChainConfig { order: Vec::new(), ..config() });
        let (_, _, respond) = pending_rx.recv_timeout(TIMEOUT).expect("request");
        respond.send(Err("db offline".to_owned())).unwrap();
        match events.recv_timeout(TIMEOUT).expect("error result") {
            LyricsEvent::Failed { request_id, message } => {
                assert_eq!(request_id, id);
                assert_eq!(message, "db offline");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn cancel_drops_inflight_result() {
        let (pending_tx, pending_rx) = channel();
        let (deliver, events) = receiver();
        let fetcher = LyricsFetcher::new(
            Box::new(NoCache),
            Box::new(PendingSource { pending: Mutex::new(pending_tx) }),
            deliver,
        );
        fetcher.request(track("A"), config());
        let (_, _, respond) = pending_rx.recv_timeout(TIMEOUT).expect("request");
        fetcher.cancel();
        respond.send(Ok(Some(outcome("late")))).unwrap();
        assert!(events.recv_timeout(Duration::from_millis(500)).is_err());
    }

    #[test]
    fn db_cache_reads_hits_and_misses() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lyrics.sqlite3");
        let conn = madrigal_db::open(&path).unwrap();
        let identity = TrackIdentity::youtube("CACHED123");
        let cache = DbLyricsCache::new(&path);
        assert!(cache.lookup(&identity).is_none());
        madrigal_db::cache_lyrics(&conn, &identity, &chain::encode_cached(&lyrics("hello", true)), 1)
            .unwrap();
        match cache.lookup(&identity).expect("hit") {
            CachedLyrics::Hit(found) => {
                assert!(found.synced);
                assert_eq!(found.plain_text(), "hello");
            }
            CachedLyrics::Miss => panic!("expected hit"),
        }
        madrigal_db::cache_lyrics(&conn, &identity, MISS_ENTRY, 2).unwrap();
        assert!(matches!(cache.lookup(&identity), Some(CachedLyrics::Miss)));
        drop(conn);
    }
}
