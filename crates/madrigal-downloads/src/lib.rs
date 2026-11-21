mod postprocess;

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Condvar, Mutex,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use madrigal_core::{Track, TrackIdentity};
use madrigal_settings::Storage;

pub use postprocess::{cut_segments, probe_duration, read_chapters, Chapter, PostprocessError};

const MAX_WORKERS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DownloadState {
    Queued,
    Downloading,
    Paused,
    Completed,
    Failed,
    Quarantined,
}

impl DownloadState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Quarantined => "quarantined",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "queued" => Some(Self::Queued),
            "downloading" => Some(Self::Downloading),
            "paused" => Some(Self::Paused),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "quarantined" => Some(Self::Quarantined),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProgressEvent {
    pub key: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bps: Option<f64>,
    pub percent: f64,
}

#[derive(Debug)]
pub enum RunnerError {
    Cancelled,
    Paused,
    Failed(String),
}

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Paused => f.write_str("paused"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

pub struct RunContext {
    pub identity: TrackIdentity,
    pub output: PathBuf,
    pub quality: Option<String>,
    pub ytdlp_bin: PathBuf,
    paused: std::sync::Arc<AtomicBool>,
    cancelled: std::sync::Arc<AtomicBool>,
}

impl RunContext {
    pub fn new(
        identity: TrackIdentity,
        output: PathBuf,
        quality: Option<String>,
        ytdlp_bin: PathBuf,
    ) -> Self {
        Self::shared(
            identity,
            output,
            quality,
            ytdlp_bin,
            std::sync::Arc::new(AtomicBool::new(false)),
            std::sync::Arc::new(AtomicBool::new(false)),
        )
    }

    pub fn shared(
        identity: TrackIdentity,
        output: PathBuf,
        quality: Option<String>,
        ytdlp_bin: PathBuf,
        paused: std::sync::Arc<AtomicBool>,
        cancelled: std::sync::Arc<AtomicBool>,
    ) -> Self {
        Self {
            identity,
            output,
            quality,
            ytdlp_bin,
            paused,
            cancelled,
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
}

pub trait DownloadRunner: Send + Sync + 'static {
    fn run(
        &self,
        ctx: &RunContext,
        report: &dyn Fn(ProgressEvent),
    ) -> Result<PathBuf, RunnerError>;
}

#[derive(Debug, Clone, Default)]
pub struct JobOptions {
    pub quality: Option<String>,
    pub segments: Vec<(f64, f64)>,
    pub chapters: Vec<Chapter>,
    pub postprocess: bool,
}

#[derive(Debug, Clone)]
pub struct ManagerConfig {
    pub downloads_dir: PathBuf,
    pub db_path: PathBuf,
    pub concurrency: usize,
    pub max_retries: u32,
    pub ytdlp_bin: PathBuf,
    pub ffmpeg_bin: PathBuf,
    pub ffprobe_bin: PathBuf,
}

impl ManagerConfig {
    pub fn with_storage(downloads_dir: PathBuf, db_path: PathBuf, storage: &Storage) -> Self {
        Self {
            downloads_dir,
            db_path,
            concurrency: storage.download_concurrency.clamp(1, MAX_WORKERS as i32) as usize,
            max_retries: 2,
            ytdlp_bin: PathBuf::from("yt-dlp"),
            ffmpeg_bin: PathBuf::from("ffmpeg"),
            ffprobe_bin: PathBuf::from("ffprobe"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DownloadRecord {
    pub source: String,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub artists: Vec<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub duration_secs: Option<u32>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub track_number: Option<u32>,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub state: String,
}

impl DownloadRecord {
    pub fn identity(&self) -> TrackIdentity {
        match self.source.as_str() {
            "tidal" => TrackIdentity::tidal(&self.id),
            "local" => TrackIdentity::local(&self.id),
            _ => TrackIdentity::youtube(&self.id),
        }
    }

    pub fn to_track(&self) -> Track {
        Track {
            identity: self.identity(),
            title: self.title.clone(),
            artists: self.artists.clone(),
            album: self.album.clone(),
            duration_secs: self.duration_secs,
            thumbnail: self.thumbnail.clone(),
            track_number: self.track_number,
            local_path: if self.path.is_empty() {
                None
            } else {
                Some(self.path.clone())
            },
            set_video_id: None,
        }
    }

    pub fn of(track: &Track, path: &Path, state: DownloadState) -> Self {
        let source = match track.identity.source {
            madrigal_core::TrackSource::Tidal => "tidal",
            madrigal_core::TrackSource::Local => "local",
            _ => "youtube",
        }
        .to_string();
        Self {
            source,
            id: track.identity.id.clone(),
            title: track.title.clone(),
            artists: track.artists.clone(),
            album: track.album.clone(),
            duration_secs: track.duration_secs,
            thumbnail: track.thumbnail.clone(),
            track_number: track.track_number,
            path: path.to_string_lossy().into_owned(),
            state: state.as_str().to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RestoreOptions {
    pub redownload_missing: bool,
}

struct JobRecord {
    track: Track,
    options: JobOptions,
    state: DownloadState,
    attempts: u32,
    error: Option<String>,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    speed_bps: Option<f64>,
    output: PathBuf,
    paused: std::sync::Arc<AtomicBool>,
    cancelled: std::sync::Arc<AtomicBool>,
}

struct Inner<R> {
    runner: R,
    config: Mutex<ManagerConfig>,
    pending: Mutex<VecDeque<String>>,
    cvar: Condvar,
    jobs: Mutex<HashMap<String, JobRecord>>,
    subscribers: Mutex<Vec<mpsc::Sender<ProgressEvent>>>,
    active: Mutex<usize>,
    registry: Mutex<()>,
    shutdown: AtomicBool,
}

pub struct DownloadManager<R: DownloadRunner> {
    inner: std::sync::Arc<Inner<R>>,
    handles: Vec<thread::JoinHandle<()>>,
}

impl<R: DownloadRunner> DownloadManager<R> {
    pub fn new(config: ManagerConfig, runner: R) -> Self {
        let _ = fs::create_dir_all(&config.downloads_dir);
        let inner = std::sync::Arc::new(Inner {
            runner,
            config: Mutex::new(config),
            pending: Mutex::new(VecDeque::new()),
            cvar: Condvar::new(),
            jobs: Mutex::new(HashMap::new()),
            subscribers: Mutex::new(Vec::new()),
            active: Mutex::new(0),
            registry: Mutex::new(()),
            shutdown: AtomicBool::new(false),
        });
        let mut handles = Vec::new();
        for _ in 0..MAX_WORKERS {
            let worker = std::sync::Arc::clone(&inner);
            handles.push(thread::spawn(move || worker_loop(worker)));
        }
        Self { inner, handles }
    }

    pub fn subscribe(&self) -> mpsc::Receiver<ProgressEvent> {
        let (tx, rx) = mpsc::channel();
        self.inner.subscribers.lock().unwrap().push(tx);
        rx
    }

    pub fn enqueue(&self, track: Track, options: JobOptions) -> bool {
        let key = track.identity.to_storage_string();
        let output = output_path(
            &self.inner.config.lock().unwrap().downloads_dir,
            &track,
        );
        let mut jobs = self.inner.jobs.lock().unwrap();
        if let Some(existing) = jobs.get(&key) {
            match existing.state {
                DownloadState::Queued | DownloadState::Downloading | DownloadState::Paused => {
                    return false;
                }
                DownloadState::Completed
                | DownloadState::Failed
                | DownloadState::Quarantined => {}
            }
        }
        jobs.insert(
            key.clone(),
            JobRecord {
                track,
                options,
                state: DownloadState::Queued,
                attempts: 0,
                error: None,
                downloaded_bytes: 0,
                total_bytes: None,
                speed_bps: None,
                output,
                paused: std::sync::Arc::new(AtomicBool::new(false)),
                cancelled: std::sync::Arc::new(AtomicBool::new(false)),
            },
        );
        drop(jobs);
        let mut pending = self.inner.pending.lock().unwrap();
        if !pending.contains(&key) {
            pending.push_back(key);
        }
        self.inner.cvar.notify_one();
        true
    }

    pub fn enqueue_on_like(&self, track: Track, liked: bool, auto_download: bool) -> bool {
        if !liked || !auto_download {
            return false;
        }
        self.enqueue(track, JobOptions::default())
    }

    pub fn pause(&self, identity: &TrackIdentity) {
        let key = identity.to_storage_string();
        let mut jobs = self.inner.jobs.lock().unwrap();
        let Some(record) = jobs.get_mut(&key) else {
            return;
        };
        record.paused.store(true, Ordering::SeqCst);
        match record.state {
            DownloadState::Queued => {
                record.state = DownloadState::Paused;
                self.inner
                    .pending
                    .lock()
                    .unwrap()
                    .retain(|item| item != &key);
                self.inner.cvar.notify_all();
            }
            DownloadState::Downloading => {
                record.state = DownloadState::Paused;
                self.inner.cvar.notify_all();
            }
            _ => {}
        }
    }

    pub fn resume(&self, identity: &TrackIdentity) {
        let key = identity.to_storage_string();
        let mut jobs = self.inner.jobs.lock().unwrap();
        let Some(record) = jobs.get_mut(&key) else {
            return;
        };
        if record.state != DownloadState::Paused {
            return;
        }
        record.paused.store(false, Ordering::SeqCst);
        record.cancelled.store(false, Ordering::SeqCst);
        record.state = DownloadState::Queued;
        drop(jobs);
        let mut pending = self.inner.pending.lock().unwrap();
        if !pending.contains(&key) {
            pending.push_back(key);
        }
        self.inner.cvar.notify_one();
    }

    pub fn cancel(&self, identity: &TrackIdentity) {
        let key = identity.to_storage_string();
        let mut jobs = self.inner.jobs.lock().unwrap();
        let Some(record) = jobs.get_mut(&key) else {
            return;
        };
        record.cancelled.store(true, Ordering::SeqCst);
        record.paused.store(false, Ordering::SeqCst);
        self.inner
            .pending
            .lock()
            .unwrap()
            .retain(|item| item != &key);
        if !matches!(record.state, DownloadState::Downloading) {
            let output = record.output.clone();
            jobs.remove(&key);
            drop(jobs);
            remove_contained(
                &self.inner.config.lock().unwrap().downloads_dir,
                &output,
            );
        }
        self.inner.cvar.notify_all();
    }

    pub fn retry(&self, identity: &TrackIdentity) {
        let key = identity.to_storage_string();
        let mut jobs = self.inner.jobs.lock().unwrap();
        let Some(record) = jobs.get_mut(&key) else {
            return;
        };
        if !matches!(
            record.state,
            DownloadState::Failed | DownloadState::Quarantined
        ) {
            return;
        }
        record.paused.store(false, Ordering::SeqCst);
        record.cancelled.store(false, Ordering::SeqCst);
        record.error = None;
        record.state = DownloadState::Queued;
        drop(jobs);
        let mut pending = self.inner.pending.lock().unwrap();
        if !pending.contains(&key) {
            pending.push_back(key);
        }
        self.inner.cvar.notify_one();
    }

    pub fn state(&self, identity: &TrackIdentity) -> Option<DownloadState> {
        self.inner
            .jobs
            .lock()
            .unwrap()
            .get(&identity.to_storage_string())
            .map(|record| record.state)
    }

    pub fn attempts(&self, identity: &TrackIdentity) -> Option<u32> {
        self.inner
            .jobs
            .lock()
            .unwrap()
            .get(&identity.to_storage_string())
            .map(|record| record.attempts)
    }

    pub fn error(&self, identity: &TrackIdentity) -> Option<String> {
        self.inner
            .jobs
            .lock()
            .unwrap()
            .get(&identity.to_storage_string())
            .and_then(|record| record.error.clone())
    }

    pub fn progress_of(
        &self,
        identity: &TrackIdentity,
    ) -> Option<(u64, Option<u64>, Option<f64>)> {
        self.inner
            .jobs
            .lock()
            .unwrap()
            .get(&identity.to_storage_string())
            .map(|record| {
                (
                    record.downloaded_bytes,
                    record.total_bytes,
                    record.speed_bps,
                )
            })
    }

    pub fn output_of(&self, identity: &TrackIdentity) -> Option<PathBuf> {
        self.inner
            .jobs
            .lock()
            .unwrap()
            .get(&identity.to_storage_string())
            .map(|record| record.output.clone())
    }

    pub fn snapshot(&self) -> Vec<(TrackIdentity, DownloadState)> {
        self.inner
            .jobs
            .lock()
            .unwrap()
            .values()
            .map(|record| (record.track.identity.clone(), record.state))
            .collect()
    }

    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            let busy = self.inner.jobs.lock().unwrap().values().any(|record| {
                matches!(
                    record.state,
                    DownloadState::Queued | DownloadState::Downloading
                )
            });
            let pending = self.inner.pending.lock().unwrap().len();
            if !busy && pending == 0 {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn delete(&self, identity: &TrackIdentity) -> bool {
        let key = identity.to_storage_string();
        let output = self
            .inner
            .jobs
            .lock()
            .unwrap()
            .remove(&key)
            .map(|record| record.output);
        let downloads_dir = self.inner.config.lock().unwrap().downloads_dir.clone();
        let db_path = self.inner.config.lock().unwrap().db_path.clone();
        if let Some(path) = output {
            remove_contained(&downloads_dir, &path);
        }
        let mut removed = false;
        {
            let _guard = self.inner.registry.lock().unwrap();
            if let Ok(conn) = madrigal_db::open(&db_path) {
                let _ = conn.busy_timeout(Duration::from_secs(5));
                if let Ok(count) = conn.execute(
                    "DELETE FROM downloads WHERE song_key = ?1",
                    rusqlite::params![key],
                ) {
                    removed = count > 0;
                }
            }
        }
        removed
    }

    pub fn list_registry(&self) -> Vec<DownloadRecord> {
        let db_path = self.inner.config.lock().unwrap().db_path.clone();
        let _guard = self.inner.registry.lock().unwrap();
        let Ok(conn) = madrigal_db::open(&db_path) else {
            return Vec::new();
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT song_key, path, title, artists, album, duration_secs, thumbnail FROM downloads ORDER BY created_at DESC",
        ) else {
            return Vec::new();
        };
        stmt.query_map([], |row| {
            let key: String = row.get(0)?;
            let path: String = row.get(1)?;
            let title: String = row.get(2)?;
            let artists: String = row.get(3)?;
            let album: Option<String> = row.get(4)?;
            let duration: Option<i64> = row.get(5)?;
            let thumbnail: Option<String> = row.get(6)?;
            Ok((key, path, title, artists, album, duration, thumbnail))
        })
        .map(|rows| {
            rows
                .flatten()
                .map(|(key, path, title, artists, album, duration, thumbnail)| {
                    let identity = TrackIdentity::parse_prefixed(&key);
                    let (source, id) = match identity.source {
                        madrigal_core::TrackSource::Tidal => ("tidal", identity.id.clone()),
                        madrigal_core::TrackSource::Local => ("local", identity.id.clone()),
                        _ => ("youtube", identity.id.clone()),
                    };
                    DownloadRecord {
                        source: source.to_string(),
                        id,
                        title,
                        artists: serde_json::from_str(&artists).unwrap_or_default(),
                        album,
                        duration_secs: duration.and_then(|v| u32::try_from(v).ok()),
                        thumbnail,
                        track_number: None,
                        path,
                        state: DownloadState::Completed.as_str().to_string(),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
    }

    pub fn export_list(&self, dest: &Path) -> io::Result<()> {
        let records: Vec<DownloadRecord> = self
            .inner
            .jobs
            .lock()
            .unwrap()
            .values()
            .map(|record| DownloadRecord::of(&record.track, &record.output, record.state))
            .collect();
        let text = serde_json::to_string_pretty(&records)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        madrigal_files::atomic_write_text(dest, &text)?;
        Ok(())
    }

    pub fn import_list(&self, src: &Path) -> io::Result<Vec<DownloadRecord>> {
        let text = fs::read_to_string(src)?;
        serde_json::from_str(&text).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }

    pub fn restore(&self, records: &[DownloadRecord], options: RestoreOptions) -> usize {
        let mut enqueued = 0;
        for record in records {
            let path = PathBuf::from(&record.path);
            if path.exists() {
                let mut jobs = self.inner.jobs.lock().unwrap();
                let key = record.identity().to_storage_string();
                let mut track = record.to_track();
                track.local_path = Some(record.path.clone());
                let output =
                    output_path(&self.inner.config.lock().unwrap().downloads_dir, &track);
                jobs.entry(key).or_insert(JobRecord {
                    track,
                    options: JobOptions::default(),
                    state: DownloadState::Completed,
                    attempts: 0,
                    error: None,
                    downloaded_bytes: 0,
                    total_bytes: None,
                    speed_bps: None,
                    output,
                    paused: std::sync::Arc::new(AtomicBool::new(false)),
                    cancelled: std::sync::Arc::new(AtomicBool::new(false)),
                });
            } else if options.redownload_missing
                && self.enqueue(record.to_track(), JobOptions::default())
            {
                enqueued += 1;
            }
        }
        enqueued
    }

    pub fn shutdown(mut self) {
        self.inner.shutdown.store(true, Ordering::SeqCst);
        self.inner.cvar.notify_all();
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

fn worker_loop<R: DownloadRunner>(inner: std::sync::Arc<Inner<R>>) {
    loop {
        let key = {
            let mut pending = inner.pending.lock().unwrap();
            while pending.is_empty() && !inner.shutdown.load(Ordering::SeqCst) {
                pending = inner.cvar.wait(pending).unwrap();
            }
            if inner.shutdown.load(Ordering::SeqCst) && pending.is_empty() {
                return;
            }
            match pending.pop_front() {
                Some(key) => key,
                None => continue,
            }
        };
        {
            let config = inner.config.lock().unwrap();
            let limit = config.concurrency.clamp(1, MAX_WORKERS);
            drop(config);
            loop {
                if inner.shutdown.load(Ordering::SeqCst) {
                    let mut pending = inner.pending.lock().unwrap();
                    pending.push_front(key.clone());
                    inner.cvar.notify_one();
                    return;
                }
                let mut active = inner.active.lock().unwrap();
                if *active < limit {
                    *active += 1;
                    break;
                }
                drop(active);
                thread::sleep(Duration::from_millis(10));
            }
        }
        run_job(&inner, &key);
        *inner.active.lock().unwrap() -= 1;
    }
}

fn run_job<R: DownloadRunner>(inner: &std::sync::Arc<Inner<R>>, key: &str) {
    let prepared = {
        let mut jobs = inner.jobs.lock().unwrap();
        let Some(record) = jobs.get_mut(key) else {
            return;
        };
        if record.state == DownloadState::Paused {
            return;
        }
        record.state = DownloadState::Downloading;
        record.downloaded_bytes = 0;
        (
            record.track.clone(),
            record.options.clone(),
            record.output.clone(),
            std::sync::Arc::clone(&record.paused),
            std::sync::Arc::clone(&record.cancelled),
            inner.config.lock().unwrap().clone(),
        )
    };
    let (track, options, output, paused, cancelled, config) = prepared;
    if let Some(parent) = output.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let identity = track.identity.clone();
    let ctx = RunContext::shared(
        identity.clone(),
        output.clone(),
        options.quality.clone(),
        config.ytdlp_bin.clone(),
        paused,
        cancelled,
    );
    let key_owned = key.to_string();
    let jobs_ref = std::sync::Arc::clone(inner);
    let result = inner.runner.run(
        &ctx,
        &|event: ProgressEvent| {
            let mut jobs = jobs_ref.jobs.lock().unwrap();
            if let Some(record) = jobs.get_mut(&key_owned) {
                record.downloaded_bytes = event.downloaded_bytes;
                record.total_bytes = event.total_bytes;
                record.speed_bps = event.speed_bps;
            }
            emit(&jobs_ref, event);
        },
    );
    match result {
        Ok(path) => {
            if ctx.is_cancelled() {
                remove_contained(&config.downloads_dir, &path);
                inner.jobs.lock().unwrap().remove(key);
            } else if ctx.is_paused() {
                set_state(inner, key, DownloadState::Paused, None);
            } else {
                finish_download(inner, key, &track, &path, &options, &config);
            }
        }
        Err(RunnerError::Cancelled) => {
            remove_contained(&config.downloads_dir, &output);
            inner.jobs.lock().unwrap().remove(key);
        }
        Err(RunnerError::Paused) => {
            set_state(inner, key, DownloadState::Paused, None);
        }
        Err(RunnerError::Failed(message)) => {
            let mut jobs = inner.jobs.lock().unwrap();
            let Some(record) = jobs.get_mut(key) else {
                return;
            };
            record.attempts += 1;
            record.error = Some(message);
            if record.attempts > config.max_retries {
                record.state = DownloadState::Quarantined;
                let stale = record.output.clone();
                drop(jobs);
                quarantine_file(&config.downloads_dir, &stale);
                remove_contained(&config.downloads_dir, &stale);
            } else {
                record.state = DownloadState::Failed;
            }
        }
    }
    inner.cvar.notify_all();
}

fn finish_download<R: DownloadRunner>(
    inner: &std::sync::Arc<Inner<R>>,
    key: &str,
    track: &Track,
    path: &Path,
    options: &JobOptions,
    config: &ManagerConfig,
) {
    let mut final_path = path.to_path_buf();
    if options.postprocess && !options.segments.is_empty() {
        match cut_segments(
            path,
            &options.segments,
            &options.chapters,
            &config.ffmpeg_bin,
            &config.ffprobe_bin,
        ) {
            Ok(cut) => {
                if fs::rename(&cut, path).is_ok() {
                    final_path = path.to_path_buf();
                } else {
                    final_path = cut;
                }
            }
            Err(err) => {
                quarantine_file(&config.downloads_dir, path);
                set_state(
                    inner,
                    key,
                    DownloadState::Quarantined,
                    Some(err.to_string()),
                );
                return;
            }
        }
    }
    let size = fs::metadata(&final_path).map(|meta| meta.len()).ok();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let registry_result = {
        let _guard = inner.registry.lock().unwrap();
        match madrigal_db::open(&config.db_path) {
            Ok(conn) => {
                let _ = conn.busy_timeout(Duration::from_secs(5));
                madrigal_db::upsert_download(&conn, &track.identity, &final_path, track, size, now)
                    .map_err(|err| format!("registry: {err}"))
            }
            Err(err) => Err(format!("registry: {err}")),
        }
    };
    if let Err(message) = registry_result {
        set_state(inner, key, DownloadState::Failed, Some(message));
        return;
    }
    let mut jobs = inner.jobs.lock().unwrap();
    if let Some(record) = jobs.get_mut(key) {
        record.state = DownloadState::Completed;
        record.output = final_path;
        record.error = None;
    }
}

fn set_state<R>(
    inner: &std::sync::Arc<Inner<R>>,
    key: &str,
    state: DownloadState,
    error: Option<String>,
) {
    let mut jobs = inner.jobs.lock().unwrap();
    if let Some(record) = jobs.get_mut(key) {
        record.state = state;
        if error.is_some() {
            record.error = error;
        }
    }
}

fn emit<R>(inner: &std::sync::Arc<Inner<R>>, event: ProgressEvent) {
    let mut subscribers = inner.subscribers.lock().unwrap();
    subscribers.retain(|tx| tx.send(event.clone()).is_ok());
}

fn remove_contained(root: &Path, target: &Path) {
    let Ok(resolved) = target.canonicalize() else {
        let _ = fs::remove_file(target);
        return;
    };
    let Ok(root_resolved) = root.canonicalize() else {
        return;
    };
    if resolved != root_resolved && !resolved.starts_with(&root_resolved) {
        return;
    }
    if let Ok(meta) = fs::symlink_metadata(&resolved) {
        if !meta.file_type().is_symlink() && meta.is_file() {
            let _ = fs::remove_file(&resolved);
        }
    }
}

fn quarantine_file(root: &Path, path: &Path) {
    let target_dir = root.join("quarantine");
    if fs::create_dir_all(&target_dir).is_err() {
        return;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download");
    let dest = target_dir.join(format!(
        "{name}.{}.q",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = fs::rename(path, dest);
}

fn sanitize_component(raw: &str, fallback: &str) -> String {
    let mut cleaned = String::new();
    for ch in raw.chars() {
        if ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            cleaned.push('_');
        } else {
            cleaned.push(ch);
        }
    }
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return fallback.to_string();
    }
    cleaned
}

pub fn output_path(base: &Path, track: &Track) -> PathBuf {
    let artist = track
        .artists
        .first()
        .map(|a| sanitize_component(a, "Unknown Artist"))
        .unwrap_or_else(|| "Unknown Artist".to_string());
    let album = track
        .album
        .as_deref()
        .map(|a| sanitize_component(a, "Unknown Album"))
        .unwrap_or_else(|| "Unknown Album".to_string());
    let prefix = track
        .track_number
        .map(|n| format!("{n:02} - "))
        .unwrap_or_default();
    let title = sanitize_component(&track.title, "Unknown Title");
    base.join(artist)
        .join(album)
        .join(format!("{prefix}{title} [{}].opus", track.identity.id))
}

pub fn sanitize_clamp_concurrency(value: i32) -> usize {
    value.clamp(1, MAX_WORKERS as i32) as usize
}

pub struct YtDlpRunner {
    pub format: String,
}

impl YtDlpRunner {
    pub fn new() -> Self {
        Self {
            format: "bestaudio/best".to_string(),
        }
    }

    pub fn with_format(format: &str) -> Self {
        Self {
            format: format.to_string(),
        }
    }
}

impl Default for YtDlpRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadRunner for YtDlpRunner {
    fn run(
        &self,
        ctx: &RunContext,
        report: &dyn Fn(ProgressEvent),
    ) -> Result<PathBuf, RunnerError> {
        if ctx.is_cancelled() {
            return Err(RunnerError::Cancelled);
        }
        if ctx.is_paused() {
            return Err(RunnerError::Paused);
        }
        let stem = ctx
            .output
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("audio");
        let parent = ctx
            .output
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        if fs::create_dir_all(&parent).is_err() {
            return Err(RunnerError::Failed("cannot create output dir".to_string()));
        }
        let template = parent.join(format!("{stem}.%(ext)s"));
        let url = match ctx.identity.source {
            madrigal_core::TrackSource::Youtube => {
                format!("https://music.youtube.com/watch?v={}", ctx.identity.id)
            }
            _ => ctx.identity.id.clone(),
        };
        let quality = ctx.quality.clone().unwrap_or_else(|| self.format.clone());
        let mut child = Command::new(&ctx.ytdlp_bin)
            .args([
                "--newline",
                "--no-playlist",
                "--extract-audio",
                "--audio-format",
                "opus",
                "--format",
                &quality,
                "-o",
            ])
            .arg(&template)
            .arg(&url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| RunnerError::Failed(format!("spawn yt-dlp: {err}")))?;
        let stdout = child.stdout.take();
        if let Some(out) = stdout {
            use std::io::{BufRead, BufReader};
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if ctx.is_cancelled() {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(RunnerError::Cancelled);
                }
                if ctx.is_paused() {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(RunnerError::Paused);
                }
                if let Some(event) = parse_progress_line(&line, &ctx.identity) {
                    report(event);
                }
            }
        }
        let status = child
            .wait()
            .map_err(|err| RunnerError::Failed(format!("wait yt-dlp: {err}")))?;
        if ctx.is_cancelled() {
            return Err(RunnerError::Cancelled);
        }
        if ctx.is_paused() {
            return Err(RunnerError::Paused);
        }
        if !status.success() {
            return Err(RunnerError::Failed(format!(
                "yt-dlp exited with {status}"
            )));
        }
        if ctx.output.exists() {
            return Ok(ctx.output.clone());
        }
        Err(RunnerError::Failed("yt-dlp produced no output".to_string()))
    }
}

fn parse_size(raw: &str) -> Option<f64> {
    let text = raw.trim().trim_start_matches('~').trim().to_string();
    let cut = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ' '))
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(cut);
    let value: f64 = number.trim().parse().ok()?;
    let factor = match unit.trim() {
        "B" | "B/s" => 1.0,
        "KiB" | "KB" | "KiB/s" | "KB/s" => 1024.0,
        "MiB" | "MB" | "MiB/s" | "MB/s" => 1024.0 * 1024.0,
        "GiB" | "GB" | "GiB/s" | "GB/s" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some(value * factor)
}

pub fn parse_progress_line(line: &str, identity: &TrackIdentity) -> Option<ProgressEvent> {
    let body = line.strip_prefix("[download]")?.trim();
    if body.starts_with("Destination:") {
        return None;
    }
    let percent_pos = body.find('%')?;
    let before = body[..percent_pos].trim();
    let percent: f64 = before.rsplit(' ').next()?.parse().ok()?;
    let mut total = None;
    let mut speed = None;
    let tokens: Vec<&str> = body.split_whitespace().collect();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index] == "of" && index + 1 < tokens.len() {
            let candidate = if tokens[index + 1] == "~" && index + 2 < tokens.len() {
                tokens[index + 2]
            } else {
                tokens[index + 1]
            };
            total = parse_size(candidate);
        }
        if tokens[index] == "at" && index + 1 < tokens.len() {
            speed = parse_size(tokens[index + 1]);
        }
        index += 1;
    }
    let downloaded = total.map(|t| (t * percent / 100.0) as u64).unwrap_or(0);
    Some(ProgressEvent {
        key: identity.to_storage_string(),
        downloaded_bytes: downloaded,
        total_bytes: total.map(|t| t as u64),
        speed_bps: speed,
        percent,
    })
}

pub fn default_downloads_dir() -> PathBuf {
    madrigal_paths::music_dir().join("Madrigal")
}
