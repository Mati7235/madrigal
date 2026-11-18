#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeKind {
    Light,
    Dark,
    PureBlack,
}

pub fn theme_index(kind: ThemeKind) -> i32 {
    match kind {
        ThemeKind::Light => 0,
        ThemeKind::Dark => 1,
        ThemeKind::PureBlack => 2,
    }
}

pub fn theme_from_index(index: i32) -> ThemeKind {
    match index {
        0 => ThemeKind::Light,
        2 => ThemeKind::PureBlack,
        _ => ThemeKind::Dark,
    }
}

pub fn from_appearance(appearance: &madrigal_settings::Appearance) -> ThemeKind {
    match appearance.theme {
        madrigal_settings::Theme::Light => ThemeKind::Light,
        madrigal_settings::Theme::Dark => {
            if appearance.pure_black {
                ThemeKind::PureBlack
            } else {
                ThemeKind::Dark
            }
        }
        madrigal_settings::Theme::System => ThemeKind::Dark,
        madrigal_settings::Theme::Matugen => ThemeKind::Dark,
    }
}

pub fn lang_code(lang: madrigal_i18n::Lang) -> &'static str {
    match lang {
        madrigal_i18n::Lang::Es => "es",
        madrigal_i18n::Lang::En => "en",
    }
}

pub fn lang_from_code(code: &str) -> madrigal_i18n::Lang {
    match code {
        "en" => madrigal_i18n::Lang::En,
        _ => madrigal_i18n::Lang::Es,
    }
}
