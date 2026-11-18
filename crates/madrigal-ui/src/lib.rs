slint::include_modules!();

mod theme;

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
