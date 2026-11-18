use slint::ComponentHandle;
use std::cell::Cell;
use std::rc::Rc;

use crate::{from_appearance, lang_code, lang_from_code, theme_index, I18n, MainShell, ThemeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellPage {
    Home,
    Search,
    Explore,
    Library,
    Downloads,
    Settings,
}

pub fn page_index(page: ShellPage) -> i32 {
    match page {
        ShellPage::Home => 0,
        ShellPage::Search => 1,
        ShellPage::Explore => 2,
        ShellPage::Library => 3,
        ShellPage::Downloads => 4,
        ShellPage::Settings => 5,
    }
}

pub fn page_from_index(index: i32) -> ShellPage {
    match index {
        1 => ShellPage::Search,
        2 => ShellPage::Explore,
        3 => ShellPage::Library,
        4 => ShellPage::Downloads,
        5 => ShellPage::Settings,
        _ => ShellPage::Home,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellOverlay {
    None,
    Confirm,
    Prompt,
}

pub fn overlay_index(overlay: ShellOverlay) -> i32 {
    match overlay {
        ShellOverlay::None => 0,
        ShellOverlay::Confirm => 1,
        ShellOverlay::Prompt => 2,
    }
}

pub fn overlay_from_index(index: i32) -> ShellOverlay {
    match index {
        1 => ShellOverlay::Confirm,
        2 => ShellOverlay::Prompt,
        _ => ShellOverlay::None,
    }
}

pub fn apply_shell_theme(shell: &MainShell, kind: ThemeKind) {
    shell.invoke_apply_theme(theme_index(kind));
}

pub fn wire_shell_i18n(
    shell: &MainShell,
    initial: madrigal_i18n::Lang,
) -> Rc<Cell<madrigal_i18n::Lang>> {
    shell.global::<I18n>().on_trl(|lang, key| {
        madrigal_i18n::tr(lang_from_code(lang.as_str()), key.as_str()).into()
    });
    let slot = Rc::new(Cell::new(initial));
    set_shell_lang(shell, &slot, initial);
    slot
}

pub fn set_shell_lang(
    shell: &MainShell,
    slot: &Rc<Cell<madrigal_i18n::Lang>>,
    lang: madrigal_i18n::Lang,
) {
    slot.set(lang);
    shell.global::<I18n>().set_lang(lang_code(lang).into());
}

pub fn apply_shell_appearance(shell: &MainShell, appearance: &madrigal_settings::Appearance) {
    apply_shell_theme(shell, from_appearance(appearance));
}

pub fn dispatch_key(shell: &MainShell, key: &str) -> bool {
    shell.invoke_handle_key(key.into())
}

pub fn sync_queue_seam(shell: &MainShell, queue: &madrigal_player::queue::Queue) {
    shell.set_queue_count(queue.len() as i32);
}
