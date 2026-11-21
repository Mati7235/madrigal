use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};

use madrigal_player::engine::EngineConfig;
use madrigal_ui::{
    apply_background_style, apply_lyrics_settings, apply_player_settings, repeat_from_index,
    repeat_index, set_queue_models, sync_lyrics_clock, track_source_index, wire_player_callbacks,
    MainShell,
};
use slint::ComponentHandle;

use crate::catalog::{self, Wiring};
use crate::downloads::Downloads;
use crate::playback::{engine_config, Playback, PlaybackService, Snapshot, POLL_INTERVAL};
use crate::store::Store;
use crate::ui::{boot_ui, Ui};

pub struct AppState {
    pub store: Store,
    pub ui: Ui,
    playback: Rc<RefCell<PlaybackService>>,
    _library: crate::library::LibraryService,
    _downloads: Downloads,
    _poll: slint::Timer,
    _catalog: Wiring,
}

fn lock(core: &Arc<Mutex<Playback>>) -> MutexGuard<'_, Playback> {
    match core.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

impl AppState {
    pub fn boot() -> Result<Self, String> {
        let ui = boot_ui()?;
        let store = Store::open()?;
        let config = engine_config(store.settings(), "auto");
        Self::with_ui(ui, store, config)
    }

    pub fn boot_with_engine(config: EngineConfig) -> Result<Self, String> {
        let ui = boot_ui()?;
        let store = Store::open()?;
        Self::with_ui(ui, store, config)
    }

    fn with_ui(ui: Ui, store: Store, config: EngineConfig) -> Result<Self, String> {
        let settings = store.settings();
        let playback = Playback::open(config, &settings.player)?;
        let service = Rc::new(RefCell::new(PlaybackService::start(playback)));
        let core = service.borrow().core();

        apply_player_settings(&ui.shell, &settings.player);
        apply_lyrics_settings(&ui.shell, &settings.lyrics);
        apply_background_style(&ui.shell, &settings.appearance);
        wire_player_callbacks(&ui.shell);
        wire_playback(&ui.shell, &core);
        crate::lyrics::wire(&ui.shell);
        let library = crate::library::wire_library(&ui.shell, &store);
        let downloads = crate::downloads::register(&ui.shell, settings);
        let poll = start_poll(&ui.shell, Rc::clone(&service));
        let catalog = catalog::wire_catalog(&ui.shell, &settings.content)?;

        Ok(Self {
            store,
            ui,
            playback: service,
            _library: library,
            _downloads: downloads,
            _poll: poll,
            _catalog: catalog,
        })
    }

    pub fn open_files(&self, files: &[String]) -> (usize, Vec<String>) {
        if files.is_empty() {
            return (0, Vec::new());
        }
        let paths: Vec<std::path::PathBuf> = files.iter().map(std::path::PathBuf::from).collect();
        let core = self.playback.borrow().core();
        let mut playback = lock(&core);
        playback.enqueue_files(&paths)
    }
}

fn wire_playback(shell: &MainShell, core: &Arc<Mutex<Playback>>) {
    shell.on_play_pause_toggled({
        let core = Arc::clone(core);
        let weak = shell.as_weak();
        move || {
            let mut playback = lock(&core);
            let _ = playback.play_pause();
            let paused = playback.paused();
            let position = playback.position_ms();
            let has_track = playback.has_track();
            drop(playback);
            if let Some(shell) = weak.upgrade() {
                shell.set_paused(paused);
                if has_track {
                    sync_lyrics_clock(&shell, i64::from(position), !paused);
                }
            }
        }
    });

    shell.on_next_requested({
        let core = Arc::clone(core);
        move || {
            let _ = lock(&core).next_track();
        }
    });

    shell.on_prev_requested({
        let core = Arc::clone(core);
        move || {
            let _ = lock(&core).previous_track();
        }
    });

    shell.on_seek_to_requested({
        let core = Arc::clone(core);
        let weak = shell.as_weak();
        move |ms| {
            let (position, paused) = {
                let mut playback = lock(&core);
                let _ = playback.seek_ms(i64::from(ms));
                (playback.position_ms(), playback.paused())
            };
            if let Some(shell) = weak.upgrade() {
                let clamped = position.clamp(0, shell.get_duration_ms().max(0));
                shell.set_position_ms(clamped);
                shell.set_seek_count(shell.get_seek_count() + 1);
                shell.set_seek_last_ms(clamped);
                sync_lyrics_clock(&shell, i64::from(clamped), !paused);
            }
        }
    });

    shell.on_seek_requested({
        let core = Arc::clone(core);
        let weak = shell.as_weak();
        move |delta| {
            let (position, paused) = {
                let mut playback = lock(&core);
                let _ = playback.seek_relative(f64::from(delta));
                (playback.position_ms(), playback.paused())
            };
            if let Some(shell) = weak.upgrade() {
                let clamped = position.clamp(0, shell.get_duration_ms().max(0));
                shell.set_position_ms(clamped);
                sync_lyrics_clock(&shell, i64::from(clamped), !paused);
            }
        }
    });

    shell.on_volume_changed({
        let core = Arc::clone(core);
        let weak = shell.as_weak();
        move |value| {
            let volume = value.clamp(0.0, 1.0);
            let _ = lock(&core).set_volume(volume);
            if let Some(shell) = weak.upgrade() {
                shell.set_volume(volume);
                shell.set_volume_count(shell.get_volume_count() + 1);
            }
        }
    });

    shell.on_repeat_toggled({
        let core = Arc::clone(core);
        let weak = shell.as_weak();
        move || {
            if let Some(shell) = weak.upgrade() {
                let mode = (shell.get_repeat_mode() + 1) % 3;
                shell.set_repeat_mode(mode);
                lock(&core).set_repeat(repeat_from_index(mode));
            }
        }
    });

    shell.on_shuffle_toggled({
        let core = Arc::clone(core);
        let weak = shell.as_weak();
        move || {
            if let Some(shell) = weak.upgrade() {
                let enabled = !shell.get_shuffle();
                shell.set_shuffle(enabled);
                lock(&core).set_shuffle(enabled);
            }
        }
    });

    shell.on_queue_play_requested({
        let core = Arc::clone(core);
        let weak = shell.as_weak();
        move |index| {
            if index < 0 {
                return;
            }
            let _ = lock(&core).play_index(index as usize);
            if let Some(shell) = weak.upgrade() {
                shell.set_queue_play_count(shell.get_queue_play_count() + 1);
                shell.set_queue_play_last(index);
            }
        }
    });

    shell.on_queue_remove_requested({
        let core = Arc::clone(core);
        let weak = shell.as_weak();
        move |index| {
            if index < 0 {
                return;
            }
            if lock(&core).remove(index as usize) {
                if let Some(shell) = weak.upgrade() {
                    shell.set_queue_remove_count(shell.get_queue_remove_count() + 1);
                    shell.set_queue_remove_last(index);
                }
            }
        }
    });

    shell.on_queue_move_requested({
        let core = Arc::clone(core);
        let weak = shell.as_weak();
        move |from, to| {
            if from < 0 || to < 0 {
                return;
            }
            if lock(&core).move_item(from as usize, to as usize) {
                if let Some(shell) = weak.upgrade() {
                    shell.set_queue_move_count(shell.get_queue_move_count() + 1);
                    shell.set_queue_move_from(from);
                    shell.set_queue_move_to(to);
                }
            }
        }
    });

    shell.on_queue_lock_toggled({
        let core = Arc::clone(core);
        let weak = shell.as_weak();
        move || {
            if let Some(shell) = weak.upgrade() {
                let locked = !shell.get_queue_locked();
                shell.set_queue_locked(locked);
                lock(&core).set_locked(locked);
            }
        }
    });

    shell.on_tempo_committed({
        let core = Arc::clone(core);
        let weak = shell.as_weak();
        move || {
            if let Some(shell) = weak.upgrade() {
                let _ = lock(&core).set_speed(shell.get_tempo_mult());
                shell.set_tempo_count(shell.get_tempo_count() + 1);
            }
        }
    });

    shell.on_tempo_reset({
        let core = Arc::clone(core);
        let weak = shell.as_weak();
        move || {
            if let Some(shell) = weak.upgrade() {
                shell.set_tempo_semitones(0.0);
                shell.set_tempo_mult(1.0);
                let _ = lock(&core).set_speed(1.0);
                shell.set_tempo_count(shell.get_tempo_count() + 1);
            }
        }
    });
}

fn start_poll(shell: &MainShell, service: Rc<RefCell<PlaybackService>>) -> slint::Timer {
    let weak = shell.as_weak();
    let revision = Rc::new(Cell::new(u64::MAX));
    let index = Rc::new(Cell::new(i32::MIN));
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, POLL_INTERVAL, move || {
        let Some(shell) = weak.upgrade() else {
            return;
        };
        let Some(snapshot) = service.borrow().latest() else {
            return;
        };
        let queue_changed =
            revision.get() != snapshot.queue_revision || index.get() != snapshot.queue_index;
        revision.set(snapshot.queue_revision);
        index.set(snapshot.queue_index);
        apply_snapshot(&shell, &snapshot, queue_changed);
    });
    timer
}

fn apply_snapshot(shell: &MainShell, snapshot: &Snapshot, queue_changed: bool) {
    shell.set_track_title(snapshot.title.clone().into());
    shell.set_track_artist(snapshot.artist.clone().into());
    shell.set_track_album(snapshot.album.clone().into());
    shell.set_track_id(snapshot.track_id.clone().into());
    shell.set_track_source(track_source_index(snapshot.track_source));
    shell.set_has_track(snapshot.has_track);
    shell.set_position_ms(snapshot.position_ms);
    shell.set_duration_ms(snapshot.duration_ms);
    shell.set_paused(snapshot.paused);
    shell.set_volume(snapshot.volume.clamp(0.0, 1.0));
    shell.set_repeat_mode(repeat_index(snapshot.repeat));
    shell.set_shuffle(snapshot.shuffle);
    shell.set_queue_locked(snapshot.locked);
    shell.set_queue_current_index(snapshot.queue_index);
    if queue_changed {
        set_queue_models(shell, &snapshot.queue);
        shell.set_queue_count(snapshot.queue.all.len() as i32);
    }
    sync_lyrics_clock(shell, i64::from(snapshot.position_ms), !snapshot.paused);
}
