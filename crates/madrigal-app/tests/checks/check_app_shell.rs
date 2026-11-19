use std::time::Duration;

use madrigal_testkit as kit;
use madrigal_ui::{from_appearance, theme_index, I18n};
use slint::ComponentHandle;

fn rgb(color: slint::Color) -> (u8, u8, u8) {
    (color.red(), color.green(), color.blue())
}

#[test]
fn app_shell() {
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, root.path());
    std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", root.path().join("cache"));

    kit::init_headless();
    let ui = madrigal_app::ui::boot_ui().expect("boot_ui");
    let path = madrigal_paths::settings_path();
    let settings = madrigal_settings::Settings::load_from(&path);

    assert_eq!(
        ui.shell.get_theme_kind(),
        theme_index(from_appearance(&settings.appearance)),
        "tema inicial"
    );
    let es_home: slint::SharedString = ui
        .shell
        .global::<I18n>()
        .invoke_trl("es".into(), "nav.home".into());
    assert!(!es_home.as_str().is_empty(), "trl es vacío");
    assert_eq!(ui.shell.global::<I18n>().get_lang().as_str(), "es");

    let before_theme = ui.shell.get_theme_kind();
    let before_surface = rgb(ui.shell.get_palette().surface);
    ui.shell
        .invoke_settings_chosen("theme".into(), "light".into());
    kit::mock_advance(Duration::from_millis(400));
    let disk = madrigal_settings::Settings::load_from(&path);
    assert_eq!(disk.appearance.theme, madrigal_settings::Theme::Light);
    assert_ne!(
        ui.shell.get_theme_kind(),
        before_theme,
        "theme_kind sin cambio"
    );
    assert_eq!(ui.shell.get_theme_kind(), 0);
    assert_ne!(
        rgb(ui.shell.get_palette().surface),
        before_surface,
        "palette sin cambio"
    );
    assert_eq!(rgb(ui.shell.get_palette().surface), (0xff, 0xff, 0xff));

    ui.shell.invoke_settings_lang_picked("en".into());
    assert_eq!(
        madrigal_settings::Settings::load_from(&path)
            .appearance
            .language,
        madrigal_settings::Language::En
    );
    assert_eq!(ui.shell.global::<I18n>().get_lang().as_str(), "en");
    let en_home: slint::SharedString = ui
        .shell
        .global::<I18n>()
        .invoke_trl("en".into(), "nav.home".into());
    assert!(!en_home.as_str().is_empty(), "trl en vacío");
    assert_ne!(en_home.as_str(), es_home.as_str());

    ui.shell.invoke_settings_lang_picked("es".into());
    assert_eq!(ui.shell.global::<I18n>().get_lang().as_str(), "es");
    assert_eq!(
        madrigal_settings::Settings::load_from(&path)
            .appearance
            .language,
        madrigal_settings::Language::Es
    );

    println!("OK app_shell");
}
