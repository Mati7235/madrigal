use std::cell::RefCell;
use std::env;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use madrigal_core::Track;
use madrigal_i18n::tr;
use madrigal_mpris::{
    ActionResult, LoopStatus, MprisService, NowPlaying, NowPlayingProvider, PlaybackStatus,
};
use madrigal_notify::{
    ACTION_NEXT, ACTION_PREVIOUS, ACTION_TOGGLE, NotificationBuilder, Notifier, ZbusBackend,
};
use madrigal_player::session::{self, SessionState};
use madrigal_scrobble::outbox::{integrations_enabled, Outbox, OutboxEntry, ScrobbleSink};
use madrigal_scrobble::{Eligibility, ListenBrainzClient, MalojaClient, SubmitOutcome};
use madrigal_settings::{Integrations, RepeatMode, Settings};
use madrigal_ui::{show_error, MainShell};
use slint::ComponentHandle;

use crate::playback::Playback;
use crate::ui::lang_from_settings;

pub const TICK: Duration = Duration::from_millis(250);
pub const SETTINGS_DEBOUNCE: Duration = Duration::from_millis(750);
pub const SESSION_INTERVAL: Duration = Duration::from_secs(5);
pub const FLUSH_INTERVAL: Duration = Duration::from_secs(30);
pub const HANDOFF_SOCKET: &str = "madrigal-handoff.sock";
pub const BUS_ADDRESS_ENV: &str = "MADRIGAL_DBUS_ADDRESS";
pub const ERROR_CODE: &str = "playback";

fn lock(core: &Arc<Mutex<Playback>>) -> MutexGuard<'_, Playback> {
    match core.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

pub fn bus_address() -> Option<String> {
    match env::var(BUS_ADDRESS_ENV) {
        Ok(value) => Some(value).filter(|value| !value.trim().is_empty()),
        Err(_) => env::var("DBUS_SESSION_BUS_ADDRESS")
            .ok()
            .filter(|value| !value.trim().is_empty()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Previous,
    PlayPause,
    Next,
}

pub fn action_from_notification(key: &str) -> Option<Action> {
    match key {
        ACTION_PREVIOUS => Some(Action::Previous),
        ACTION_TOGGLE => Some(Action::PlayPause),
        ACTION_NEXT => Some(Action::Next),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerState {
    pub volume: i32,
    pub repeat: RepeatMode,
}

impl PlayerState {
    pub fn from_playback(playback: &Playback) -> Self {
        Self {
            volume: (playback.volume() * 100.0).round().clamp(0.0, 130.0) as i32,
            repeat: playback.repeat_mode(),
        }
    }
}

pub fn persist_player_state(path: &Path, state: PlayerState) -> io::Result<bool> {
    let mut settings = Settings::load_from(path);
    let player = &mut settings.player;
    if player.volume == state.volume && player.repeat_mode == state.repeat {
        return Ok(false);
    }
    player.volume = state.volume;
    player.repeat_mode = state.repeat;
    settings.save_to(path)?;
    Ok(true)
}

pub struct SessionKeeper {
    path: PathBuf,
    last: Option<SessionState>,
    written: Option<Instant>,
}

impl SessionKeeper {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            last: None,
            written: None,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn last(&self) -> Option<&SessionState> {
        self.last.as_ref()
    }

    pub fn due(&self, now: Instant) -> bool {
        self.written
            .is_none_or(|at| now.saturating_duration_since(at) >= SESSION_INTERVAL)
    }

    pub fn restore(&mut self, playback: &mut Playback) -> Option<SessionState> {
        let state = session::load_from(&self.path).ok().flatten()?;
        if state.is_empty() || !playback.restore_session(&state) {
            return None;
        }
        self.last = Some(state.clone());
        self.written = Some(Instant::now());
        Some(state)
    }

    pub fn tick(&mut self, state: SessionState, now: Instant) -> bool {
        if self.last.as_ref() == Some(&state) || !self.due(now) {
            return false;
        }
        self.write(&state, now).is_ok()
    }

    pub fn save(&mut self, state: &SessionState) -> io::Result<()> {
        self.write(state, Instant::now())
    }

    fn write(&mut self, state: &SessionState, now: Instant) -> io::Result<()> {
        session::save_to(state, &self.path)?;
        self.last = Some(state.clone());
        self.written = Some(now);
        Ok(())
    }
}

pub fn handoff_path() -> PathBuf {
    madrigal_paths::runtime_dir().join(HANDOFF_SOCKET)
}

pub fn forward_files(path: &Path, files: &[String]) -> io::Result<usize> {
    if files.is_empty() {
        return Ok(0);
    }
    let payload = serde_json::to_vec(files)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let mut stream = UnixStream::connect(path)?;
    stream.write_all(&payload)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(files.len())
}

pub struct Handoff {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Handoff {
    pub fn start(
        path: PathBuf,
        mut on_files: impl FnMut(&[String]) + Send + 'static,
    ) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let listener = match UnixListener::bind(&path) {
            Ok(listener) => listener,
            Err(err) if err.kind() == io::ErrorKind::AddrInUse => {
                if UnixStream::connect(&path).is_ok() {
                    return Err(err);
                }
                std::fs::remove_file(&path)?;
                UnixListener::bind(&path)?
            }
            Err(err) => return Err(err),
        };
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name("madrigal-handoff".to_owned())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        match listener.accept() {
                            Ok((stream, _)) => {
                                if let Some(files) = read_files(stream) {
                                    on_files(&files);
                                }
                            }
                            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                                thread::sleep(Duration::from_millis(50));
                            }
                            Err(_) => break,
                        }
                    }
                })?
        };
        Ok(Self {
            path,
            stop,
            worker: Some(worker),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Handoff {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

fn read_files(stream: UnixStream) -> Option<Vec<String>> {
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    serde_json::from_str(line.trim()).ok()
}

#[derive(Debug, Clone)]
pub struct Labels {
    pub previous: String,
    pub toggle: String,
    pub next: String,
}

impl Labels {
    pub fn from_settings(settings: &Settings) -> Self {
        let lang = lang_from_settings(settings.appearance.language);
        Self {
            previous: tr(lang, "action.previousTrack").to_owned(),
            toggle: tr(lang, "action.playPause").to_owned(),
            next: tr(lang, "action.nextTrack").to_owned(),
        }
    }
}

struct Notice {
    title: String,
    artists: String,
}

pub struct NotifyBridge {
    tx: Sender<Notice>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl NotifyBridge {
    pub fn track(&self, track: &Track) {
        let _ = self.tx.send(Notice {
            title: track.title.clone(),
            artists: track.artist_text(),
        });
    }
}

impl Drop for NotifyBridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn start_notifications(
    address: Option<&str>,
    labels: Labels,
    actions: Sender<Action>,
) -> Option<NotifyBridge> {
    let backend = zbus::block_on(ZbusBackend::connect(address)).ok()?;
    let subscriber = zbus::block_on(ZbusBackend::connect(address)).ok()?;
    let mut subscription = zbus::block_on(subscriber.subscribe_actions()).ok()?;
    let notifier = Notifier::new(backend);
    let labeled = zbus::block_on(notifier.server_capabilities())
        .map(|caps| caps.iter().any(|cap| cap == "actions"))
        .unwrap_or(false);
    let (tx, rx) = mpsc::channel::<Notice>();
    let stop = Arc::new(AtomicBool::new(false));
    let worker = {
        let stop = Arc::clone(&stop);
        thread::Builder::new()
            .name("madrigal-notify".to_owned())
            .spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let notice = match rx.recv_timeout(Duration::from_millis(200)) {
                        Ok(notice) => notice,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    let builder = NotificationBuilder::new(notice.title).body(notice.artists);
                    let builder = if labeled {
                        builder.playback_actions(&labels.previous, &labels.toggle, &labels.next)
                    } else {
                        builder
                    };
                    let _ = zbus::block_on(notifier.show(&builder));
                }
            })
            .ok()?
    };
    thread::Builder::new()
        .name("madrigal-notify-actions".to_owned())
        .spawn(move || {
            while let Ok((_, key)) = zbus::block_on(subscription.next()) {
                let Some(action) = action_from_notification(&key) else {
                    continue;
                };
                if actions.send(action).is_err() {
                    break;
                }
            }
        })
        .ok()?;
    Some(NotifyBridge {
        tx,
        stop,
        worker: Some(worker),
    })
}

fn playback_status(has_track: bool, paused: bool) -> PlaybackStatus {
    if !has_track {
        PlaybackStatus::Stopped
    } else if paused {
        PlaybackStatus::Paused
    } else {
        PlaybackStatus::Playing
    }
}

fn repeat_from_loop(status: LoopStatus) -> RepeatMode {
    match status {
        LoopStatus::None => RepeatMode::Off,
        LoopStatus::Track => RepeatMode::One,
        LoopStatus::Playlist => RepeatMode::All,
    }
}

struct PlaybackProvider {
    core: Arc<Mutex<Playback>>,
}

impl NowPlayingProvider for PlaybackProvider {
    fn now_playing(&self) -> Option<NowPlaying> {
        let playback = lock(&self.core);
        let track = playback.current_track()?;
        Some(NowPlaying {
            id: track.identity.id.clone(),
            title: track.title,
            artists: track.artists,
            album: track.album,
            art_url: track.thumbnail,
            url: track.local_path,
            length_secs: f64::from(track.duration_secs.unwrap_or(0)),
        })
    }

    fn playback_status(&self) -> PlaybackStatus {
        let playback = lock(&self.core);
        playback_status(playback.has_track(), playback.paused())
    }

    fn position_secs(&self) -> f64 {
        f64::from(lock(&self.core).position_ms()) / 1000.0
    }

    fn loop_status(&self) -> LoopStatus {
        LoopStatus::from_repeat(lock(&self.core).repeat_mode())
    }

    fn shuffle(&self) -> bool {
        lock(&self.core).shuffle_enabled()
    }

    fn volume(&self) -> f64 {
        f64::from(lock(&self.core).volume()).clamp(0.0, 1.0)
    }

    fn rate(&self) -> f64 {
        1.0
    }

    fn play(&self) -> ActionResult {
        lock(&self.core).resume()
    }

    fn pause(&self) -> ActionResult {
        lock(&self.core).pause()
    }

    fn play_pause(&self) -> ActionResult {
        lock(&self.core).play_pause()
    }

    fn stop(&self) -> ActionResult {
        let mut playback = lock(&self.core);
        playback.pause()?;
        playback.seek_ms(0)
    }

    fn next(&self) -> ActionResult {
        lock(&self.core).next_track().map(|_| ())
    }

    fn previous(&self) -> ActionResult {
        lock(&self.core).previous_track().map(|_| ())
    }

    fn seek_relative(&self, offset_secs: f64) -> ActionResult {
        lock(&self.core).seek_relative(offset_secs)
    }

    fn seek_absolute(&self, position_secs: f64) -> ActionResult {
        lock(&self.core).seek_ms((position_secs.max(0.0) * 1000.0) as i64)
    }

    fn set_loop(&self, status: LoopStatus) -> ActionResult {
        lock(&self.core).set_repeat(repeat_from_loop(status));
        Ok(())
    }

    fn set_shuffle(&self, shuffle: bool) -> ActionResult {
        lock(&self.core).set_shuffle(shuffle);
        Ok(())
    }

    fn set_volume(&self, volume: f64) -> ActionResult {
        lock(&self.core).set_volume(volume.clamp(0.0, 1.0) as f32)
    }

    fn set_rate(&self, rate: f64) -> ActionResult {
        lock(&self.core).set_speed(rate as f32)
    }
}

pub struct MprisBridge {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Drop for MprisBridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn start_mpris(core: Arc<Mutex<Playback>>, address: &str) -> Option<MprisBridge> {
    let service = zbus::block_on(MprisService::start(
        Arc::new(PlaybackProvider { core }),
        Some(address),
    ))
    .ok()?;
    let stop = Arc::new(AtomicBool::new(false));
    let worker = {
        let stop = Arc::clone(&stop);
        thread::Builder::new()
            .name("madrigal-mpris".to_owned())
            .spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_secs(1));
                    if zbus::block_on(service.sync()).is_err() {
                        break;
                    }
                }
            })
            .ok()?
    };
    Some(MprisBridge {
        stop,
        worker: Some(worker),
    })
}

pub const MAX_TICK_GAP: Duration = Duration::from_secs(2);

#[derive(Debug, Default)]
pub struct TrackWatch {
    key: String,
    started_at: i64,
    played: Duration,
    last: Option<Instant>,
    submitted: bool,
}

impl TrackWatch {
    pub fn played(&self) -> Duration {
        self.played
    }
}

pub fn observe_track(
    outbox: &mut Outbox,
    watch: &mut TrackWatch,
    eligibility: &Eligibility,
    track: &Track,
    playing: bool,
    now: Instant,
    unix: i64,
) -> bool {
    let key = track.identity.to_storage_string();
    if watch.key != key {
        *watch = TrackWatch {
            key,
            started_at: unix,
            played: Duration::ZERO,
            last: Some(now),
            submitted: false,
        };
    }
    if let (true, Some(last)) = (playing, watch.last) {
        watch.played += now.saturating_duration_since(last).min(MAX_TICK_GAP);
    }
    watch.last = Some(now);
    if watch.submitted {
        return false;
    }
    let duration_secs = u64::from(track.duration_secs.unwrap_or(0));
    if !eligibility.eligible(duration_secs, watch.played.as_secs()) {
        return false;
    }
    watch.submitted = true;
    outbox.push(track, watch.started_at).unwrap_or(false)
}

pub fn combine_outcomes(outcomes: &[SubmitOutcome]) -> SubmitOutcome {
    if outcomes.is_empty() {
        return SubmitOutcome::Retryable;
    }
    if outcomes.contains(&SubmitOutcome::Submitted) {
        return SubmitOutcome::Submitted;
    }
    if outcomes.contains(&SubmitOutcome::Retryable) {
        return SubmitOutcome::Retryable;
    }
    SubmitOutcome::Permanent
}

struct RemoteSink {
    maloja: Option<MalojaClient>,
    listenbrainz: Option<ListenBrainzClient>,
}

impl RemoteSink {
    fn from_integrations(integrations: &Integrations) -> Self {
        let maloja = integrations
            .maloja_enabled
            .then(|| MalojaClient::from_integrations(integrations))
            .filter(|client| client.configured());
        let listenbrainz = integrations
            .listenbrainz_enabled
            .then(|| ListenBrainzClient::from_integrations(integrations))
            .filter(|client| client.configured());
        Self {
            maloja,
            listenbrainz,
        }
    }
}

impl ScrobbleSink for RemoteSink {
    fn submit(&self, entry: &OutboxEntry) -> SubmitOutcome {
        let track = entry.track();
        let mut outcomes = Vec::new();
        if let Some(client) = &self.maloja {
            outcomes.push(SubmitOutcome::from_result(
                &client.scrobble(&track, entry.started_at),
            ));
        }
        if let Some(client) = &self.listenbrainz {
            outcomes.push(SubmitOutcome::from_result(
                &client.submit_listen(&track, entry.started_at),
            ));
        }
        combine_outcomes(&outcomes)
    }
}

enum ScrobbleCommand {
    Observe {
        track: Box<Track>,
        playing: bool,
        now: Instant,
        unix: i64,
    },
    Flush {
        now: i64,
    },
}

pub struct ScrobbleBridge {
    tx: Option<Sender<ScrobbleCommand>>,
    worker: Option<JoinHandle<()>>,
}

impl ScrobbleBridge {
    pub fn observe(&self, track: &Track, playing: bool, now: Instant, unix: i64) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(ScrobbleCommand::Observe {
                track: Box::new(track.clone()),
                playing,
                now,
                unix,
            });
        }
    }

    pub fn flush(&self, now: i64) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(ScrobbleCommand::Flush { now });
        }
    }
}

impl Drop for ScrobbleBridge {
    fn drop(&mut self) {
        self.tx = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn start_scrobbling(integrations: &Integrations) -> Option<ScrobbleBridge> {
    if !integrations_enabled(integrations) {
        return None;
    }
    let mut outbox = Outbox::open_default().ok()?;
    let eligibility = Eligibility::from_integrations(integrations);
    let sink = RemoteSink::from_integrations(integrations);
    let (tx, rx) = mpsc::channel::<ScrobbleCommand>();
    let worker = thread::Builder::new()
        .name("madrigal-scrobble".to_owned())
        .spawn(move || {
            let mut watch = TrackWatch::default();
            let mut last_flush = Instant::now();
            loop {
                match rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(ScrobbleCommand::Observe {
                        track,
                        playing,
                        now,
                        unix,
                    }) => {
                        observe_track(
                            &mut outbox,
                            &mut watch,
                            &eligibility,
                            &track,
                            playing,
                            now,
                            unix,
                        );
                    }
                    Ok(ScrobbleCommand::Flush { now }) => {
                        let _ = outbox.flush(&sink, now);
                        last_flush = Instant::now();
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if last_flush.elapsed() >= FLUSH_INTERVAL {
                            let _ = outbox.flush(&sink, unix_now());
                            last_flush = Instant::now();
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .ok()?;
    Some(ScrobbleBridge {
        tx: Some(tx),
        worker: Some(worker),
    })
}

struct Runtime {
    settings_path: PathBuf,
    settings: PlayerState,
    dirty: Option<Instant>,
    session: SessionKeeper,
    scrobble: Option<ScrobbleBridge>,
    notify: Option<NotifyBridge>,
    actions: Receiver<Action>,
    notice_key: String,
}

pub struct Desktop {
    core: Arc<Mutex<Playback>>,
    settings_path: PathBuf,
    runtime: Rc<RefCell<Runtime>>,
    timer: slint::Timer,
    _handoff: Option<Handoff>,
    _mpris: Option<MprisBridge>,
}

impl Desktop {
    pub fn start(shell: &MainShell, core: Arc<Mutex<Playback>>, settings: &Settings) -> Self {
        let settings_path = madrigal_paths::settings_path();
        let state = PlayerState::from_playback(&lock(&core));
        let mut keeper = SessionKeeper::new(session::session_path());
        if settings.player.resume_enabled {
            let restored = {
                let mut playback = lock(&core);
                keeper.restore(&mut playback)
            };
            if settings.player.resume_autoplay {
                if let Some(state) = restored.filter(|state| !state.paused) {
                    let _ = lock(&core).play_index(state.position.unwrap_or(0));
                }
            }
        }
        let (action_tx, actions) = mpsc::channel();
        let bus = bus_address();
        let mpris = bus
            .as_deref()
            .and_then(|address| start_mpris(Arc::clone(&core), address));
        let notify = bus.as_deref().filter(|_| settings.appearance.notify_track_change).and_then(|address| {
            start_notifications(Some(address), Labels::from_settings(settings), action_tx)
        });
        let scrobble = start_scrobbling(&settings.integrations);
        let handoff = {
            let core = Arc::clone(&core);
            Handoff::start(handoff_path(), move |files| {
                let paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
                let _ = lock(&core).enqueue_files(&paths);
            })
            .ok()
        };
        let runtime = Rc::new(RefCell::new(Runtime {
            settings_path: settings_path.clone(),
            settings: state,
            dirty: None,
            session: keeper,
            scrobble,
            notify,
            actions,
            notice_key: String::new(),
        }));
        let timer = slint::Timer::default();
        {
            let weak = shell.as_weak();
            let core = Arc::clone(&core);
            let runtime = Rc::clone(&runtime);
            timer.start(slint::TimerMode::Repeated, TICK, move || {
                if let Some(shell) = weak.upgrade() {
                    tick(&shell, &core, &runtime);
                }
            });
        }
        Self {
            core,
            settings_path,
            runtime,
            timer,
            _handoff: handoff,
            _mpris: mpris,
        }
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        self.timer.stop();
        let session = lock(&self.core).session_state();
        if let Some(session) = session {
            let _ = self.runtime.borrow_mut().session.save(&session);
        }
        let state = PlayerState::from_playback(&lock(&self.core));
        let _ = persist_player_state(&self.settings_path, state);
    }
}

fn tick(shell: &MainShell, core: &Arc<Mutex<Playback>>, runtime: &Rc<RefCell<Runtime>>) {
    let now = Instant::now();
    let error = lock(core).error();
    if let Some(error) = error {
        show_error(shell, ERROR_CODE, &error);
    }
    let (state, track, playing) = {
        let playback = lock(core);
        (
            PlayerState::from_playback(&playback),
            playback.current_track(),
            playback.has_track() && !playback.paused(),
        )
    };
    let pending: Vec<Action> = {
        let runtime = runtime.borrow();
        runtime.actions.try_iter().collect()
    };
    for action in pending {
        match action {
            Action::Previous => shell.invoke_prev_requested(),
            Action::PlayPause => shell.invoke_play_pause_toggled(),
            Action::Next => shell.invoke_next_requested(),
        }
    }
    let mut runtime = runtime.borrow_mut();
    if runtime.settings != state {
        runtime.settings = state;
        runtime.dirty = Some(now);
    }
    if runtime
        .dirty
        .is_some_and(|at| now.saturating_duration_since(at) >= SETTINGS_DEBOUNCE)
        && persist_player_state(&runtime.settings_path, runtime.settings).is_ok()
    {
        runtime.dirty = None;
    }
    if runtime.session.due(now) {
        let session = lock(core).session_state();
        if let Some(session) = session {
            runtime.session.tick(session, now);
        }
    }
    if let Some(track) = track {
        let key = format!("{}|{}", track.title, track.artist_text());
        if runtime.notice_key != key {
            runtime.notice_key = key;
            if let Some(notify) = &runtime.notify {
                notify.track(&track);
            }
        }
        if let Some(scrobble) = &runtime.scrobble {
            scrobble.observe(&track, playing, now, unix_now());
        }
    }
}
