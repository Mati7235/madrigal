use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::lyrics_clock::{LyricsClock, MAX_OFFSET_MS};
use crate::lyrics_fetch::{
    ChainSource, DbLyricsCache, Generation, LyricsEvent, LyricsFetcher,
};
use crate::{LyricLineRow, LyricWordRow, MainShell, QueueRow};

static LYRICS_CACHE: Mutex<Option<madrigal_lyrics::Lyrics>> = Mutex::new(None);
static DRAG: Mutex<DragState> = Mutex::new(DragState { from: -1, press_y: 0.0 });

const CLOCK_TICK_MS: u64 = 16;

struct ClockWiring {
    clock: Rc<RefCell<LyricsClock>>,
    timer: Rc<slint::Timer>,
    running: Rc<Cell<bool>>,
}

thread_local! {
    static CLOCK: RefCell<Option<ClockWiring>> = const { RefCell::new(None) };
    static FETCH: RefCell<Option<FetchWiring>> = const { RefCell::new(None) };
    static LAST_TRACK: RefCell<Option<String>> = const { RefCell::new(None) };
}

struct FetchWiring {
    fetcher: Rc<LyricsFetcher>,
    generation: Generation,
}

struct DragState {
    from: i32,
    press_y: f32,
}

const ROW_H: f32 = 64.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlayerBackgroundStyle {
    Theme,
    Gradient,
    Blur,
    Glow,
}

pub fn background_index(style: PlayerBackgroundStyle) -> i32 {
    match style {
        PlayerBackgroundStyle::Theme => 0,
        PlayerBackgroundStyle::Gradient => 1,
        PlayerBackgroundStyle::Blur => 2,
        PlayerBackgroundStyle::Glow => 3,
    }
}

pub fn background_from_appearance(appearance: &madrigal_settings::Appearance) -> PlayerBackgroundStyle {
    use madrigal_settings::PlayerBackground as Bg;
    match appearance.player_background {
        Bg::Default => PlayerBackgroundStyle::Theme,
        Bg::Gradient => PlayerBackgroundStyle::Gradient,
        Bg::Custom | Bg::Blur | Bg::Coloring | Bg::BlurGradient => PlayerBackgroundStyle::Blur,
        Bg::Glow | Bg::GlowAnimated => PlayerBackgroundStyle::Glow,
    }
}

#[derive(Clone, Debug, Default)]
pub struct QueueRowData {
    pub title: String,
    pub artist: String,
    pub current: bool,
    pub thumbnail: String,
}

#[derive(Clone, Debug, Default)]
pub struct QueueTabData {
    pub all: Vec<QueueRowData>,
    pub searched: Vec<QueueRowData>,
    pub cont: Vec<QueueRowData>,
    pub auto: Vec<QueueRowData>,
}

pub fn queue_tabs_from_queue(queue: &madrigal_player::queue::Queue, query: &str) -> QueueTabData {
    let current = queue.current_index();
    let all: Vec<QueueRowData> = queue
        .items()
        .iter()
        .enumerate()
        .map(|(index, item)| QueueRowData {
            title: item.id.clone(),
            artist: String::new(),
            current: Some(index) == current,
            thumbnail: String::new(),
        })
        .collect();
    let searched = all
        .iter()
        .filter(|row| row.title.contains(query))
        .cloned()
        .collect();
    let cont = match current {
        Some(at) => all.iter().skip(at + 1).cloned().collect(),
        None => all.clone(),
    };
    QueueTabData { all, searched, cont, auto: Vec::new() }
}

fn to_model(rows: &[QueueRowData]) -> ModelRc<QueueRow> {
    ModelRc::new(Rc::new(VecModel::from_slice(rows.iter().map(|row| QueueRow {
        title: row.title.clone().into(),
        artist: row.artist.clone().into(),
        current: row.current,
        thumbnail: row.thumbnail.clone().into(),
        cover: Default::default(),
    }).collect::<Vec<_>>().as_slice())))
}

#[derive(Clone, Debug)]
pub struct PlaybackUiState {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub position_ms: i32,
    pub duration_ms: i32,
    pub paused: bool,
    pub liked: bool,
    pub repeat: madrigal_settings::RepeatMode,
    pub shuffle: bool,
    pub volume: f32,
    pub background: PlayerBackgroundStyle,
    pub queue_tab: i32,
    pub queue_locked: bool,
    pub queue: QueueTabData,
    pub lyrics_font_px: f32,
    pub lyrics_spacing: f32,
    pub track_id: String,
    pub track_source: madrigal_core::TrackSource,
    pub queue_index: i32,
}

pub fn repeat_index(mode: madrigal_settings::RepeatMode) -> i32 {
    match mode {
        madrigal_settings::RepeatMode::Off => 0,
        madrigal_settings::RepeatMode::One => 1,
        madrigal_settings::RepeatMode::All => 2,
    }
}

pub fn repeat_from_index(index: i32) -> madrigal_settings::RepeatMode {
    match index {
        1 => madrigal_settings::RepeatMode::One,
        2 => madrigal_settings::RepeatMode::All,
        _ => madrigal_settings::RepeatMode::Off,
    }
}

pub fn apply_playback_state(shell: &MainShell, state: &PlaybackUiState) {
    shell.set_track_title(state.title.clone().into());
    shell.set_track_artist(state.artist.clone().into());
    shell.set_track_album(state.album.clone().into());
    shell.set_has_track(!state.title.is_empty());
    shell.set_position_ms(state.position_ms);
    shell.set_duration_ms(state.duration_ms);
    shell.set_paused(state.paused);
    shell.set_liked(state.liked);
    shell.set_repeat_mode(repeat_index(state.repeat));
    shell.set_shuffle(state.shuffle);
    shell.set_volume(state.volume.clamp(0.0, 1.0));
    shell.set_bg_style(background_index(state.background));
    shell.set_queue_tab(state.queue_tab.clamp(0, 3));
    shell.set_queue_locked(state.queue_locked);
    shell.set_lyrics_font_px(state.lyrics_font_px);
    shell.set_lyrics_spacing(state.lyrics_spacing);
    shell.set_track_id(state.track_id.clone().into());
    shell.set_track_source(track_source_index(state.track_source));
    shell.set_queue_current_index(state.queue_index);
    set_queue_models(shell, &state.queue);
    anchor_clock(state.position_ms as i64);
    set_clock_playing(!state.paused);
    sync_clock(shell);
    maybe_fetch_lyrics(shell);
}

pub fn track_source_index(source: madrigal_core::TrackSource) -> i32 {
    match source {
        madrigal_core::TrackSource::Youtube => 0,
        madrigal_core::TrackSource::Tidal => 1,
        madrigal_core::TrackSource::Local => 2,
    }
}

pub fn track_source_from_index(index: i32) -> madrigal_core::TrackSource {
    match index {
        1 => madrigal_core::TrackSource::Tidal,
        2 => madrigal_core::TrackSource::Local,
        _ => madrigal_core::TrackSource::Youtube,
    }
}

fn anchor_clock(position_ms: i64) {
    CLOCK.with(|slot| {
        if let Some(wiring) = slot.borrow().as_ref() {
            wiring.clock.borrow_mut().anchor(position_ms, Instant::now());
        }
    });
}

fn set_clock_playing(playing: bool) {
    CLOCK.with(|slot| {
        if let Some(wiring) = slot.borrow().as_ref() {
            wiring.clock.borrow_mut().set_playing(playing, Instant::now());
        }
    });
}

fn set_clock_offset(ms: i64) {
    CLOCK.with(|slot| {
        if let Some(wiring) = slot.borrow().as_ref() {
            wiring.clock.borrow_mut().set_offset(ms);
        }
    });
}

fn clock_should_run(shell: &MainShell) -> bool {
    shell.get_full_player_open() && shell.get_has_track() && !shell.get_paused()
}

fn start_clock(shell: &MainShell, wiring: &ClockWiring) {
    if wiring.running.get() {
        return;
    }
    wiring.running.set(true);
    let weak = shell.as_weak();
    let clock = wiring.clock.clone();
    let running = wiring.running.clone();
    let timer = wiring.timer.clone();
    wiring.timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(CLOCK_TICK_MS),
        move || {
            let Some(shell) = weak.upgrade() else {
                running.set(false);
                timer.stop();
                return;
            };
            if !clock_should_run(&shell) {
                running.set(false);
                timer.stop();
                return;
            }
            clock_tick(&shell, &clock);
        },
    );
}

fn sync_clock(shell: &MainShell) {
    CLOCK.with(|slot| {
        let slot = slot.borrow();
        let Some(wiring) = slot.as_ref() else {
            return;
        };
        if clock_should_run(shell) {
            start_clock(shell, wiring);
        } else {
            wiring.running.set(false);
            wiring.timer.stop();
        }
    });
}

fn clock_tick(shell: &MainShell, clock: &Rc<RefCell<LyricsClock>>) {
    let now = Instant::now();
    let position = {
        let mut clock = clock.borrow_mut();
        clock.pump(Duration::from_millis(CLOCK_TICK_MS), now);
        clock.position_at(now)
    };
    let duration = shell.get_duration_ms().max(0) as i64;
    let position = if duration > 0 { position.min(duration) } else { position };
    shell.set_position_ms(position as i32);
    refresh_karaoke(shell, position);
    shell.set_lyrics_clock_ticks(shell.get_lyrics_clock_ticks() + 1);
}

pub fn wire_lyrics_clock(shell: &MainShell) {
    let clock = Rc::new(RefCell::new(LyricsClock::new(Instant::now())));
    let timer = Rc::new(slint::Timer::default());
    let running = Rc::new(Cell::new(false));
    let previous = CLOCK.with(|slot| {
        let previous = slot.borrow_mut().take();
        *slot.borrow_mut() = Some(ClockWiring { clock, timer, running });
        previous
    });
    if let Some(previous) = previous {
        previous.running.set(false);
        previous.timer.stop();
    }
    sync_clock(shell);
}

pub fn sync_lyrics_clock(shell: &MainShell, position_ms: i64, playing: bool) {
    let now = Instant::now();
    CLOCK.with(|slot| {
        if let Some(wiring) = slot.borrow().as_ref() {
            let mut clock = wiring.clock.borrow_mut();
            clock.anchor(position_ms, now);
            clock.set_playing(playing, now);
        }
    });
    refresh_karaoke(shell, position_ms);
    sync_clock(shell);
}

pub fn set_queue_models(shell: &MainShell, tabs: &QueueTabData) {
    shell.set_queue_all(to_model(&tabs.all));
    shell.set_queue_searched(to_model(&tabs.searched));
    shell.set_queue_continue(to_model(&tabs.cont));
    shell.set_queue_auto(to_model(&tabs.auto));
    let total = (tabs.all.len() + tabs.searched.len() + tabs.cont.len() + tabs.auto.len()) as i32;
    shell.set_queue_count(total);
    crate::artwork::resolve_queue(shell, tabs);
}

pub fn set_lyrics(shell: &MainShell, lyrics: &madrigal_lyrics::Lyrics) {
    let words_model = |line: &madrigal_lyrics::LyricLine| {
        let rows: Vec<LyricWordRow> = match line.words.as_ref() {
            Some(words) if !words.is_empty() => {
                let mut cursor = 0usize;
                words
                    .iter()
                    .map(|word| {
                        let text = word.text.trim();
                        let after = match line.text.get(cursor..).and_then(|rest| rest.find(text)) {
                            Some(at) => cursor + at + text.len(),
                            None => line.text.len(),
                        };
                        let space = line
                            .text
                            .get(after..)
                            .and_then(|rest| rest.chars().next())
                            .is_some_and(char::is_whitespace);
                        cursor = after.min(line.text.len());
                        let display = if space { format!("{text}\u{00A0}") } else { text.to_owned() };
                        LyricWordRow {
                            text: display.into(),
                            start: word.start_ms.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
                            end: word
                                .end_ms
                                .unwrap_or(word.start_ms)
                                .clamp(i32::MIN as i64, i32::MAX as i64) as i32,
                        }
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        ModelRc::new(Rc::new(VecModel::from_slice(rows.as_slice())))
    };
    let rows: Vec<LyricLineRow> = lyrics
        .lines
        .iter()
        .map(|line| LyricLineRow {
            text: line.text.clone().into(),
            translation: line.markers.translation.clone().unwrap_or_default().into(),
            start: line.start_ms.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            words: words_model(line),
        })
        .collect();
    shell.set_lyric_rows(ModelRc::new(Rc::new(VecModel::from_slice(rows.as_slice()))));
    shell.set_lyrics_synced(lyrics.synced);
    *LYRICS_CACHE.lock().unwrap() = Some(lyrics.clone());
    refresh_karaoke(shell, shell.get_position_ms() as i64);
}

pub fn clear_lyrics(shell: &MainShell) {
    shell.set_lyric_rows(ModelRc::new(Rc::new(VecModel::from(Vec::<LyricLineRow>::new()))));
    *LYRICS_CACHE.lock().unwrap() = None;
    shell.set_active_line(-1);
    shell.set_active_word(-1);
    shell.set_word_progress(0.0);
}

pub fn lyrics_delivery(shell: &MainShell) -> Arc<dyn Fn(LyricsEvent) + Send + Sync> {
    let weak = shell.as_weak();
    Arc::new(move |event| {
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(shell) = weak.upgrade() {
                apply_lyrics_event(&shell, event);
            }
        });
    })
}

pub fn chain_fetcher(shell: &MainShell) -> Rc<LyricsFetcher> {
    Rc::new(LyricsFetcher::new(
        Box::new(DbLyricsCache::default_profile()),
        Box::new(ChainSource::default_profile()),
        lyrics_delivery(shell),
    ))
}

pub fn wire_lyrics_fetch(shell: &MainShell, fetcher: Rc<LyricsFetcher>) {
    let generation = fetcher.generation();
    FETCH.with(|slot| *slot.borrow_mut() = Some(FetchWiring { fetcher, generation }));
    LAST_TRACK.with(|slot| *slot.borrow_mut() = None);
    shell.on_lyrics_retry_requested({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                shell.set_lyrics_retry_count(shell.get_lyrics_retry_count() + 1);
                fetch_current_lyrics(&shell);
            }
        }
    });
}

pub fn apply_lyrics_event(shell: &MainShell, event: LyricsEvent) {
    let current = FETCH.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|wiring| wiring.generation.is_current(event.request_id()))
    });
    if !current {
        return;
    }
    match event {
        LyricsEvent::Found { lyrics, .. } => {
            set_lyrics(shell, &lyrics);
            shell.set_lyrics_error("".into());
            shell.set_lyrics_state(2);
        }
        LyricsEvent::NotFound { .. } => {
            clear_lyrics(shell);
            shell.set_lyrics_error("".into());
            shell.set_lyrics_state(3);
        }
        LyricsEvent::Failed { message, .. } => {
            clear_lyrics(shell);
            shell.set_lyrics_error(message.into());
            shell.set_lyrics_state(4);
        }
    }
}

fn current_track(shell: &MainShell) -> Option<madrigal_core::Track> {
    let id = shell.get_track_id();
    if id.is_empty() {
        return None;
    }
    let artist = shell.get_track_artist();
    let album = shell.get_track_album();
    let duration_ms = shell.get_duration_ms();
    Some(madrigal_core::Track {
        identity: madrigal_core::TrackIdentity::new(
            track_source_from_index(shell.get_track_source()),
            id.to_string(),
        ),
        title: shell.get_track_title().to_string(),
        artists: if artist.is_empty() { Vec::new() } else { vec![artist.to_string()] },
        album: (!album.is_empty()).then(|| album.to_string()),
        duration_secs: (duration_ms > 0).then(|| (duration_ms as u32).div_ceil(1000)),
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    })
}

pub fn maybe_fetch_lyrics(shell: &MainShell) {
    let key = format!("{}:{}", shell.get_track_id(), shell.get_queue_current_index());
    let changed = LAST_TRACK.with(|slot| {
        let mut slot = slot.borrow_mut();
        let changed = slot.as_deref() != Some(key.as_str());
        if changed {
            *slot = Some(key);
        }
        changed
    });
    if changed {
        fetch_current_lyrics(shell);
    }
}

fn fetch_current_lyrics(shell: &MainShell) -> bool {
    let Some(track) = current_track(shell) else {
        clear_lyrics(shell);
        shell.set_lyrics_error("".into());
        shell.set_lyrics_state(0);
        return false;
    };
    let fetcher = FETCH.with(|slot| slot.borrow().as_ref().map(|wiring| wiring.fetcher.clone()));
    let Some(fetcher) = fetcher else {
        return false;
    };
    let config =
        madrigal_lyrics::chain::ChainConfig::from(&madrigal_settings::Settings::load().lyrics);
    clear_lyrics(shell);
    shell.set_lyrics_error("".into());
    shell.set_lyrics_state(1);
    fetcher.request(track, config);
    shell.set_lyrics_fetch_count(shell.get_lyrics_fetch_count() + 1);
    true
}

pub fn active_lyric_at(lyrics: &madrigal_lyrics::Lyrics, t_ms: i32) -> i32 {
    if !lyrics.synced {
        return -1;
    }
    madrigal_lyrics::karaoke::line_at(lyrics, t_ms as i64).map(|i| i as i32).unwrap_or(-1)
}

pub fn active_word_at(lyrics: &madrigal_lyrics::Lyrics, t_ms: i32) -> i32 {
    if !lyrics.synced {
        return -1;
    }
    let t = t_ms as i64;
    madrigal_lyrics::karaoke::line_at(lyrics, t)
        .and_then(|index| lyrics.lines.get(index))
        .and_then(|line| madrigal_lyrics::karaoke::word_at(line, t))
        .map(|index| index as i32)
        .unwrap_or(-1)
}

pub fn lyric_progress_at(lyrics: &madrigal_lyrics::Lyrics, t_ms: i32) -> f32 {
    if !lyrics.synced {
        return 0.0;
    }
    let t = t_ms as i64;
    match madrigal_lyrics::karaoke::line_at(lyrics, t).and_then(|i| lyrics.lines.get(i)) {
        Some(line) => madrigal_lyrics::karaoke::progress_within(line, t),
        None => 0.0,
    }
}

fn clock_offset_ms() -> i64 {
    CLOCK.with(|slot| {
        slot.borrow().as_ref().map(|wiring| wiring.clock.borrow().offset_ms()).unwrap_or(0)
    })
}

fn refresh_karaoke(shell: &MainShell, position_ms: i64) {
    let t = (position_ms + clock_offset_ms()).clamp(0, i32::MAX as i64) as i32;
    let cache = LYRICS_CACHE.lock().unwrap();
    match cache.as_ref() {
        Some(lyrics) => {
            shell.set_active_line(active_lyric_at(lyrics, t));
            shell.set_word_progress(lyric_progress_at(lyrics, t));
            shell.set_active_word(active_word_at(lyrics, t));
        }
        None => {
            shell.set_active_line(-1);
            shell.set_word_progress(0.0);
            shell.set_active_word(-1);
        }
    }
}

pub fn apply_lyrics_settings(shell: &MainShell, settings: &madrigal_settings::Lyrics) {
    shell.set_lyrics_font_px(settings.font_size.clamp(10.0, 28.0));
    shell.set_lyrics_spacing(settings.line_spacing.clamp(0.5, 3.0));
}

pub fn apply_background_style(shell: &MainShell, appearance: &madrigal_settings::Appearance) {
    shell.set_bg_style(background_index(background_from_appearance(appearance)));
}

pub fn apply_player_settings(shell: &MainShell, settings: &madrigal_settings::Player) {
    shell.set_volume((settings.volume as f32 / 100.0).clamp(0.0, 1.0));
    shell.set_repeat_mode(repeat_index(settings.repeat_mode));
    shell.set_tempo_semitones(settings.pitch_semitones);
    shell.set_tempo_mult(settings.playback_speed);
    shell.set_eq_on(settings.equalizer_enabled);
    shell.set_eq_gains(eq_gains_model(&settings.equalizer_gains));
}

fn eq_gains_model(gains: &[f32]) -> ModelRc<f32> {
    let normalized: Vec<f32> = madrigal_settings::EQ_BANDS
        .iter()
        .enumerate()
        .map(|(i, _)| gains.get(i).copied().unwrap_or(0.0))
        .map(|gain| ((gain + 12.0) / 24.0).clamp(0.0, 1.0))
        .collect();
    ModelRc::new(Rc::new(VecModel::from_slice(normalized.as_slice())))
}

pub fn eq_preset_gains(name: &str) -> Option<[f32; 10]> {
    madrigal_player::audio::preset_gains(name)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlayerDialog {
    None,
    Sleep,
    Tempo,
    Equalizer,
    Error,
    Info,
}

pub fn dialog_index(dialog: PlayerDialog) -> i32 {
    match dialog {
        PlayerDialog::None => 0,
        PlayerDialog::Sleep => 1,
        PlayerDialog::Tempo => 2,
        PlayerDialog::Equalizer => 3,
        PlayerDialog::Error => 4,
        PlayerDialog::Info => 5,
    }
}

pub fn open_player_dialog(shell: &MainShell, dialog: PlayerDialog) {
    shell.set_player_dialog(dialog_index(dialog));
}

pub fn set_stream_info(shell: &MainShell, info: Option<&madrigal_player::stream_info::StreamInfo>) {
    match info {
        Some(info) => {
            shell.set_stream_codec(info.codec.clone().unwrap_or_default().into());
            shell.set_stream_bitrate(info.abr_kbps.map(|kbps| format!("{kbps:.0} kbps")).unwrap_or_default().into());
            shell.set_stream_rate(info.sample_rate.map(|rate| format!("{} Hz", rate)).unwrap_or_default().into());
            shell.set_stream_loudness(info.loudness_hint.map(|loud| format!("{loud:.1} dB")).unwrap_or_default().into());
            shell.set_stream_buffer(info.buffer_health.map(|health| format!("{health:.0}%")).unwrap_or_default().into());
            shell.set_stream_state(format!("{:?} @ {:.2}x", info.state, info.speed).into());
        }
        None => {
            shell.set_stream_codec("".into());
            shell.set_stream_bitrate("".into());
            shell.set_stream_rate("".into());
            shell.set_stream_loudness("".into());
            shell.set_stream_buffer("".into());
            shell.set_stream_state("".into());
        }
    }
}

pub fn show_error(shell: &MainShell, code: &str, message: &str) {
    shell.set_error_code(code.into());
    shell.set_error_message(message.into());
    open_player_dialog(shell, PlayerDialog::Error);
}

pub fn render_share_png(
    lyrics: &madrigal_lyrics::Lyrics,
    title: &str,
    artist: &str,
    active: Option<usize>,
) -> Vec<u8> {
    let options = madrigal_lyrics::share_image::ShareOptions {
        title: title.to_owned(),
        artist: artist.to_owned(),
        active,
        ..Default::default()
    };
    madrigal_lyrics::share_image::render_png(lyrics, &options)
}

fn active_model(shell: &MainShell) -> ModelRc<QueueRow> {
    match shell.get_queue_tab() {
        1 => shell.get_queue_searched(),
        2 => shell.get_queue_continue(),
        3 => shell.get_queue_auto(),
        _ => shell.get_queue_all(),
    }
}

fn move_in_model(model: &ModelRc<QueueRow>, from: usize, to: usize) -> bool {
    let count = model.row_count();
    if from >= count || to >= count || from == to {
        return false;
    }
    let Some(cell) = model.as_any().downcast_ref::<VecModel<QueueRow>>() else {
        return false;
    };
    let item = cell.row_data(from).unwrap();
    cell.remove(from);
    cell.insert(to, item);
    true
}

fn remove_in_model(model: &ModelRc<QueueRow>, at: usize) -> bool {
    if at >= model.row_count() {
        return false;
    }
    let Some(cell) = model.as_any().downcast_ref::<VecModel<QueueRow>>() else {
        return false;
    };
    cell.remove(at);
    true
}

fn set_eq_band(shell: &MainShell, index: usize, value: f32) {
    let model = shell.get_eq_gains();
    if let Some(cell) = model.as_any().downcast_ref::<VecModel<f32>>() {
        if index < cell.row_count() {
            cell.set_row_data(index, value.clamp(0.0, 1.0));
        }
    }
}

pub fn wire_player_callbacks(shell: &MainShell) {
    wire_lyrics_clock(shell);
    shell.on_player_state_changed({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                sync_clock(&shell);
            }
        }
    });
    shell.on_line_at(|t| {
        LYRICS_CACHE.lock().unwrap().as_ref().map(|lyrics| active_lyric_at(lyrics, t)).unwrap_or(-1)
    });
    shell.on_line_progress(|t| {
        LYRICS_CACHE.lock().unwrap().as_ref().map(|lyrics| lyric_progress_at(lyrics, t)).unwrap_or(0.0)
    });
    shell.on_play_pause_toggled({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                let paused = !shell.get_paused();
                shell.set_paused(paused);
                set_clock_playing(!paused);
                sync_clock(&shell);
            }
        }
    });
    shell.on_like_toggled({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                shell.set_liked(!shell.get_liked());
                shell.set_like_count(shell.get_like_count() + 1);
            }
        }
    });
    shell.on_queue_opened({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                shell.set_queue_open(true);
                shell.set_queue_open_count(shell.get_queue_open_count() + 1);
            }
        }
    });
    shell.on_queue_closed({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                shell.set_queue_open(false);
            }
        }
    });
    shell.on_full_player_opened({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                shell.set_full_player_open(true);
                sync_clock(&shell);
            }
        }
    });
    shell.on_full_player_closed({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                shell.set_full_player_open(false);
                shell.set_player_dialog(0);
                shell.set_player_dismiss_count(shell.get_player_dismiss_count() + 1);
                sync_clock(&shell);
            }
        }
    });
    shell.on_repeat_toggled({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                shell.set_repeat_mode((shell.get_repeat_mode() + 1) % 3);
            }
        }
    });
    shell.on_shuffle_toggled({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                shell.set_shuffle(!shell.get_shuffle());
            }
        }
    });
    shell.on_volume_changed({
        let shell = shell.as_weak();
        move |value| {
            if let Some(shell) = shell.upgrade() {
                shell.set_volume(value.clamp(0.0, 1.0));
                shell.set_volume_count(shell.get_volume_count() + 1);
            }
        }
    });
    shell.on_seek_to_requested({
        let shell = shell.as_weak();
        move |ms| {
            if let Some(shell) = shell.upgrade() {
                let clamped = ms.clamp(0, shell.get_duration_ms().max(0));
                shell.set_position_ms(clamped);
                anchor_clock(clamped as i64);
                refresh_karaoke(&shell, clamped as i64);
                shell.set_seek_count(shell.get_seek_count() + 1);
                shell.set_seek_last_ms(clamped);
            }
        }
    });
    shell.on_lyrics_seek_requested({
        let shell = shell.as_weak();
        move |ms| {
            if let Some(shell) = shell.upgrade() {
                let clamped = ms.clamp(0, shell.get_duration_ms().max(0));
                shell.set_position_ms(clamped);
                anchor_clock(clamped as i64);
                refresh_karaoke(&shell, clamped as i64);
                shell.set_lyrics_seek_count(shell.get_lyrics_seek_count() + 1);
                shell.set_lyrics_seek_ms(clamped);
            }
        }
    });
    shell.on_lyrics_offset_changed({
        let shell = shell.as_weak();
        move |delta| {
            if let Some(shell) = shell.upgrade() {
                let next = (shell.get_lyrics_offset_ms() as i64 + delta as i64)
                    .clamp(-MAX_OFFSET_MS, MAX_OFFSET_MS);
                set_clock_offset(next);
                shell.set_lyrics_offset_ms(next as i32);
                refresh_karaoke(&shell, shell.get_position_ms() as i64);
                shell.set_lyrics_offset_count(shell.get_lyrics_offset_count() + 1);
            }
        }
    });
    shell.on_lyrics_share_requested({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                shell.set_lyrics_share_count(shell.get_lyrics_share_count() + 1);
                if let Ok(path) = std::env::var("MADRIGAL_SHARE_OUT") {
                    if let Some(lyrics) = LYRICS_CACHE.lock().unwrap().clone() {
                        let active = shell.get_active_line();
                        let png = render_share_png(
                            &lyrics,
                            &shell.get_track_title(),
                            &shell.get_track_artist(),
                            if active >= 0 { Some(active as usize) } else { None },
                        );
                        let _ = std::fs::write(path, png);
                    }
                }
            }
        }
    });
    shell.on_queue_play_requested({
        let shell = shell.as_weak();
        move |index| {
            if let Some(shell) = shell.upgrade() {
                shell.set_queue_play_count(shell.get_queue_play_count() + 1);
                shell.set_queue_play_last(index);
            }
        }
    });
    shell.on_queue_remove_requested({
        let shell = shell.as_weak();
        move |index| {
            if let Some(shell) = shell.upgrade() {
                if shell.get_queue_locked() || index < 0 {
                    return;
                }
                if remove_in_model(&active_model(&shell), index as usize) {
                    shell.set_queue_remove_count(shell.get_queue_remove_count() + 1);
                    shell.set_queue_remove_last(index);
                }
            }
        }
    });
    shell.on_queue_move_requested({
        let shell = shell.as_weak();
        move |from, to| {
            if let Some(shell) = shell.upgrade() {
                if shell.get_queue_locked() || from < 0 || to < 0 {
                    return;
                }
                if move_in_model(&active_model(&shell), from as usize, to as usize) {
                    shell.set_queue_move_count(shell.get_queue_move_count() + 1);
                    shell.set_queue_move_from(from);
                    shell.set_queue_move_to(to);
                }
            }
        }
    });
    shell.on_queue_tab_selected({
        let shell = shell.as_weak();
        move |tab| {
            if let Some(shell) = shell.upgrade() {
                shell.set_queue_tab(tab.clamp(0, 3));
            }
        }
    });
    shell.on_queue_lock_toggled({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                shell.set_queue_locked(!shell.get_queue_locked());
            }
        }
    });
    shell.on_player_dialog_requested({
        let shell = shell.as_weak();
        move |dialog| {
            if let Some(shell) = shell.upgrade() {
                shell.set_player_dialog(dialog.clamp(0, 5));
                shell.set_dialog_count(shell.get_dialog_count() + 1);
            }
        }
    });
    shell.on_player_dialog_dismissed({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                shell.set_player_dialog(0);
            }
        }
    });
    shell.on_sleep_selected({
        let shell = shell.as_weak();
        move |mode, minutes| {
            if let Some(shell) = shell.upgrade() {
                shell.set_sleep_mode(mode.clamp(0, 2));
                shell.set_sleep_minutes(minutes.max(0));
                shell.set_sleep_count(shell.get_sleep_count() + 1);
            }
        }
    });
    shell.on_tempo_committed({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                shell.set_tempo_count(shell.get_tempo_count() + 1);
            }
        }
    });
    shell.on_tempo_reset({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                shell.set_tempo_semitones(0.0);
                shell.set_tempo_mult(1.0);
                shell.set_tempo_count(shell.get_tempo_count() + 1);
            }
        }
    });
    shell.on_eq_toggled({
        let shell = shell.as_weak();
        move |on| {
            if let Some(shell) = shell.upgrade() {
                shell.set_eq_on(on);
                shell.set_eq_count(shell.get_eq_count() + 1);
            }
        }
    });
    shell.on_eq_band_changed({
        let shell = shell.as_weak();
        move |index, value| {
            if let Some(shell) = shell.upgrade() {
                if index >= 0 {
                    set_eq_band(&shell, index as usize, value);
                    shell.set_eq_count(shell.get_eq_count() + 1);
                }
            }
        }
    });
    shell.on_eq_preset_selected({
        let shell = shell.as_weak();
        move |name| {
            if let Some(shell) = shell.upgrade() {
                if let Some(gains) = eq_preset_gains(&name) {
                    let normalized: Vec<f32> =
                        gains.iter().map(|gain| ((gain + 12.0) / 24.0).clamp(0.0, 1.0)).collect();
                    shell.set_eq_gains(ModelRc::new(Rc::new(VecModel::from_slice(
                        normalized.as_slice(),
                    ))));
                    shell.set_eq_preset(name);
                    shell.set_eq_count(shell.get_eq_count() + 1);
                }
            }
        }
    });
    shell.on_error_retried({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                shell.set_error_retry_count(shell.get_error_retry_count() + 1);
            }
        }
    });
    shell.on_drag_start(move |index, y| {
        *DRAG.lock().unwrap() = DragState { from: index, press_y: y };
    });
    shell.on_drag_move({
        let shell = shell.as_weak();
        move |y| {
            if let Some(shell) = shell.upgrade() {
                let drag = DRAG.lock().unwrap();
                if drag.from < 0 {
                    return;
                }
                let count = active_model(&shell).row_count() as i32;
                if count == 0 {
                    return;
                }
                let target = (drag.from as f32 + (y - drag.press_y) / ROW_H).round() as i32;
                shell.set_drag_from(drag.from);
                shell.set_drag_to(target.clamp(0, count - 1));
            }
        }
    });
    shell.on_drag_end({
        let shell = shell.as_weak();
        move || {
            if let Some(shell) = shell.upgrade() {
                let drag = std::mem::replace(
                    &mut *DRAG.lock().unwrap(),
                    DragState { from: -1, press_y: 0.0 },
                );
                let (from, to) = (drag.from, shell.get_drag_to());
                shell.set_drag_from(-1);
                shell.set_drag_to(-1);
                if shell.get_queue_locked() || from < 0 || to < 0 || from == to {
                    return;
                }
                if move_in_model(&active_model(&shell), from as usize, to as usize) {
                    shell.set_queue_move_count(shell.get_queue_move_count() + 1);
                    shell.set_queue_move_from(from);
                    shell.set_queue_move_to(to);
                }
            }
        }
    });
}
