use madrigal_testkit as kit;
use madrigal_ui::{
    apply_shell_appearance, apply_shell_theme, dispatch_key, overlay_from_index, overlay_index,
    page_from_index, page_index, set_shell_lang, sync_queue_seam, wire_shell_i18n, MainShell,
    ShellOverlay, ShellPage, ThemeKind,
};
use slint::ComponentHandle;
use std::time::Duration;

fn expect_rgb(color: slint::Color, r: u8, g: u8, b: u8) {
    assert_eq!(
        (color.red(), color.green(), color.blue()),
        (r, g, b),
        "unexpected color {color:?}"
    );
}

#[test]
fn ui_shell() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", tmp.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", tmp.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", tmp.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", tmp.path().join("state"));

    assert_eq!(page_index(ShellPage::Home), 0);
    assert_eq!(page_index(ShellPage::Settings), 5);
    assert_eq!(page_from_index(2), ShellPage::Explore);
    assert_eq!(page_from_index(99), ShellPage::Home);
    assert_eq!(overlay_index(ShellOverlay::None), 0);
    assert_eq!(overlay_index(ShellOverlay::Prompt), 2);
    assert_eq!(overlay_from_index(1), ShellOverlay::Confirm);
    assert_eq!(overlay_from_index(7), ShellOverlay::None);

    kit::init_headless();
    let shell = MainShell::new().unwrap();
    let lang = wire_shell_i18n(&shell, madrigal_i18n::Lang::Es);

    let es_home: slint::SharedString = shell
        .global::<madrigal_ui::I18n>()
        .invoke_trl("es".into(), "nav.home".into());
    assert_eq!(es_home.as_str(), "Inicio");
    set_shell_lang(&shell, &lang, madrigal_i18n::Lang::En);
    let en_home: slint::SharedString = shell
        .global::<madrigal_ui::I18n>()
        .invoke_trl("en".into(), "nav.home".into());
    assert_eq!(en_home.as_str(), "Home");
    set_shell_lang(&shell, &lang, madrigal_i18n::Lang::Es);

    apply_shell_theme(&shell, ThemeKind::Light);
    assert_eq!(shell.get_theme_kind(), 0);
    expect_rgb(shell.get_palette().surface, 0xff, 0xf8, 0xf7);
    apply_shell_theme(&shell, ThemeKind::Dark);
    assert_eq!(shell.get_theme_kind(), 1);
    expect_rgb(shell.get_palette().surface, 0x1a, 0x11, 0x11);
    apply_shell_theme(&shell, ThemeKind::PureBlack);
    assert_eq!(shell.get_theme_kind(), 2);
    expect_rgb(shell.get_palette().surface, 0x00, 0x00, 0x00);
    apply_shell_theme(&shell, ThemeKind::Light);

    let light_appearance = madrigal_settings::Appearance {
        theme: madrigal_settings::Theme::Light,
        ..Default::default()
    };
    apply_shell_appearance(&shell, &light_appearance);
    assert_eq!(shell.get_theme_kind(), 0);
    let black_appearance = madrigal_settings::Appearance {
        theme: madrigal_settings::Theme::Dark,
        pure_black: true,
        ..Default::default()
    };
    apply_shell_appearance(&shell, &black_appearance);
    assert_eq!(shell.get_theme_kind(), 2);
    apply_shell_theme(&shell, ThemeKind::Light);

    let queue = madrigal_player::queue::Queue::new();
    sync_queue_seam(&shell, &queue);
    assert_eq!(shell.get_queue_count(), 0);

    assert_eq!(shell.get_page(), 0);
    for (id, page) in [
        ("SideNav::nav-home", 0),
        ("SideNav::nav-search", 1),
        ("SideNav::nav-explore", 2),
        ("SideNav::nav-library", 3),
        ("SideNav::nav-downloads", 4),
        ("SideNav::nav-settings", 5),
    ] {
        kit::click_by_id(&shell, id).unwrap();
        assert_eq!(shell.get_page(), page);
    }

    shell.set_page(0);
    for expected in [1, 2, 3, 4, 5, 0] {
        assert!(dispatch_key(&shell, "]"));
        assert_eq!(shell.get_page(), expected);
    }
    assert!(dispatch_key(&shell, "["));
    assert_eq!(shell.get_page(), 5);
    for (key, expected) in [("1", 0), ("2", 1), ("3", 2), ("4", 3), ("5", 4), ("6", 5)] {
        assert!(dispatch_key(&shell, key));
        assert_eq!(shell.get_page(), expected);
    }
    shell.set_page(0);

    assert!(dispatch_key(&shell, " "));
    assert_eq!(shell.get_play_toggle_count(), 1);
    assert!(dispatch_key(&shell, "Left"));
    assert_eq!(shell.get_seek_back_count(), 1);
    assert_eq!(shell.get_seek_delta(), -10);
    assert!(dispatch_key(&shell, "Right"));
    assert_eq!(shell.get_seek_fwd_count(), 1);
    assert_eq!(shell.get_seek_delta(), 10);
    assert!(dispatch_key(&shell, "n"));
    assert!(dispatch_key(&shell, "N"));
    assert_eq!(shell.get_next_count(), 2);
    assert!(dispatch_key(&shell, "p"));
    assert!(dispatch_key(&shell, "P"));
    assert_eq!(shell.get_prev_count(), 2);
    assert!(dispatch_key(&shell, "f"));
    assert!(dispatch_key(&shell, "F"));
    assert_eq!(shell.get_full_player_count(), 2);
    assert!(!shell.get_search_focused());
    assert!(dispatch_key(&shell, "/"));
    assert!(shell.get_search_focused());
    assert_eq!(shell.get_search_focus_count(), 1);
    assert!(!dispatch_key(&shell, "x"));
    assert_eq!(shell.get_play_toggle_count(), 1);

    shell.invoke_open_confirm("Borrar".into(), "Seguro".into());
    assert_eq!(shell.get_overlay(), 1);
    assert!(!dispatch_key(&shell, " "));
    assert_eq!(shell.get_play_toggle_count(), 1);
    assert!(dispatch_key(&shell, "Escape"));
    assert_eq!(shell.get_overlay(), 0);
    assert_eq!(shell.get_confirm_result(), 0);
    assert!(!shell.get_sidebar_focused());
    assert!(dispatch_key(&shell, "Escape"));
    assert!(shell.get_sidebar_focused());

    shell.invoke_open_confirm("Borrar".into(), "Seguro".into());
    assert_eq!(shell.get_confirm_result(), -1);
    kit::click_by_id(&shell, "ConfirmOverlay::accept-touch").unwrap();
    assert_eq!(shell.get_confirm_result(), 1);
    assert_eq!(shell.get_overlay(), 0);

    shell.invoke_open_confirm("Borrar".into(), "Seguro".into());
    kit::click_by_id(&shell, "ConfirmOverlay::cancel-touch").unwrap();
    assert_eq!(shell.get_confirm_result(), 0);
    assert_eq!(shell.get_overlay(), 0);

    shell.invoke_open_prompt("Renombrar".into());
    assert_eq!(shell.get_overlay(), 2);
    assert_eq!(shell.get_prompt_input().as_str(), "");
    shell.set_prompt_input("Nueva lista".into());
    kit::click_by_id(&shell, "PromptOverlay::prompt-ok-touch").unwrap();
    assert_eq!(shell.get_prompt_result().as_str(), "Nueva lista");
    assert_eq!(shell.get_overlay(), 0);

    shell.invoke_open_prompt("Renombrar".into());
    assert!(dispatch_key(&shell, "Escape"));
    assert_eq!(shell.get_overlay(), 0);
    assert_eq!(shell.get_prompt_result().as_str(), "");

    let missing = kit::click_by_id(&shell, "SideNav::no-existe");
    assert!(matches!(missing, Err(kit::Error::ElementNotFound(_))));

    kit::mock_advance(Duration::from_millis(600));

    shell.show().unwrap();
    let shot = tmp.path().join("shell.png");
    kit::snapshot_to(shell.window(), &shot).unwrap();
    let bytes = std::fs::read(&shot).unwrap();
    assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    assert!(bytes.len() > 4096, "snapshot too small: {}", bytes.len());
}
