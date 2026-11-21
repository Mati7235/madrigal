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
    expect_rgb(shell.get_palette().surface, 0xff, 0xff, 0xff);
    apply_shell_theme(&shell, ThemeKind::Dark);
    assert_eq!(shell.get_theme_kind(), 1);
    expect_rgb(shell.get_palette().surface, 0x0b, 0x08, 0x10);
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

    shell.set_page(0);
    shell.set_search_query("beat".into());
    kit::click_by_id(&shell, "MainShell::top-bar").unwrap();
    shell.window().dispatch_event(slint::platform::WindowEvent::KeyPressed {
        text: slint::platform::Key::Return.into(),
    });
    assert_eq!(shell.get_page(), 1);
    assert_eq!(shell.get_search_query().as_str(), "beat");

    kit::mock_advance(Duration::from_millis(20));
    kit::click_by_id(&shell, "MainShell::top-bar").unwrap();
    kit::mock_advance(Duration::from_millis(20));
    assert!(shell.get_search_input_focused());
    kit::click_by_id(&shell, "M3SearchBar::search-clear").unwrap();
    kit::mock_advance(Duration::from_millis(20));
    assert_eq!(shell.get_search_query().as_str(), "");
    assert!(shell.get_search_input_focused());
    shell.set_search_query("beat".into());

    shell.set_page(3);
    shell.set_local_query("abbey".into());
    kit::click_by_id(&shell, "MainShell::local-bar").unwrap();
    shell.window().dispatch_event(slint::platform::WindowEvent::KeyPressed {
        text: slint::platform::Key::Return.into(),
    });
    assert_eq!(shell.get_page(), 3);
    assert_eq!(shell.get_local_query().as_str(), "abbey");
    assert_eq!(shell.get_search_query().as_str(), "beat");
    shell.set_page(0);

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

    shell.invoke_lib_sort_opened();
    assert!(shell.get_lib_sort_open());
    assert!(dispatch_key(&shell, "Escape"));
    assert!(!shell.get_lib_sort_open());
    shell.invoke_lib_tags_opened();
    assert!(shell.get_lib_tags_open());
    assert!(dispatch_key(&shell, "Escape"));
    assert!(!shell.get_lib_tags_open());

    let missing = kit::click_by_id(&shell, "SideNav::no-existe");
    assert!(matches!(missing, Err(kit::Error::ElementNotFound(_))));

    kit::mock_advance(Duration::from_millis(600));

    shell.show().unwrap();
    for (width, height) in kit::WINDOW_SIZES {
        kit::resize(&shell, width, height);
        assert_eq!(shell.get_nowbar_width(), width);
        assert_eq!(shell.get_nowbar_bottom(), height);
        let violations = kit::lint_layout(&shell, (width, height));
        assert!(
            violations.is_empty(),
            "layout violations at {width}x{height}: {violations:#?}"
        );
    }
    kit::resize(&shell, 1920.0, 1080.0);
    let shot = kit::snapshot_dir(tmp.path()).join("shell.png");
    kit::snapshot_to(shell.window(), &shot).unwrap();
    let bytes = std::fs::read(&shot).unwrap();
    assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    assert!(bytes.len() > 4096, "snapshot too small: {}", bytes.len());

    kit::resize(&shell, 683.0, 768.0);
    assert!(dispatch_key(&shell, "/"));
    assert!(dispatch_key(&shell, "Escape"));
    let home_label = kit::find_by_id(&shell, "SideNav::home-focus").unwrap();
    assert_eq!(home_label.accessible_label().unwrap().as_str(), "Inicio");
    assert!(
        kit::find_by_id(&shell, "SideNav::home-focus-ring").is_ok(),
        "focused nav item has no ring"
    );
    let tip = kit::find_by_id(&shell, "MainShell::nav-tip-label").unwrap();
    assert_eq!(tip.accessible_label().unwrap().as_str(), "Inicio");
    assert!(kit::find_by_id(&shell, "MainShell::nav-tip").is_ok());

    shell
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: "\t".into() });
    assert!(kit::find_by_id(&shell, "SideNav::home-focus-ring").is_err());
    assert!(kit::find_by_id(&shell, "SideNav::search-focus-ring").is_ok());
    let tip = kit::find_by_id(&shell, "MainShell::nav-tip-label").unwrap();
    assert_eq!(tip.accessible_label().unwrap().as_str(), "Buscar");
    shell.window().dispatch_event(slint::platform::WindowEvent::KeyPressed {
        text: slint::platform::Key::Return.into(),
    });
    assert_eq!(shell.get_page(), 1);

    let home_touch = kit::find_by_id(&shell, "SideNav::nav-home").unwrap();
    let home_pos = home_touch.absolute_position();
    let home_size = home_touch.size();
    shell
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(
                home_pos.x + home_size.width / 2.0,
                home_pos.y + home_size.height / 2.0,
            ),
        });
    kit::mock_advance(Duration::from_millis(50));
    let tip = kit::find_by_id(&shell, "MainShell::nav-tip-label").unwrap();
    assert_eq!(tip.accessible_label().unwrap().as_str(), "Inicio");
    shell
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(2.0, 2.0),
        });

    shell.set_search_query("".into());
    let mut chip_focused = false;
    for _ in 0..20 {
        shell
            .window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: "\t".into() });
        if kit::ElementHandle::find_by_element_id(&shell, "M3Chip::focus-ring").count() == 1 {
            chip_focused = true;
            break;
        }
    }
    assert!(chip_focused, "chip focus ring after Tab");
    shell.window().dispatch_event(slint::platform::WindowEvent::KeyPressed {
        text: slint::platform::Key::Return.into(),
    });
    assert_eq!(shell.get_search_source(), 0, "Return activates the focused chip");

    shell.set_page(0);
    kit::resize(&shell, 1920.0, 1080.0);
    shell.set_full_player_open(true);
    kit::mock_advance(Duration::from_millis(50));
    let bar_track = kit::find_by_id(&shell, "NowPlayingBar::bar-vol-track").unwrap();
    let bar_fill = kit::find_by_id(&shell, "NowPlayingBar::bar-vol-fill").unwrap();
    let full_track = kit::find_by_id(&shell, "PlayerConsole::vol-track").unwrap();
    let full_fill = kit::find_by_id(&shell, "PlayerConsole::vol-fill").unwrap();
    let bar_touch = kit::find_by_id(&shell, "NowPlayingBar::bar-vol-touch").unwrap();
    let start = bar_touch.absolute_position();
    let size = bar_touch.size();
    bar_touch.mock_drag(
        slint::LogicalPosition::new(start.x + size.width * 0.3, start.y + size.height / 2.0),
        slint::platform::PointerEventButton::Left,
    );
    let bar_volume = shell.get_volume();
    assert!((bar_volume - 0.7).abs() > 0.05, "bar volume did not move");
    assert!((bar_fill.size().width / bar_track.size().width - bar_volume).abs() < 0.02);
    assert!((full_fill.size().width / full_track.size().width - bar_volume).abs() < 0.02);

    let full_touch = kit::find_by_id(&shell, "PlayerConsole::vol-touch").unwrap();
    let start = full_touch.absolute_position();
    let size = full_touch.size();
    full_touch.mock_drag(
        slint::LogicalPosition::new(
            start.x + 16.0 + (size.width - 32.0) * 0.2,
            start.y + size.height / 2.0,
        ),
        slint::platform::PointerEventButton::Left,
    );
    let full_volume = shell.get_volume();
    assert!((full_volume - 0.2).abs() < 0.05, "full player volume did not move");
    assert!((full_fill.size().width / full_track.size().width - full_volume).abs() < 0.02);
    assert!((bar_fill.size().width / bar_track.size().width - full_volume).abs() < 0.02);
}
