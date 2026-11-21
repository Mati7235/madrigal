slint::include_modules!();

mod artwork;
mod lyrics_clock;
mod lyrics_fetch;
mod pages_catalog;
mod pages_library;
mod pages_settings;
mod player_ui;
mod shell;
mod theme;

pub use artwork::{apply_cover, to_slint_image};

pub use pages_catalog::{
    album_results, apply_album_detail, apply_artist_detail, apply_browse, apply_browse_detail,
    apply_charts_entry, apply_home, apply_home_chip, apply_home_chips, apply_moods, apply_pins,
    apply_playlist_detail, apply_podcast_detail, apply_releases, apply_search_results,
    apply_suggestions, artist_results, episode_item, home_chips, home_sections, load_album_fixture,
    load_artist_fixture, load_browse_fixture, load_explore_fixtures, load_home_fixture,
    load_playlist_fixture, load_search_fixture, load_suggestions_fixture, playlist_item,
    quick_pick_items, song_results, track_item, CatalogResult,
};

pub use pages_library::{
    apply_library, album_cards, artist_cards, chip_order, default_chip_order, download_models,
    download_state_index, downloaded_rows, event_years, filter_cards, filter_downloads,
    filter_history, filter_playlists, filter_songs, history_rows, load_downloads_fixture,
    load_library_fixture, matches_query, move_chip, playlist_rows, save_chip_order, song_rows,
    song_sort_for_settings, stats_streaks, stats_totals, top_album_rows, top_artist_rows,
    top_track_rows, year_index, year_model, DownloadItem, LibraryFixture, LibraryModels,
    CHIP_ALBUMS, CHIP_ARTISTS, CHIP_HISTORY, CHIP_LIBRARY, CHIP_PLAYLISTS, CHIP_SONGS, CHIP_STATS,
};

pub use player_ui::{
    active_lyric_at, active_word_at, apply_background_style, apply_lyrics_event,
    apply_lyrics_settings, apply_playback_state, apply_player_settings, background_from_appearance,
    background_index, chain_fetcher, clear_lyrics, dialog_index, eq_preset_gains,
    lyric_progress_at, lyrics_delivery, maybe_fetch_lyrics, open_player_dialog, queue_tabs_from_queue,
    render_share_png, repeat_from_index, repeat_index, set_lyrics, set_queue_models,
    set_stream_info, show_error, sync_lyrics_clock, track_source_from_index, track_source_index,
    wire_lyrics_clock, wire_lyrics_fetch, wire_player_callbacks, PlaybackUiState,
    PlayerBackgroundStyle, PlayerDialog, QueueRowData, QueueTabData,
};
pub use lyrics_clock::{LyricsClock, MAX_OFFSET_MS};
pub use lyrics_fetch::{
    CachedLyrics, ChainSource, DbLyricsCache, FetchTask, Generation, LyricsCache, LyricsEvent,
    LyricsFetcher, LyricsSource,
};

pub use pages_settings::{
    all_groups, apply_to_file, apply_value, detect_po_provider, field_count, group_from_index,
    group_index, group_key, group_title_key, group_titles, kind_tag, latencies_from_marks,
    refresh_settings, settings_section, wire_shell, SettingsGroup, SettingKind, SettingRow,
    SettingsPresenter, SettingsValue, DIAG_INDEX, GROUP_COUNT, MASKED_SECRET,
};
pub use shell::{
    apply_shell_appearance, apply_shell_theme, close_detail, dispatch_key, is_detail_open,
    overlay_from_index, overlay_index, page_from_index, page_index, set_shell_lang,
    sync_queue_seam, wire_shell_i18n, ShellOverlay, ShellPage,
};
pub use theme::{
    from_appearance, lang_code, lang_from_code, theme_from_index, theme_index, ThemeKind,
};

use std::cell::Cell;
use std::rc::Rc;

pub fn apply_theme(gallery: &DemoGallery, kind: ThemeKind) {
    gallery.invoke_apply_theme(theme_index(kind));
}

pub fn wire_i18n(
    gallery: &DemoGallery,
    initial: madrigal_i18n::Lang,
) -> Rc<Cell<madrigal_i18n::Lang>> {
    gallery.global::<I18n>().on_trl(|lang, key| {
        madrigal_i18n::tr(lang_from_code(lang.as_str()), key.as_str()).into()
    });
    let slot = Rc::new(Cell::new(initial));
    set_lang(gallery, &slot, initial);
    slot
}

pub fn set_lang(
    gallery: &DemoGallery,
    slot: &Rc<Cell<madrigal_i18n::Lang>>,
    lang: madrigal_i18n::Lang,
) {
    slot.set(lang);
    gallery.global::<I18n>().set_lang(lang_code(lang).into());
}
