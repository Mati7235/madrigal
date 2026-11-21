use madrigal_testkit as kit;
use madrigal_ui::{
    apply_background_style, apply_lyrics_settings, apply_playback_state, apply_player_settings,
    dispatch_key, eq_band_labels, lyrics_delivery, open_player_dialog, queue_tabs_from_queue,
    render_share_png, resolution_route, sample_rate_text, set_lyrics, set_stream_info, show_error,
    wire_lyrics_fetch, wire_player_callbacks, wire_shell_i18n, CachedLyrics, FetchTask, LyricsCache,
    LyricsFetcher, LyricsSource, MainShell, PlaybackUiState, PlayerBackgroundStyle, PlayerDialog,
    QueueTabData,
};
use slint::{ComponentHandle, LogicalPosition, Model};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

type LyricsReply = Result<Option<madrigal_lyrics::chain::ChainOutcome>, String>;

struct FakeSource {
    pending: Mutex<Sender<(String, Sender<LyricsReply>)>>,
}

impl LyricsSource for FakeSource {
    fn fetch(&self, task: &FetchTask) -> LyricsReply {
        let (respond, reply) = channel();
        self.pending
            .lock()
            .unwrap()
            .send((task.track.identity.id.clone(), respond))
            .unwrap();
        reply.recv().map_err(|err| err.to_string())?
    }
}

struct EmptyCache;

impl LyricsCache for EmptyCache {
    fn lookup(&self, _identity: &madrigal_core::TrackIdentity) -> Option<CachedLyrics> {
        None
    }
}

fn outcome(lyrics: madrigal_lyrics::Lyrics) -> madrigal_lyrics::chain::ChainOutcome {
    madrigal_lyrics::chain::ChainOutcome { lyrics, provider: "fake" }
}

fn plain_lyrics() -> madrigal_lyrics::Lyrics {
    madrigal_lyrics::lrc::parse("Primera línea\nSegunda línea\n")
}

fn line_lyrics(text: &str) -> madrigal_lyrics::Lyrics {
    madrigal_lyrics::Lyrics {
        lines: vec![madrigal_lyrics::LyricLine {
            start_ms: 0,
            end_ms: None,
            text: text.to_owned(),
            words: None,
            markers: Default::default(),
        }],
        synced: false,
    }
}

fn pump_until(timeout: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    loop {
        let _ = slint::quit_event_loop();
        slint::run_event_loop().unwrap();
        if done() {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for the lyrics fetch");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn wait_for_request(requests: &Receiver<(String, Sender<LyricsReply>)>, id: &str) -> Sender<LyricsReply> {
    let (sent_id, respond) = requests.recv_timeout(Duration::from_secs(5)).expect("lyrics request");
    assert_eq!(sent_id, id);
    respond
}

fn assert_player_layout(shell: &MainShell, label: &str) {
    for (width, height) in kit::WINDOW_SIZES {
        kit::resize(shell, width, height);
        let violations: Vec<_> = kit::lint_layout(shell, (width, height))
            .into_iter()
            .filter(|violation| match &violation.kind {
                kit::ViolationKind::Overlap(other) => {
                    in_player_surface(&violation.id) && in_player_surface(other)
                }
                kit::ViolationKind::OutOfWindow => true,
            })
            .collect();
        assert!(
            violations.is_empty(),
            "{label} layout violations at {width}x{height}: {violations:#?}"
        );
    }
    kit::resize(shell, 1920.0, 1080.0);
}

fn fixture_lyrics() -> madrigal_lyrics::Lyrics {
    use madrigal_lyrics::{LineMarkers, LyricLine, Lyrics, WordTiming};
    let words = |pairs: &[(i64, i64, &str)]| {
        Some(
            pairs
                .iter()
                .map(|(start, end, text)| WordTiming {
                    start_ms: *start,
                    end_ms: Some(*end),
                    text: (*text).to_owned(),
                })
                .collect::<Vec<_>>(),
        )
    };
    Lyrics {
        lines: vec![
            LyricLine {
                start_ms: 0,
                end_ms: Some(1500),
                text: "Hola".to_owned(),
                words: words(&[(0, 500, "Ho"), (500, 1000, "la")]),
                markers: LineMarkers::default(),
            },
            LyricLine {
                start_ms: 2000,
                end_ms: Some(3500),
                text: "Mundo".to_owned(),
                words: words(&[(2000, 2500, "Mun"), (2500, 3000, "do")]),
                markers: LineMarkers {
                    translation: Some("World".to_owned()),
                    ..Default::default()
                },
            },
            LyricLine {
                start_ms: 4000,
                end_ms: Some(6000),
                text: "Fin".to_owned(),
                words: None,
                markers: LineMarkers::default(),
            },
        ],
        synced: true,
    }
}

fn session_lyrics() -> madrigal_lyrics::Lyrics {
    use madrigal_lyrics::{LineMarkers, LyricLine, WordTiming};
    let mut lyrics = fixture_lyrics();
    lyrics.lines[0].words = None;
    lyrics.lines[2].start_ms = 6_000;
    lyrics.lines[2].end_ms = Some(8_000);
    lyrics.lines.push(LyricLine {
        start_ms: 12_000,
        end_ms: Some(14_000),
        text: "Vuelta".to_owned(),
        words: Some(vec![WordTiming {
            start_ms: 12_000,
            end_ms: Some(13_000),
            text: "Vuelta".to_owned(),
        }]),
        markers: LineMarkers::default(),
    });
    lyrics
}

fn in_player_surface(id: &str) -> bool {
    id.starts_with("FullPlayer::") || id.starts_with("PlayerConsole::") || id.starts_with("PlayerTools::") || id.starts_with("LyricsView::")
}

fn word_lyrics_fixture() -> madrigal_lyrics::Lyrics {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../madrigal-lyrics/tests/fixtures/enhanced.lrc");
    let text = std::fs::read_to_string(path).expect("enhanced lrc fixture");
    madrigal_lyrics::lrc_enhanced::parse(&text).expect("parse enhanced lrc")
}

fn fixture_queue() -> madrigal_player::queue::Queue {
    use madrigal_core::TrackIdentity;
    use madrigal_player::queue::{QueueEntry, TrackMeta};
    let mut queue = madrigal_player::queue::Queue::new();
    for id in ["song-a1", "song-a2", "song-b1", "song-b2"] {
        assert!(queue.push(TrackIdentity::youtube(id)));
    }
    assert!(queue.push_entry(QueueEntry::with_meta(
        TrackIdentity::youtube("2ttGjtfQ7EA"),
        TrackMeta {
            title: "Canción A".to_owned(),
            artist: "Artista A".to_owned(),
            album: "Álbum A".to_owned(),
            duration_secs: Some(245),
            thumbnail: Some("https://example.test/a.webp".to_owned()),
        },
    )));
    queue.play_at(0);
    queue
}

#[test]
fn ui_i18n_formats_args() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", tmp.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", tmp.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", tmp.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", tmp.path().join("state"));
    kit::init_headless();
    let shell = MainShell::new().unwrap();
    wire_shell_i18n(&shell, madrigal_i18n::Lang::Es);
    let args = slint::ModelRc::new(slint::VecModel::from(vec![
        slint::SharedString::from("2.5"),
    ]));
    let filled: slint::SharedString = shell
        .global::<madrigal_ui::I18n>()
        .invoke_trf("es".into(), "audio.speed".into(), args);
    assert_eq!(filled.as_str(), "Velocidad: 2.5×");
    let pitch: slint::SharedString = shell.global::<madrigal_ui::I18n>().invoke_trf(
        "en".into(),
        "audio.pitch".into(),
        slint::ModelRc::new(slint::VecModel::from(vec![slint::SharedString::from("-2")])),
    );
    assert_eq!(pitch.as_str(), "Pitch: -2 semitones");
}

fn drag_to(shell: &MainShell, id: &str, index: usize, frac: f32) {
    let handles: Vec<_> = kit::ElementHandle::find_by_element_id(shell, id).collect();
    assert!(index < handles.len(), "missing {id}[{index}] of {}", handles.len());
    let handle = &handles[index];
    let pos = handle.absolute_position();
    let size = handle.size();
    let target = LogicalPosition::new(
        pos.x + 16.0 + frac * (size.width - 32.0),
        pos.y + size.height / 2.0,
    );
    handle.mock_drag(target, slint::platform::PointerEventButton::Left);
}

#[test]
fn liked_icons_are_filled_hearts() {
    for source in [
        include_str!("../../ui/player/full.slint"),
        include_str!("../../ui/player/bar.slint"),
        include_str!("../../ui/pages/library/library.slint"),
    ] {
        assert!(source.contains("Icons.heart-filled : Icons.heart"));
        assert!(!source.contains("Icons.star-filled : Icons.heart"));
    }
    assert!(include_str!("../../ui/components/icons.slint").contains("heart-filled"));
    let svg = include_str!("../../ui/icons/heart-filled.svg");
    assert!(svg.contains("fill=\"#000000\""));
    assert!(!svg.contains("fill=\"none\""));
}

#[test]
fn ui_player() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", tmp.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", tmp.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", tmp.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", tmp.path().join("state"));

    kit::init_headless();
    let shell = MainShell::new().unwrap();
    wire_shell_i18n(&shell, madrigal_i18n::Lang::Es);
    wire_player_callbacks(&shell);

    for (key, es, en) in [
        ("player.sleep_timer", "Temporizador de apagado", "Sleep timer"),
        ("player.sleep_off", "Desactivar", "Turn off"),
        ("player.sleep_off_status", "Desactivado", "Off"),
        ("player.sleep_status_minutes", "Se apaga en", "Turns off in"),
        ("player.tempoPitch", "Tempo y tono", "Tempo and pitch"),
        ("player.playbackError", "Error de reproducción", "Playback error"),
        ("player.shareLyrics", "Compartir letra", "Share lyrics"),
        ("player.lyricsBackToLine", "Volver a la línea actual", "Back to current line"),
        ("player.lyricsOffsetHelp", "Ajustá el tiempo si la letra va adelantada o atrasada", "Adjust the timing if the lyrics run ahead or behind"),
        ("player.lyricsOffsetMinus", "Desfase de la letra −0,1 s", "Lyrics offset −0.1 s"),
        ("player.lyricsOffsetPlus", "Desfase de la letra +0,1 s", "Lyrics offset +0.1 s"),
        ("player.lyricsSearching", "Buscando letra", "Searching lyrics"),
        ("player.lyricsNotFound", "No hay letra para esta canción", "No lyrics for this track"),
        ("player.lyricsError", "No se pudo buscar la letra", "Could not search lyrics"),
        ("player.lyricsSyncedChip", "Sincronizada", "Synced"),
        ("player.lyricsPlainChip", "Sin sincronizar", "Not synced"),
        ("common.retry", "Reintentar", "Retry"),
        ("queue.tab.all", "Todo", "All"),
        ("queue.tab.searched", "Resultados", "Results"),
        ("queue.tab.continue", "Próximas", "Up next"),
        ("queue.tab.autoplaying", "Recomendadas", "Recommended"),
        ("audio.equalizer", "Ecualizador", "Equalizer"),
        ("confirm.apply", "Aplicar", "Apply"),
        ("stream.title", "Información del audio", "Audio information"),
        ("stream.empty", "Sin datos todavía", "No data yet"),
        ("stream.unknown", "Sin datos todavía", "No data yet"),
        ("stream.route.builtin", "Reproductor integrado", "Built-in player"),
        ("stream.route.local", "Archivo local", "Local file"),
    ] {
        let es_hit: slint::SharedString = shell
            .global::<madrigal_ui::I18n>()
            .invoke_trl("es".into(), key.into());
        let en_hit: slint::SharedString = shell
            .global::<madrigal_ui::I18n>()
            .invoke_trl("en".into(), key.into());
        assert_eq!(es_hit.as_str(), es, "es {key}");
        assert_eq!(en_hit.as_str(), en, "en {key}");
    }

    let queue = fixture_queue();
    let tabs = queue_tabs_from_queue(&queue, "song-a");
    assert_eq!(tabs.all.len(), 5);
    assert_eq!(tabs.searched.len(), 2);
    assert_eq!(tabs.cont.len(), 4);
    assert!(tabs.auto.is_empty());
    assert!(tabs.all[0].current);
    assert_eq!(tabs.all[4].title, "Canción A");
    assert_eq!(tabs.all[4].artist, "Artista A");
    assert_eq!(tabs.all[4].album, "Álbum A");
    assert_eq!(tabs.all[4].duration_secs, Some(245));
    assert!(tabs.all[4].thumbnail.is_some());
    let by_title = queue_tabs_from_queue(&queue, "canción");
    assert_eq!(by_title.searched.len(), 1);
    let by_artist = queue_tabs_from_queue(&queue, "artista a");
    assert_eq!(by_artist.searched.len(), 1);

    let state = PlaybackUiState {
        title: "Titulo".to_owned(),
        artist: "Artista".to_owned(),
        album: "Album".to_owned(),
        position_ms: 30_000,
        duration_ms: 240_000,
        paused: true,
        liked: false,
        repeat: madrigal_settings::RepeatMode::Off,
        shuffle: false,
        volume: 0.7,
        background: PlayerBackgroundStyle::Gradient,
        queue_tab: 0,
        queue_locked: false,
        queue: QueueTabData::default(),
        lyrics_font_px: 16.0,
        lyrics_spacing: 1.5,
        track_id: "video-1".to_owned(),
        track_source: madrigal_core::TrackSource::Youtube,
        queue_index: 1,
    };
    apply_playback_state(&shell, &state);
    assert!(shell.get_has_track());
    assert_eq!(shell.get_track_title().as_str(), "Titulo");
    assert_eq!(shell.get_position_ms(), 30_000);
    assert_eq!(shell.get_bg_style(), 1);
    assert!(!shell.get_full_player_open());

    kit::click_by_id(&shell, "NowPlayingBar::bar-play").unwrap();
    assert_eq!(shell.get_play_toggle_count(), 1);
    assert!(!shell.get_paused());
    shell.set_paused(true);
    kit::click_by_id(&shell, "NowPlayingBar::bar-next").unwrap();
    kit::click_by_id(&shell, "NowPlayingBar::bar-prev").unwrap();
    assert_eq!(shell.get_next_count(), 1);
    assert_eq!(shell.get_prev_count(), 1);
    let bar_like = kit::find_by_id(&shell, "NowPlayingBar::bar-like").unwrap();
    assert_eq!(bar_like.accessible_label().unwrap().as_str(), "Agregar a favoritos");
    kit::click_by_id(&shell, "NowPlayingBar::bar-like").unwrap();
    assert!(shell.get_liked());
    assert_eq!(shell.get_like_count(), 1);
    assert_eq!(bar_like.accessible_label().unwrap().as_str(), "Quitar de favoritos");
    kit::click_by_id(&shell, "NowPlayingBar::bar-queue").unwrap();
    assert!(shell.get_queue_open());
    assert_eq!(shell.get_queue_open_count(), 1);
    shell.invoke_queue_closed();
    assert!(!shell.get_queue_open());

    assert!(dispatch_key(&shell, "f"));
    assert!(shell.get_full_player_open());
    assert_eq!(shell.get_full_player_count(), 1);
    let full_like = kit::find_by_id(&shell, "FullPlayer::full-like").unwrap();
    assert_eq!(full_like.accessible_label().unwrap().as_str(), "Quitar de favoritos");
    shell.set_liked(false);
    assert_eq!(full_like.accessible_label().unwrap().as_str(), "Agregar a favoritos");
    shell.set_liked(true);

    for (bg, style) in [
        (madrigal_settings::PlayerBackground::Default, 0),
        (madrigal_settings::PlayerBackground::Gradient, 1),
        (madrigal_settings::PlayerBackground::Blur, 2),
        (madrigal_settings::PlayerBackground::Glow, 3),
    ] {
        let appearance = madrigal_settings::Appearance {
            player_background: bg,
            ..Default::default()
        };
        apply_background_style(&shell, &appearance);
        assert_eq!(shell.get_bg_style(), style);
    }

    let lyrics_settings = madrigal_settings::Lyrics {
        font_size: 20.0,
        line_spacing: 2.0,
        ..Default::default()
    };
    apply_lyrics_settings(&shell, &lyrics_settings);
    assert_eq!(shell.get_lyrics_font_px(), 20.0);
    assert_eq!(shell.get_lyrics_spacing(), 2.0);

    let player_settings = madrigal_settings::Player::default();
    apply_player_settings(&shell, &player_settings);
    assert_eq!(shell.get_eq_gains().row_count(), 10);
    assert_eq!(shell.get_repeat_mode(), 0);

    shell.set_paused(true);
    drag_to(&shell, "PlayerConsole::seek-touch", 0, 0.5);
    assert_eq!(shell.get_position_ms(), 120_000);
    assert_eq!(shell.get_seek_last_ms(), 120_000);
    assert!(shell.get_seek_count() >= 1);
    drag_to(&shell, "PlayerConsole::vol-touch", 0, 0.25);
    assert!((shell.get_volume() - 0.25).abs() < 0.05);
    assert!(shell.get_volume_count() >= 1);

    kit::click_by_id(&shell, "PlayerConsole::full-repeat").unwrap();
    assert_eq!(shell.get_repeat_mode(), 1);
    kit::click_by_id(&shell, "PlayerConsole::full-repeat").unwrap();
    assert_eq!(shell.get_repeat_mode(), 2);
    kit::click_by_id(&shell, "PlayerConsole::full-shuffle").unwrap();
    assert!(shell.get_shuffle());

    let lyrics = fixture_lyrics();
    set_lyrics(&shell, &lyrics);
    assert_eq!(shell.get_lyric_rows().row_count(), 3);
    assert_eq!(
        shell.get_lyric_rows().row_data(1).unwrap().translation.as_str(),
        "World"
    );
    let png = render_share_png(&lyrics, "Titulo", "Artista", Some(1));
    assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);

    kit::click_by_id(&shell, "PlayerTools::full-lyrics-btn").unwrap();
    assert_eq!(shell.get_player_panel(), 1);
    assert_eq!(
        kit::find_by_id(&shell, "LyricsView::offset-minus")
            .unwrap()
            .accessible_label()
            .unwrap()
            .as_str(),
        "Desfase de la letra −0,1 s"
    );
    assert_eq!(
        kit::find_by_id(&shell, "LyricsView::lyrics-offset-help")
            .unwrap()
            .accessible_label()
            .unwrap()
            .as_str(),
        "Ajustá el tiempo si la letra va adelantada o atrasada"
    );
    assert_eq!(
        kit::find_by_id(&shell, "LyricsView::lyrics-share")
            .unwrap()
            .accessible_label()
            .unwrap()
            .as_str(),
        "Compartir letra"
    );

    let session = session_lyrics();
    set_lyrics(&shell, &session);
    assert_eq!(shell.get_lyric_rows().row_count(), 4);
    assert_eq!(shell.get_lyric_rows().row_data(1).unwrap().words.row_count(), 2);

    shell.invoke_seek_to_requested(0);
    assert_eq!(shell.get_position_ms(), 0);
    assert_eq!(shell.get_active_line(), 0);
    let ticks_before = shell.get_lyrics_clock_ticks();
    shell.set_paused(false);
    kit::mock_advance(Duration::from_millis(2_100));
    assert!(
        shell.get_lyrics_clock_ticks() > ticks_before + 100,
        "clock did not tick: {} -> {}",
        ticks_before,
        shell.get_lyrics_clock_ticks()
    );
    assert!((shell.get_position_ms() - 2_100).abs() <= 32, "position {}", shell.get_position_ms());
    assert_eq!(shell.get_active_line(), 1);
    assert_eq!(shell.get_active_word(), 0);
    kit::mock_advance(Duration::from_millis(100));
    assert!(shell.get_word_progress() > 0.0 && shell.get_word_progress() < 1.0);
    kit::mock_advance(Duration::from_millis(3_900));
    assert_eq!(shell.get_active_line(), 2);
    kit::mock_advance(Duration::from_millis(6_000));
    assert_eq!(shell.get_active_line(), 3);

    let paused_ticks = shell.get_lyrics_clock_ticks();
    let paused_pos = shell.get_position_ms();
    shell.invoke_play_pause_toggled();
    assert!(shell.get_paused());
    kit::mock_advance(Duration::from_millis(1_000));
    assert_eq!(shell.get_lyrics_clock_ticks(), paused_ticks, "clock kept ticking while paused");
    assert_eq!(shell.get_position_ms(), paused_pos, "position moved while paused");
    assert_eq!(shell.get_active_line(), 3);

    shell.invoke_seek_to_requested(1_600);
    assert_eq!(shell.get_active_line(), 0);
    for _ in 0..5 {
        kit::click_by_id(&shell, "LyricsView::offset-plus").unwrap();
    }
    assert_eq!(shell.get_lyrics_offset_ms(), 500);
    assert_eq!(shell.get_lyrics_offset_count(), 5);
    assert_eq!(
        kit::find_by_id(&shell, "LyricsView::offset-value")
            .unwrap()
            .accessible_label()
            .unwrap()
            .as_str(),
        "Desfase de la letra +0,5 s"
    );
    assert_eq!(shell.get_active_line(), 1, "offset did not move the line boundary");
    kit::click_by_id(&shell, "LyricsView::offset-minus").unwrap();
    assert_eq!(shell.get_lyrics_offset_ms(), 400);
    assert_eq!(shell.get_active_line(), 1);
    shell.invoke_lyrics_offset_changed(-400);
    assert_eq!(shell.get_lyrics_offset_ms(), 0);
    assert_eq!(shell.get_active_line(), 0);

    let list = kit::find_by_id(&shell, "LyricsView::lyrics-list").unwrap();
    assert!(!shell.get_lyrics_scroll_locked());
    list.scroll(0.0, -160.0);
    kit::mock_advance(Duration::from_millis(400));
    assert!(shell.get_lyrics_scroll_locked(), "manual scroll did not pause auto scroll");
    let back: Vec<_> =
        kit::ElementHandle::find_by_element_id(&shell, "LyricsView::lyrics-back").collect();
    assert_eq!(back.len(), 1);
    assert!(back[0].size().height > 0.0, "back button hidden");
    kit::click(&back[0]);
    assert!(!shell.get_lyrics_scroll_locked(), "back button kept auto scroll paused");
    kit::mock_advance(Duration::from_millis(400));

    list.scroll(0.0, -200.0);
    kit::mock_advance(Duration::from_millis(400));
    assert!(shell.get_lyrics_scroll_locked());
    kit::mock_advance(Duration::from_millis(4_000));
    assert!(!shell.get_lyrics_scroll_locked(), "auto scroll did not resume after 4s");
    kit::mock_advance(Duration::from_millis(400));

    let lines: Vec<_> = kit::ElementHandle::find_by_element_id(&shell, "LyricsView::line-touch").collect();
    assert_eq!(lines.len(), 4);
    kit::click(&lines[1]);
    assert_eq!(shell.get_lyrics_seek_ms(), 2000);
    assert_eq!(shell.get_lyrics_seek_count(), 1);
    assert!(!shell.get_lyrics_scroll_locked());
    kit::mock_advance(Duration::from_millis(400));

    let list_pos = list.absolute_position();
    let list_size = list.size();
    let active = shell.get_active_line() as usize;
    let line_pos = lines[active].absolute_position();
    let third = list_size.height / 3.0;
    assert!(
        (line_pos.y - list_pos.y - third).abs() < 12.0,
        "active line at {} instead of {}",
        line_pos.y - list_pos.y,
        third
    );

    let share_path = tmp.path().join("share.png");
    std::env::set_var("MADRIGAL_SHARE_OUT", &share_path);
    kit::click_by_id(&shell, "LyricsView::lyrics-share").unwrap();
    assert_eq!(shell.get_lyrics_share_count(), 1);
    let bytes = std::fs::read(&share_path).unwrap();
    assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);

    apply_playback_state(
        &shell,
        &PlaybackUiState { queue: queue_tabs_from_queue(&queue, "song-a"), ..state.clone() },
    );
    shell.set_queue_tab(0);
    shell.invoke_queue_opened();
    assert!(shell.get_queue_open());
    let tab_handles: Vec<_> =
        kit::ElementHandle::find_by_element_id(&shell, "QueueDrawer::tab-touch").collect();
    assert_eq!(tab_handles.len(), 4);
    for (tab, rows) in [(1, 2), (2, 4), (3, 0), (0, 5)] {
        kit::click(&tab_handles[tab]);
        assert_eq!(shell.get_queue_tab(), tab as i32);
        let visible: Vec<_> =
            kit::ElementHandle::find_by_element_id(&shell, "QueueDrawer::row-touch").collect();
        assert_eq!(visible.len(), rows, "tab {tab}");
    }

    let row_handles: Vec<_> =
        kit::ElementHandle::find_by_element_id(&shell, "QueueDrawer::row-touch").collect();
    assert_eq!(row_handles.len(), 5);

    kit::click(&row_handles[0]);
    assert_eq!(shell.get_queue_play_count(), 1);
    assert_eq!(shell.get_queue_play_last(), 0);
    let remove_handles: Vec<_> =
        kit::ElementHandle::find_by_element_id(&shell, "QueueDrawer::row-remove").collect();
    assert_eq!(remove_handles.len(), 5);
    kit::click(&remove_handles[0]);
    assert_eq!(shell.get_queue_remove_count(), 1);
    let row_handles: Vec<_> =
        kit::ElementHandle::find_by_element_id(&shell, "QueueDrawer::row-touch").collect();
    assert_eq!(row_handles.len(), 4);

    let first_title = shell.get_queue_all().row_data(0).unwrap().title.to_string();
    let pos = row_handles[0].absolute_position();
    let target = LogicalPosition::new(pos.x + 20.0, pos.y + 130.0);
    row_handles[0].mock_drag(target, slint::platform::PointerEventButton::Left);
    assert_eq!(shell.get_queue_move_count(), 1);
    assert_eq!(shell.get_queue_move_from(), 0);
    assert_eq!(shell.get_queue_move_to(), 2);
    assert_eq!(shell.get_queue_all().row_data(2).unwrap().title.as_str(), first_title.as_str());

    kit::click_by_id(&shell, "QueueDrawer::queue-lock").unwrap();
    assert!(shell.get_queue_locked());
    let moves_before = shell.get_queue_move_count();
    shell.invoke_queue_move_requested(0, 1);
    assert_eq!(shell.get_queue_move_count(), moves_before);
    kit::click_by_id(&shell, "QueueDrawer::queue-lock").unwrap();
    assert!(!shell.get_queue_locked());

    open_player_dialog(&shell, PlayerDialog::Sleep);
    assert_eq!(shell.get_player_dialog(), 1);
    let sleep_buttons: Vec<_> =
        kit::ElementHandle::find_by_element_id(&shell, "SleepTimerDialog::sleep-opt").collect();
    assert_eq!(sleep_buttons.len(), 3);
    for (button, label) in sleep_buttons.iter().zip(["15 min", "30 min", "60 min"]) {
        assert_eq!(button.accessible_label().unwrap().as_str(), label);
    }
    let status = kit::find_by_id(&shell, "SleepTimerDialog::sleep-status").unwrap();
    assert_eq!(status.accessible_label().unwrap().as_str(), "Desactivado");
    let sleep_opts: Vec<_> =
        kit::ElementHandle::find_by_element_id(&shell, "SleepTimerDialog::sleep-opt-touch").collect();
    assert_eq!(sleep_opts.len(), 3);
    kit::click(&sleep_opts[0]);
    assert_eq!(shell.get_sleep_mode(), 1);
    assert_eq!(shell.get_sleep_minutes(), 15);
    assert_eq!(shell.get_sleep_count(), 1);
    assert_eq!(status.accessible_label().unwrap().as_str(), "Se apaga en 15 min");
    kit::click_by_id(&shell, "SleepTimerDialog::sleep-eos").unwrap();
    assert_eq!(shell.get_sleep_mode(), 2);
    assert_eq!(status.accessible_label().unwrap().as_str(), "Al final de la canción");
    assert_eq!(
        kit::find_by_id(&shell, "SleepTimerDialog::sleep-off-btn")
            .unwrap()
            .accessible_label()
            .unwrap()
            .as_str(),
        "Desactivar"
    );
    kit::click_by_id(&shell, "SleepTimerDialog::sleep-off-btn").unwrap();
    assert_eq!(shell.get_sleep_mode(), 0);
    assert_eq!(status.accessible_label().unwrap().as_str(), "Desactivado");
    assert!(dispatch_key(&shell, "Escape"));
    assert_eq!(shell.get_player_dialog(), 0);

    shell.invoke_player_dialog_requested(2);
    assert_eq!(shell.get_player_dialog(), 2);
    assert_eq!(shell.get_dialog_count(), 1);
    drag_to(&shell, "TempoPitchDialog::tempo-semi", 0, 0.75);
    assert_eq!(shell.get_tempo_semitones(), 6.0);
    assert!(shell.get_tempo_count() >= 1);
    kit::click_by_id(&shell, "TempoPitchDialog::tempo-reset-btn").unwrap();
    assert_eq!(shell.get_tempo_semitones(), 0.0);
    assert_eq!(shell.get_tempo_mult(), 1.0);
    shell.window().dispatch_event(slint::platform::WindowEvent::KeyPressed {
        text: slint::platform::Key::Escape.into(),
    });
    assert_eq!(shell.get_player_dialog(), 0);

    shell.invoke_player_dialog_requested(3);
    assert_eq!(shell.get_player_dialog(), 3);
    assert_eq!(shell.get_dialog_count(), 2);
    let labels = eq_band_labels();
    assert_eq!(
        labels,
        ["31 Hz", "63 Hz", "125 Hz", "250 Hz", "500 Hz", "1 kHz", "2 kHz", "4 kHz", "8 kHz", "16 kHz"]
    );
    let labels = shell.get_eq_band_labels();
    assert_eq!(labels.row_count(), 10);
    assert_eq!(labels.row_data(0).unwrap().as_str(), "31 Hz");
    assert_eq!(labels.row_data(9).unwrap().as_str(), "16 kHz");
    let sliders: Vec<_> =
        kit::ElementHandle::find_by_element_id(&shell, "EqualizerDialog::eq-band").collect();
    assert_eq!(sliders.len(), 10);
    assert_eq!(sliders[0].accessible_label().unwrap().as_str(), "31 Hz · 0 dB");
    assert_eq!(
        kit::ElementHandle::find_by_element_id(&shell, "M3Slider::value-label").count(),
        0
    );
    assert!(!shell.get_eq_on());
    let presets: Vec<_> =
        kit::ElementHandle::find_by_element_id(&shell, "EqualizerDialog::eq-preset-touch").collect();
    assert_eq!(presets.len(), 4);
    kit::click(&presets[1]);
    assert!(shell.get_eq_on(), "preset did not enable the equalizer");
    assert_eq!(shell.get_eq_preset().as_str(), "bass");
    let gains = shell.get_eq_gains();
    assert!((gains.row_data(0).unwrap() - 17.0 / 24.0).abs() < 0.001);
    kit::mock_advance(Duration::from_millis(50));
    assert_eq!(sliders[0].accessible_label().unwrap().as_str(), "31 Hz · +5 dB");
    let eq_before = shell.get_eq_count();
    drag_to(&shell, "EqualizerDialog::eq-band", 0, 0.9);
    assert!(shell.get_eq_count() > eq_before);
    assert!((shell.get_eq_gains().row_data(0).unwrap() - 0.9).abs() < 0.05);
    kit::click_by_id(&shell, "EqualizerDialog::eq-apply").unwrap();
    assert_eq!(shell.get_player_dialog(), 0);

    show_error(&shell, "E404", "sin stream");
    assert_eq!(shell.get_player_dialog(), 4);
    assert_eq!(shell.get_error_code().as_str(), "E404");
    assert_eq!(shell.get_error_message().as_str(), "sin stream");
    kit::click_by_id(&shell, "PlaybackErrorDialog::error-retry-btn").unwrap();
    assert_eq!(shell.get_error_retry_count(), 1);
    assert!(dispatch_key(&shell, "Escape"));
    assert_eq!(shell.get_player_dialog(), 0);

    use madrigal_player::stream_info::{PlaybackState, StreamInfo, StreamOrigin};
    assert_eq!(sample_rate_text(44_100), "44.1 kHz");
    assert_eq!(sample_rate_text(48_000), "48 kHz");
    assert_eq!(resolution_route("innertube:visionos"), "innertube");
    assert_eq!(resolution_route("web_safari"), "web_safari");
    set_stream_info(
        &shell,
        Some(&StreamInfo {
            codec: Some("opus".to_owned()),
            abr_kbps: Some(160.0),
            sample_rate: Some(48000),
            media_id: "song-a1".to_owned(),
            loudness_hint: Some(-14.0),
            buffer_health: Some(80.0),
            speed: 1.0,
            state: PlaybackState::Playing,
            origin: Some(StreamOrigin::Streaming),
            resolution_path: Some("innertube:visionos".to_owned()),
        }),
    );
    shell.invoke_player_dialog_requested(5);
    assert_eq!(shell.get_player_dialog(), 5);
    assert!(shell.get_stream_has_data());
    assert_eq!(shell.get_stream_codec().as_str(), "opus");
    assert_eq!(shell.get_stream_bitrate().as_str(), "160 kbps");
    assert_eq!(shell.get_stream_rate().as_str(), "48 kHz");
    assert_eq!(shell.get_stream_quality().as_str(), "80");
    assert_eq!(shell.get_stream_origin(), 2);
    assert_eq!(shell.get_stream_resolution().as_str(), "innertube");
    assert_eq!(kit::ElementHandle::find_by_accessible_label(&shell, "Códec: opus").count(), 1);
    assert_eq!(
        kit::ElementHandle::find_by_accessible_label(&shell, "Origen: Streaming").count(),
        1
    );
    assert_eq!(
        kit::ElementHandle::find_by_accessible_label(&shell, "Ruta de resolución: Reproductor integrado").count(),
        1
    );
    set_stream_info(
        &shell,
        Some(&StreamInfo {
            codec: None,
            abr_kbps: None,
            sample_rate: None,
            media_id: "song-a1".to_owned(),
            loudness_hint: None,
            buffer_health: None,
            speed: 1.0,
            state: PlaybackState::Paused,
            origin: Some(StreamOrigin::Local),
            resolution_path: Some("local".to_owned()),
        }),
    );
    assert_eq!(
        kit::ElementHandle::find_by_accessible_label(&shell, "Códec: Sin datos todavía").count(),
        1
    );
    assert_eq!(
        kit::ElementHandle::find_by_accessible_label(&shell, "Origen: Archivo local o descarga").count(),
        1
    );
    assert_eq!(
        kit::ElementHandle::find_by_accessible_label(&shell, "Ruta de resolución: Archivo local").count(),
        1
    );
    assert!(dispatch_key(&shell, "Escape"));
    assert_eq!(shell.get_player_dialog(), 0);
    set_stream_info(&shell, None);
    assert!(!shell.get_stream_has_data());
    assert_eq!(shell.get_stream_codec().as_str(), "");
    shell.invoke_player_dialog_requested(5);
    let empty = kit::find_by_id(&shell, "StreamInfoPanel::info-empty").unwrap();
    assert_eq!(empty.accessible_label().unwrap().as_str(), "Sin datos todavía");
    assert!(dispatch_key(&shell, "Escape"));
    assert_eq!(shell.get_player_dialog(), 0);

    shell.invoke_queue_opened();
    assert!(shell.get_queue_open());
    assert!(dispatch_key(&shell, "Escape"));
    assert!(!shell.get_queue_open());
    assert!(shell.get_full_player_open());
    shell.window().dispatch_event(slint::platform::WindowEvent::KeyPressed {
        text: slint::platform::Key::Escape.into(),
    });
    assert!(!shell.get_full_player_open());
    assert_eq!(shell.get_player_dismiss_count(), 1);

    shell.invoke_full_player_closed();
    dispatch_key(&shell, "f");
    shell.set_paused(true);
    set_lyrics(&shell, &session);
    shell.invoke_seek_to_requested(0);
    shell.show().unwrap();
    kit::mock_advance(Duration::from_millis(400));

    let snap_dir = kit::snapshot_dir(tmp.path());
    let shot_0s = snap_dir.join("player-0s.png");
    kit::snapshot_to(shell.window(), &shot_0s).unwrap();

    shell.invoke_seek_to_requested(6_500);
    kit::mock_advance(Duration::from_millis(400));
    assert_eq!(shell.get_active_line(), 2);
    let shot_mid = snap_dir.join("player-mid.png");
    kit::snapshot_to(shell.window(), &shot_mid).unwrap();

    assert_player_layout(&shell, "player");

    let worded = word_lyrics_fixture();
    set_lyrics(&shell, &worded);
    assert_eq!(shell.get_lyric_rows().row_count(), 1);
    assert_eq!(shell.get_lyric_rows().row_data(0).unwrap().words.row_count(), 3);
    shell.invoke_seek_to_requested(10_900);
    kit::mock_advance(Duration::from_millis(400));
    assert_eq!(shell.get_active_line(), 0);
    assert_eq!(shell.get_active_word(), 1);
    assert!(
        shell.get_word_progress() > 0.1 && shell.get_word_progress() < 0.9,
        "word progress {}",
        shell.get_word_progress()
    );
    let shot_words = snap_dir.join("player-words.png");
    kit::snapshot_to(shell.window(), &shot_words).unwrap();

    let (pending_tx, pending_rx) = channel();
    let fetcher = std::rc::Rc::new(LyricsFetcher::new(
        Box::new(EmptyCache),
        Box::new(FakeSource { pending: Mutex::new(pending_tx) }),
        lyrics_delivery(&shell),
    ));
    wire_lyrics_fetch(&shell, fetcher);

    let fetch_state = PlaybackUiState {
        title: "Yesterday".to_owned(),
        artist: "The Beatles".to_owned(),
        album: "Help!".to_owned(),
        position_ms: 0,
        duration_ms: 125_000,
        paused: true,
        liked: false,
        repeat: madrigal_settings::RepeatMode::Off,
        shuffle: false,
        volume: 0.7,
        background: PlayerBackgroundStyle::Theme,
        queue_tab: 0,
        queue_locked: false,
        queue: QueueTabData::default(),
        lyrics_font_px: 16.0,
        lyrics_spacing: 1.5,
        track_id: "NrgmdOz227I".to_owned(),
        track_source: madrigal_core::TrackSource::Youtube,
        queue_index: 1,
    };

    apply_playback_state(
        &shell,
        &PlaybackUiState { track_id: String::new(), queue_index: 0, ..fetch_state.clone() },
    );
    assert_eq!(shell.get_lyrics_state(), 0, "idle state");
    assert_eq!(shell.get_lyric_rows().row_count(), 0);
    kit::mock_advance(Duration::from_millis(200));
    assert_player_layout(&shell, "lyrics idle");

    apply_playback_state(&shell, &fetch_state);
    assert_eq!(shell.get_lyrics_state(), 1, "searching state");
    assert_eq!(shell.get_lyrics_fetch_count(), 1);
    let respond = wait_for_request(&pending_rx, "NrgmdOz227I");
    kit::mock_advance(Duration::from_millis(400));
    let shot_searching = snap_dir.join("player-lyrics-searching.png");
    kit::snapshot_to(shell.window(), &shot_searching).unwrap();
    assert_player_layout(&shell, "lyrics searching");

    respond.send(Ok(Some(outcome(fixture_lyrics())))).unwrap();
    pump_until(Duration::from_secs(5), || shell.get_lyrics_state() == 2);
    assert!(shell.get_lyrics_synced(), "synced flag");
    assert_eq!(shell.get_lyric_rows().row_count(), 3);
    assert_eq!(shell.get_active_line(), 0, "synced lyrics lost the active line");
    kit::mock_advance(Duration::from_millis(400));
    assert_player_layout(&shell, "lyrics found synced");
    shell.invoke_seek_to_requested(700);
    kit::mock_advance(Duration::from_millis(400));
    assert_eq!(shell.get_active_line(), 0);
    assert_eq!(shell.get_active_word(), 1);
    let shot_found = snap_dir.join("player-lyrics-found.png");
    kit::snapshot_to(shell.window(), &shot_found).unwrap();

    apply_playback_state(
        &shell,
        &PlaybackUiState { track_id: "missing-track".to_owned(), queue_index: 2, ..fetch_state.clone() },
    );
    assert_eq!(shell.get_lyrics_state(), 1);
    let respond = wait_for_request(&pending_rx, "missing-track");
    respond.send(Ok(None)).unwrap();
    pump_until(Duration::from_secs(5), || shell.get_lyrics_state() == 3);
    assert_eq!(shell.get_lyric_rows().row_count(), 0);
    kit::mock_advance(Duration::from_millis(200));
    let shot_notfound = snap_dir.join("player-lyrics-notfound.png");
    kit::snapshot_to(shell.window(), &shot_notfound).unwrap();
    assert_player_layout(&shell, "lyrics not found");
    let retry = kit::find_by_id(&shell, "LyricsView::lyrics-retry").expect("retry button");
    assert!(retry.size().height > 0.0, "retry button hidden");

    kit::click(&retry);
    assert_eq!(shell.get_lyrics_state(), 1, "retry starts a new search");
    assert_eq!(shell.get_lyrics_retry_count(), 1);
    assert_eq!(shell.get_lyrics_fetch_count(), 3);
    let respond = wait_for_request(&pending_rx, "missing-track");
    respond.send(Ok(Some(outcome(plain_lyrics())))).unwrap();
    pump_until(Duration::from_secs(5), || shell.get_lyrics_state() == 2);
    assert!(!shell.get_lyrics_synced(), "plain flag");
    assert_eq!(shell.get_lyric_rows().row_count(), 2);
    shell.invoke_seek_to_requested(2_000);
    assert_eq!(shell.get_active_line(), -1, "plain lyrics got a live highlight");
    shell.invoke_play_pause_toggled();
    let paused_at = shell.get_position_ms();
    kit::mock_advance(Duration::from_millis(600));
    assert!(shell.get_position_ms() > paused_at, "clock did not run");
    assert_eq!(shell.get_active_line(), -1, "clock moved the line on plain lyrics");
    shell.invoke_play_pause_toggled();
    kit::mock_advance(Duration::from_millis(200));
    let shot_plain = snap_dir.join("player-lyrics-plain.png");
    kit::snapshot_to(shell.window(), &shot_plain).unwrap();
    assert_player_layout(&shell, "lyrics found plain");

    apply_playback_state(
        &shell,
        &PlaybackUiState { track_id: "broken-track".to_owned(), queue_index: 3, ..fetch_state.clone() },
    );
    let respond = wait_for_request(&pending_rx, "broken-track");
    respond.send(Err("provider timeout".to_owned())).unwrap();
    pump_until(Duration::from_secs(5), || shell.get_lyrics_state() == 4);
    assert_eq!(shell.get_lyrics_error().as_str(), "provider timeout");
    kit::mock_advance(Duration::from_millis(200));
    let shot_error = snap_dir.join("player-lyrics-error.png");
    kit::snapshot_to(shell.window(), &shot_error).unwrap();
    assert_player_layout(&shell, "lyrics error");

    apply_playback_state(
        &shell,
        &PlaybackUiState { track_id: "stale-track".to_owned(), queue_index: 4, ..fetch_state.clone() },
    );
    let respond_stale = wait_for_request(&pending_rx, "stale-track");
    apply_playback_state(
        &shell,
        &PlaybackUiState { track_id: "fresh-track".to_owned(), queue_index: 5, ..fetch_state.clone() },
    );
    let respond_fresh = wait_for_request(&pending_rx, "fresh-track");
    respond_stale.send(Ok(Some(outcome(line_lyrics("Stale line"))))).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let _ = slint::quit_event_loop();
    slint::run_event_loop().unwrap();
    assert_eq!(shell.get_lyrics_state(), 1, "stale response changed the state");
    assert_eq!(shell.get_lyric_rows().row_count(), 0, "stale response rendered lyrics");
    respond_fresh.send(Ok(Some(outcome(line_lyrics("Fresh line"))))).unwrap();
    pump_until(Duration::from_secs(5), || shell.get_lyrics_state() == 2);
    assert_eq!(shell.get_lyric_rows().row_data(0).unwrap().text.as_str(), "Fresh line");

    for shot in [
        &shot_0s,
        &shot_mid,
        &shot_words,
        &shot_searching,
        &shot_found,
        &shot_notfound,
        &shot_plain,
        &shot_error,
    ] {
        let bytes = std::fs::read(shot).unwrap();
        assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        assert!(bytes.len() > 4096, "snapshot too small: {}", bytes.len());
    }
}
