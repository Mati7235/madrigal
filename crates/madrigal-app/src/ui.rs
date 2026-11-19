use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use madrigal_i18n::Lang;
use madrigal_settings::{Language, Settings};
use madrigal_ui::{
    apply_shell_appearance, from_appearance, lang_from_code, set_shell_lang, theme_index,
    wire_shell_i18n, MainShell, SettingsPresenter,
};
use slint::ComponentHandle;

pub struct Ui {
    pub shell: MainShell,
    pub lang: Rc<Cell<Lang>>,
    pub presenter: Rc<RefCell<SettingsPresenter>>,
    pub appearance_sync: slint::Timer,
}

pub fn lang_from_settings(language: Language) -> Lang {
    lang_from_code(match language {
        Language::En => "en",
        Language::Es => "es",
    })
}

pub fn boot_ui() -> Result<Ui, String> {
    madrigal_paths::ensure_dirs().map_err(|err| format!("ensure_dirs: {err}"))?;
    let shell = MainShell::new().map_err(|err| err.to_string())?;
    let settings = Settings::load();
    let path = madrigal_paths::settings_path();
    if !path.is_file() {
        settings
            .save_to(&path)
            .map_err(|err| format!("settings: {err}"))?;
    }
    let lang = wire_shell_i18n(&shell, lang_from_settings(settings.appearance.language));
    apply_shell_appearance(&shell, &settings.appearance);
    let presenter = Rc::new(RefCell::new(SettingsPresenter::load(
        &path,
        Vec::new(),
        &madrigal_paths::data_dir().join("diag-logs"),
    )));
    madrigal_ui::wire_shell(&shell, presenter.clone(), &lang);
    let appearance_sync = start_appearance_sync(&shell, &presenter, &lang);
    Ok(Ui {
        shell,
        lang,
        presenter,
        appearance_sync,
    })
}

fn start_appearance_sync(
    shell: &MainShell,
    presenter: &Rc<RefCell<SettingsPresenter>>,
    lang: &Rc<Cell<Lang>>,
) -> slint::Timer {
    let weak = shell.as_weak();
    let presenter = presenter.clone();
    let lang = lang.clone();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(250),
        move || {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            let appearance = presenter.borrow().settings.appearance.clone();
            if shell.get_theme_kind() != theme_index(from_appearance(&appearance)) {
                apply_shell_appearance(&shell, &appearance);
            }
            let wanted = lang_from_settings(appearance.language);
            if lang.get() != wanted {
                set_shell_lang(&shell, &lang, wanted);
            }
        },
    );
    timer
}
