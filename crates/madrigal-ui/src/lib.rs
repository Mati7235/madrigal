slint::include_modules!();

mod pages_library;
mod shell;
mod theme;

pub use pages_library::{
    LibraryFixture, album_cards, artist_cards, chip_order, default_chip_order, download_models,
    download_state_index, event_years, history_rows, load_downloads_fixture, load_library_fixture,
    move_chip, playlist_rows, save_chip_order, song_rows, song_sort_for_settings, stats_streaks,
    stats_totals, top_album_rows, top_artist_rows, top_track_rows, year_index, year_model,
    DownloadItem, CHIP_ALBUMS, CHIP_ARTISTS, CHIP_HISTORY, CHIP_LIBRARY, CHIP_PLAYLISTS, CHIP_SONGS,
    CHIP_STATS,
};

pub use shell::{
    apply_shell_appearance, apply_shell_theme, dispatch_key, overlay_from_index, overlay_index,
    page_from_index, page_index, set_shell_lang, sync_queue_seam, wire_shell_i18n, ShellOverlay,
    ShellPage,
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
