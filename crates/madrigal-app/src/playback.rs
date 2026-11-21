use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use madrigal_core::{Track, TrackIdentity, TrackSource};
use madrigal_player::engine::{Engine, EngineConfig};
use madrigal_player::player::Player;
use madrigal_player::session::SessionState;
use madrigal_player::source::{ResolvedStream, StreamSource};
use madrigal_settings::Player as PlayerConfig;
use madrigal_settings::{RepeatMode, TransitionMode};
use madrigal_streams::Orchestrator;
use madrigal_ui::{queue_tabs_from_queue, QueueTabData};

pub const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Default, Clone)]
pub struct TrackMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: Option<u32>,
    pub thumbnail: Option<String>,
}

impl From<&Track> for TrackMeta {
    fn from(track: &Track) -> Self {
        Self {
            title: track.title.clone(),
            artist: track.artist_text(),
            album: track.album.clone().unwrap_or_default(),
            duration_secs: track.duration_secs,
            thumbnail: track.thumbnail.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub thumbnail: String,
    pub track_id: String,
    pub track_source: TrackSource,
    pub has_track: bool,
    pub position_ms: i32,
    pub duration_ms: i32,
    pub paused: bool,
    pub volume: f32,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    pub locked: bool,
    pub queue_index: i32,
    pub queue_revision: u64,
    pub queue: QueueTabData,
}

pub struct AppSource {
    streams: Orchestrator,
}

impl AppSource {
    pub fn new(streams: Orchestrator) -> Self {
        Self { streams }
    }
}

impl StreamSource for AppSource {
    fn resolve(&self, track: &TrackIdentity) -> Result<ResolvedStream, String> {
        match track.source {
            TrackSource::Local => resolve_local(track),
            TrackSource::Youtube => {
                let (stream, _) = self
                    .streams
                    .resolve(track, None)
                    .map_err(|err| err.to_string())?;
                Ok(ResolvedStream::new(stream.url).with_user_agent(stream.user_agent))
            }
            TrackSource::Tidal => Err(format!("unsupported source: {}", track.source)),
        }
    }

    fn resolves_remotely(&self, track: &TrackIdentity) -> bool {
        track.source == TrackSource::Youtube
    }
}

fn resolve_local(track: &TrackIdentity) -> Result<ResolvedStream, String> {
    if track.id.is_empty() {
        return Err("el track no tiene url".to_owned());
    }
    if track.id.starts_with("http://") || track.id.starts_with("https://") {
        return Ok(ResolvedStream::new(track.id.clone()));
    }
    let path = Path::new(&track.id);
    if !path.is_file() {
        return Err(format!("no es un archivo: {}", track.id));
    }
    Ok(ResolvedStream::new(track.id.clone()))
}

pub fn engine_config(settings: &madrigal_settings::Settings, ao: &str) -> EngineConfig {
    EngineConfig {
        vo: "null".to_owned(),
        ao: ao.to_owned(),
        gapless_audio: !matches!(settings.player.transition_mode, TransitionMode::Crossfade),
    }
}

pub fn track_from_file(path: &Path) -> Result<(TrackIdentity, TrackMeta), String> {
    let absolute = path
        .canonicalize()
        .map_err(|err| format!("{}: {err}", path.display()))?;
    if !absolute.is_file() {
        return Err(format!("no es un archivo: {}", absolute.display()));
    }
    let tags = madrigal_localmedia::parse_file(&absolute).ok();
    let stem = absolute
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = tags
        .as_ref()
        .and_then(|tags| tags.title.clone())
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(stem);
    let artist = tags
        .as_ref()
        .map(|tags| tags.artists.join(", "))
        .unwrap_or_default();
    let album = tags
        .as_ref()
        .and_then(|tags| tags.album.clone())
        .unwrap_or_default();
    let duration_secs = tags.as_ref().and_then(|tags| tags.duration_secs);
    let id = absolute.to_string_lossy().into_owned();
    Ok((
        TrackIdentity::local(id),
        TrackMeta {
            title,
            artist,
            album,
            duration_secs,
            thumbnail: None,
        },
    ))
}

pub struct Playback {
    player: Player<AppSource>,
    meta: HashMap<String, TrackMeta>,
    settings: PlayerConfig,
    revision: u64,
    error: Option<String>,
}

impl Playback {
    pub fn open(
        config: EngineConfig,
        settings: &PlayerConfig,
        streams: Orchestrator,
    ) -> Result<Self, String> {
        let engine = Engine::open(&config).map_err(|err| err.to_string())?;
        let mut player = Player::new(engine, AppSource::new(streams));
        player
            .apply_settings(settings)
            .map_err(|err| err.to_string())?;
        Ok(Self {
            player,
            meta: HashMap::new(),
            settings: settings.clone(),
            revision: 0,
            error: None,
        })
    }

    pub fn apply_settings(&mut self, settings: &PlayerConfig) -> Result<(), String> {
        self.player
            .apply_settings(settings)
            .map_err(|err| err.to_string())?;
        self.settings = settings.clone();
        Ok(())
    }

    pub fn enqueue_files(&mut self, paths: &[PathBuf]) -> (usize, Vec<String>) {
        let start = self.player.queue().len();
        let mut added = 0usize;
        let mut errors = Vec::new();
        for path in paths {
            match track_from_file(path) {
                Ok((identity, meta)) => {
                    let id = identity.id.clone();
                    if self.player.queue_mut().push(identity) {
                        self.meta.insert(id, meta);
                        added += 1;
                    } else {
                        errors.push(format!(
                            "la cola está bloqueada, se omite {}",
                            path.display()
                        ));
                    }
                }
                Err(err) => errors.push(err),
            }
        }
        if added > 0 {
            self.revision = self.revision.wrapping_add(1);
            let _ = self.player.play_at(start);
        }
        (added, errors)
    }

    pub fn report(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }

    pub fn enqueue_track(&mut self, track: &Track) -> Result<(), String> {
        let start = self.player.queue().len();
        if !self.player.queue_mut().push(track.identity.clone()) {
            return Err("queue is locked".to_owned());
        }
        self.meta
            .insert(track.identity.id.clone(), TrackMeta::from(track));
        self.revision = self.revision.wrapping_add(1);
        self.play_index(start)
    }

    pub fn play_index(&mut self, index: usize) -> Result<(), String> {
        self.player.play_at(index).map_err(|err| err.to_string())?;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), String> {
        self.player
            .engine()
            .set_paused(true)
            .map_err(|err| err.to_string())
    }

    pub fn resume(&mut self) -> Result<(), String> {
        if !self.has_track() {
            return Ok(());
        }
        self.player
            .resume(&self.settings)
            .map_err(|err| err.to_string())
    }

    pub fn session_state(&self) -> Option<SessionState> {
        self.player.snapshot().ok()
    }

    pub fn restore_session(&mut self, state: &SessionState) -> bool {
        if !self.player.restore(state).unwrap_or(false) {
            return false;
        }
        for identity in state
            .tracks
            .iter()
            .filter(|identity| identity.source == TrackSource::Local)
        {
            if let Ok((_, meta)) = track_from_file(Path::new(&identity.id)) {
                self.meta.insert(identity.id.clone(), meta);
            }
        }
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub fn repeat_mode(&self) -> RepeatMode {
        self.player.queue().repeat()
    }

    pub fn shuffle_enabled(&self) -> bool {
        self.player.queue().is_shuffled()
    }

    pub fn current_track(&self) -> Option<Track> {
        let identity = self.player.queue().current()?.clone();
        let meta = self.meta.get(&identity.id);
        let title = meta
            .map(|meta| meta.title.clone())
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| identity.id.clone());
        let artists = meta
            .map(|meta| {
                meta.artist
                    .split(", ")
                    .map(str::trim)
                    .filter(|artist| !artist.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let album = meta
            .map(|meta| meta.album.clone())
            .filter(|album| !album.trim().is_empty());
        let local_path = (identity.source == TrackSource::Local).then(|| identity.id.clone());
        Some(Track {
            identity,
            title,
            artists,
            album,
            duration_secs: meta.and_then(|meta| meta.duration_secs),
            thumbnail: meta.and_then(|meta| meta.thumbnail.clone()),
            track_number: None,
            local_path,
            set_video_id: None,
        })
    }

    pub fn play_pause(&mut self) -> Result<(), String> {
        if self.player.queue().current().is_none() || self.player.is_idle() {
            return self.player.play().map_err(|err| err.to_string());
        }
        if self.paused() {
            self.player
                .resume(&self.settings)
                .map_err(|err| err.to_string())
        } else {
            self.player
                .engine()
                .set_paused(true)
                .map_err(|err| err.to_string())
        }
    }

    pub fn next_track(&mut self) -> Result<bool, String> {
        let advanced = self.player.next_track().map_err(|err| err.to_string())?;
        if advanced {
            self.revision = self.revision.wrapping_add(1);
        }
        Ok(advanced)
    }

    pub fn previous_track(&mut self) -> Result<bool, String> {
        let moved = self.player.prev_track().map_err(|err| err.to_string())?;
        if moved {
            self.revision = self.revision.wrapping_add(1);
        }
        Ok(moved)
    }

    pub fn seek_ms(&mut self, ms: i64) -> Result<(), String> {
        let target = ms.max(0) as f64 / 1000.0;
        self.player
            .engine()
            .seek(target)
            .map_err(|err| err.to_string())
    }

    pub fn seek_relative(&mut self, secs: f64) -> Result<(), String> {
        let base = self.player.engine().position().unwrap_or(0.0);
        self.player
            .engine()
            .seek((base + secs).max(0.0))
            .map_err(|err| err.to_string())
    }

    pub fn set_volume(&mut self, volume: f32) -> Result<(), String> {
        let percent = (f64::from(volume) * 100.0).round() as i32;
        self.player
            .set_volume(percent)
            .map_err(|err| err.to_string())?;
        self.settings.volume = percent.clamp(0, 130);
        Ok(())
    }

    pub fn set_repeat(&mut self, mode: RepeatMode) {
        self.player.queue_mut().set_repeat(mode);
        self.settings.repeat_mode = mode;
    }

    pub fn set_shuffle(&mut self, enabled: bool) {
        self.player.queue_mut().set_shuffle(enabled);
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn set_locked(&mut self, locked: bool) {
        self.player.queue_mut().set_locked(locked);
    }

    pub fn remove(&mut self, index: usize) -> bool {
        let removed = self.player.queue_mut().remove(index);
        if removed {
            self.revision = self.revision.wrapping_add(1);
        }
        removed
    }

    pub fn move_item(&mut self, from: usize, to: usize) -> bool {
        let moved = self.player.queue_mut().move_item(from, to);
        if moved {
            self.revision = self.revision.wrapping_add(1);
        }
        moved
    }

    pub fn set_speed(&mut self, speed: f32) -> Result<(), String> {
        self.player
            .engine()
            .set_speed(f64::from(speed))
            .map_err(|err| err.to_string())
    }

    pub fn paused(&self) -> bool {
        self.player.engine().paused().unwrap_or(true)
    }

    pub fn position_ms(&self) -> i32 {
        self.player
            .engine()
            .position()
            .map(|secs| (secs * 1000.0).round().clamp(0.0, f64::from(i32::MAX)) as i32)
            .unwrap_or(0)
    }

    pub fn volume(&self) -> f32 {
        let percent = self
            .player
            .engine()
            .volume()
            .unwrap_or(f64::from(self.settings.volume));
        (percent as f32 / 100.0).clamp(0.0, 1.3)
    }

    pub fn has_track(&self) -> bool {
        self.player.queue().current().is_some()
    }

    pub fn error(&mut self) -> Option<String> {
        self.error.take()
    }

    pub fn engine_logs(&self) -> Vec<String> {
        self.player.engine().logs()
    }

    pub fn poll(&mut self, now: Instant) {
        if let Err(err) = self.player.poll_failed_load(now) {
            self.error = Some(err.to_string());
        }
        if let Err(err) = self.player.poll_sleep(now) {
            self.error = Some(err.to_string());
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let queue = self.player.queue();
        let current = queue.current();
        let track_id = current.map(|track| track.id.clone()).unwrap_or_default();
        let track_source = current
            .map(|track| track.source)
            .unwrap_or(TrackSource::Local);
        let meta = self.meta.get(&track_id);
        let meta_duration = meta.and_then(|meta| meta.duration_secs);
        let duration_ms = self
            .player
            .engine()
            .duration()
            .filter(|secs| *secs > 0.0)
            .map(|secs| (secs * 1000.0).round().clamp(0.0, f64::from(i32::MAX)) as i32)
            .or_else(|| meta_duration.map(|secs| (secs as i32).saturating_mul(1000)))
            .unwrap_or(0);
        let mut tabs = queue_tabs_from_queue(queue, "");
        for row in tabs
            .all
            .iter_mut()
            .chain(tabs.searched.iter_mut())
            .chain(tabs.cont.iter_mut())
            .chain(tabs.auto.iter_mut())
        {
            row.thumbnail = self
                .meta
                .get(row.title.as_str())
                .and_then(|meta| meta.thumbnail.clone())
                .unwrap_or_default();
        }
        Snapshot {
            title: meta.map(|meta| meta.title.clone()).unwrap_or_default(),
            artist: meta.map(|meta| meta.artist.clone()).unwrap_or_default(),
            album: meta.map(|meta| meta.album.clone()).unwrap_or_default(),
            thumbnail: meta
                .and_then(|meta| meta.thumbnail.clone())
                .unwrap_or_default(),
            track_id,
            track_source,
            has_track: current.is_some(),
            position_ms: self.position_ms(),
            duration_ms,
            paused: self.paused(),
            volume: self.volume(),
            repeat: queue.repeat(),
            shuffle: queue.is_shuffled(),
            locked: queue.locked(),
            queue_index: queue.current_index().map(|i| i as i32).unwrap_or(-1),
            queue_revision: self.revision,
            queue: tabs,
        }
    }
}

pub struct PlaybackService {
    core: Arc<Mutex<Playback>>,
    updates: Receiver<Snapshot>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl PlaybackService {
    pub fn start(playback: Playback) -> Self {
        let core = Arc::new(Mutex::new(playback));
        let (tx, updates) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let core = Arc::clone(&core);
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name("madrigal-playback".to_owned())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let snapshot = {
                            let mut playback = match core.lock() {
                                Ok(guard) => guard,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                            playback.poll(Instant::now());
                            playback.snapshot()
                        };
                        if tx.send(snapshot).is_err() {
                            break;
                        }
                        thread::sleep(POLL_INTERVAL);
                    }
                })
                .expect("spawn playback thread")
        };
        Self {
            core,
            updates,
            stop,
            worker: Some(worker),
        }
    }

    pub fn core(&self) -> Arc<Mutex<Playback>> {
        Arc::clone(&self.core)
    }

    pub fn latest(&self) -> Option<Snapshot> {
        let mut latest = None;
        while let Ok(snapshot) = self.updates.try_recv() {
            latest = Some(snapshot);
        }
        latest
    }
}

impl Drop for PlaybackService {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
