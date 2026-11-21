use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use madrigal_testkit as kit;
use madrigal_testkit::ElementHandle;
use madrigal_ui::{
    all_groups, group_index, group_key, group_title_key, settings_section, wire_shell,
    wire_shell_i18n, SettingsGroup, SettingsPresenter, GROUP_COUNT, MASKED_SECRET,
};
use slint::{ComponentHandle, Model as _, SharedString};

fn json_keys(group: SettingsGroup) -> Vec<String> {
    let settings = madrigal_settings::Settings::default();
    let doc = serde_json::to_value(&settings).unwrap();
    let section = doc.get(group_key(group)).unwrap();
    let mut keys: Vec<String> = section
        .as_object()
        .unwrap()
        .keys()
        .map(|key| key.to_owned())
        .collect();
    keys.sort();
    keys
}

fn row_snapshot(shell: &madrigal_ui::MainShell) -> Vec<(String, String)> {
    let rows = shell.get_settings_rows();
    (0..rows.row_count())
        .map(|index| {
            let row = rows.row_data(index).unwrap();
            (row.key.to_string(), row.kind.to_string())
        })
        .collect()
}

fn handles(shell: &madrigal_ui::MainShell, id: &str) -> Vec<ElementHandle> {
    ElementHandle::find_by_element_id(shell, id).collect()
}

fn scroll_down(shell: &madrigal_ui::MainShell, amount: f32) {
    let flicks = handles(shell, "SectionView::rows-flick");
    assert!(!flicks.is_empty(), "rows flickable missing");
    let steps = ((amount / 320.0).ceil() as i32).max(1);
    for _ in 0..steps {
        flicks[0].scroll(0.0, -320.0);
        kit::mock_advance(Duration::from_millis(50));
    }
}

fn scroll_until_found(shell: &madrigal_ui::MainShell, id: &str) -> Vec<ElementHandle> {
    for _ in 0..16 {
        let found = handles(shell, id);
        if !found.is_empty() {
            return found;
        }
        scroll_down(shell, 320.0);
    }
    panic!("{id} never scrolled into view");
}

fn adjacent_swap(before: &[String], after: &[String]) -> Option<usize> {
    if before.len() != after.len() {
        return None;
    }
    let diffs: Vec<usize> = before
        .iter()
        .zip(after.iter())
        .enumerate()
        .filter(|(_, (current, next))| current != next)
        .map(|(index, _)| index)
        .collect();
    if diffs.len() == 2
        && diffs[1] == diffs[0] + 1
        && before[diffs[0]] == after[diffs[1]]
        && before[diffs[1]] == after[diffs[0]]
    {
        Some(diffs[0])
    } else {
        None
    }
}

fn toggle_at(shell: &madrigal_ui::MainShell, rows: &[(String, String)], key: &str) {
    let switches: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, (_, kind))| kind == "switch")
        .map(|(position, _)| position)
        .collect();
    let nth = rows.iter().position(|(name, _)| name == key).unwrap();
    let order = switches.iter().position(|slot| *slot == nth).unwrap();
    let candidates = handles(shell, "SectionView::row-toggle");
    assert!(order < candidates.len(), "toggle not visible");
    kit::click(&candidates[order]);
}

fn choose_option(shell: &madrigal_ui::MainShell, rows: &[(String, String)], key: &str, value: &str) {
    let shell_rows = shell.get_settings_rows();
    let mut flat: Vec<(String, String)> = Vec::new();
    for (index, (name, kind)) in rows.iter().enumerate() {
        if kind != "choice" {
            continue;
        }
        let options = shell_rows.row_data(index).unwrap().options;
        for position in 0..options.row_count() {
            flat.push((name.clone(), options.row_data(position).unwrap().to_string()));
        }
    }
    let order = flat
        .iter()
        .position(|(name, option)| name == key && option == value)
        .unwrap();
    let candidates = handles(shell, "SectionView::row-option");
    assert!(order < candidates.len(), "option not visible");
    kit::click(&candidates[order]);
}

fn read_settings(path: &std::path::Path) -> madrigal_settings::Settings {
    madrigal_settings::Settings::load_from(path)
}

#[test]
fn ui_settings() {
    let tmp = tempfile::tempdir().unwrap();
    let settings_path = tmp.path().join("settings.json");
    madrigal_settings::Settings::default()
        .save_to(&settings_path)
        .unwrap();

    let profiles_dir = tmp.path().join("profiles");
    let store = madrigal_account::SessionStore::with_dir(profiles_dir);
    store.create("fixture").unwrap();

    let diag_dir = tmp.path().join("diag");
    let diag = madrigal_player::diag_log::DiagLog::open_in(
        &diag_dir,
        madrigal_player::diag_log::DiagLogConfig::default(),
    )
    .unwrap();
    diag.push(madrigal_player::diag_log::Level::Info, "test", "first-entry")
        .unwrap();
    diag.push(madrigal_player::diag_log::Level::Warn, "test", "second-entry")
        .unwrap();
    diag.push(madrigal_player::diag_log::Level::Error, "test", "third-entry")
        .unwrap();

    let _single = madrigal_player::single_instance::acquire_at(&tmp.path().join("m.sock")).unwrap();
    let primary = matches!(
        madrigal_player::single_instance::acquire_at(&tmp.path().join("other.sock")).unwrap(),
        madrigal_player::single_instance::InstanceRole::Primary(_)
    );
    assert!(primary);

    kit::init_headless();
    let shell = madrigal_ui::MainShell::new().unwrap();
    let lang = wire_shell_i18n(&shell, madrigal_i18n::Lang::Es);

    let state = Rc::new(RefCell::new(SettingsPresenter::load(
        &settings_path,
        store.ids(),
        &diag_dir,
    )));
    state.borrow_mut().po_detect = || true;
    state.borrow_mut().latency = Some(madrigal_ui::latencies_from_marks(200.0, 120.0));
    state.borrow_mut().single_primary = true;
    wire_shell(&shell, state.clone(), &lang);
    shell.set_page(5);
    shell.show().unwrap();
    kit::mock_advance(Duration::from_millis(100));

    assert_eq!(GROUP_COUNT, 11);
    assert_eq!(all_groups().len(), 11);
    assert!(state.borrow().needs_onboarding());
    assert!(shell.get_settings_show_onboarding());

    kit::click_by_id(&shell, "OnboardingOverlay::ob-lang-en").unwrap();
    assert_eq!(read_settings(&settings_path).appearance.language, madrigal_settings::Language::En);
    assert_eq!(
        shell
            .global::<madrigal_ui::I18n>()
            .get_lang()
            .as_str(),
        "en"
    );
    kit::click_by_id(&shell, "OnboardingOverlay::ob-next").unwrap();
    assert_eq!(shell.get_settings_ob_step(), 1);
    kit::click_by_id(&shell, "OnboardingOverlay::ob-po-check").unwrap();
    assert!(state.borrow().po_done);
    assert!(state.borrow().po_ok);
    assert_eq!(state.borrow().test_calls, vec!["po".to_owned()]);
    kit::click_by_id(&shell, "OnboardingOverlay::ob-next").unwrap();
    assert_eq!(shell.get_settings_ob_step(), 2);
    kit::click_by_id(&shell, "OnboardingOverlay::ob-login-google").unwrap();
    assert_eq!(state.borrow().login_calls, vec!["firefox".to_owned()]);
    kit::click_by_id(&shell, "OnboardingOverlay::ob-next").unwrap();
    assert_eq!(shell.get_settings_ob_step(), 3);
    shell.set_settings_ob_folder(SharedString::from(tmp.path().join("music").to_string_lossy().as_ref()));
    kit::click_by_id(&shell, "OnboardingOverlay::ob-folder-scan").unwrap();
    assert_eq!(state.borrow().scan_calls.len(), 1);
    kit::click_by_id(&shell, "OnboardingOverlay::ob-finish").unwrap();
    assert!(read_settings(&settings_path).updates.onboarding_done);
    assert!(!shell.get_settings_show_onboarding());
    assert!(!SettingsPresenter::load(&settings_path, store.ids(), &diag_dir).needs_onboarding());

    madrigal_ui::set_shell_lang(&shell, &lang, madrigal_i18n::Lang::Es);

    let groups = handles(&shell, "SettingsPage::group-touch");
    assert_eq!(groups.len(), GROUP_COUNT, "group list");
    assert_eq!(handles(&shell, "SettingsPage::diag-touch").len(), 1);

    for (index, group) in all_groups().iter().enumerate() {
        kit::click(&groups[index]);
        kit::mock_advance(Duration::from_millis(50));
        assert_eq!(shell.get_settings_group(), group_index(*group));
        assert!(!shell.get_settings_show_diag());

        let rows = row_snapshot(&shell);
        let mut expected = json_keys(*group);
        if *group == SettingsGroup::Account {
            let hidden = [
                "active_account_id",
                "account_name",
                "account_email",
                "account_channel_handle",
                "saved_accounts_json",
                "last_like_song_sync",
                "last_library_song_sync",
                "last_album_sync",
                "last_artist_sync",
                "last_playlist_sync",
            ];
            for key in hidden {
                assert!(expected.contains(&key.to_owned()), "settings keeps {key}");
            }
            expected.retain(|key| !hidden.contains(&key.as_str()));
        }
        let mut actual: Vec<String> = rows.iter().map(|(key, _)| key.clone()).collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected, "field parity for {group:?}");

        let section = settings_section(&read_settings(&settings_path), *group, &store.ids());
        assert_eq!(section.len(), rows.len());
        for (row, (key, kind)) in section.iter().zip(rows.iter()) {
            assert_eq!(row.key, key);
            assert_eq!(madrigal_ui::kind_tag(row.kind), kind);
        }

        let title: SharedString = shell
            .global::<madrigal_ui::I18n>()
            .invoke_trl("es".into(), group_title_key(*group).into());
        assert!(!title.as_str().is_empty());
        let english: SharedString = shell
            .global::<madrigal_ui::I18n>()
            .invoke_trl("en".into(), group_title_key(*group).into());
        assert!(!english.as_str().is_empty());
        if *group == SettingsGroup::Appearance {
            assert_ne!(title.as_str(), english.as_str());
        }

        match group {
            SettingsGroup::Appearance => {
                assert!(!handles(&shell, "SectionView::row-toggle").is_empty());
                assert!(!handles(&shell, "SectionView::row-choice-cycle").is_empty());
            }
            SettingsGroup::Content => {
                assert!(!handles(&shell, "SectionView::row-toggle").is_empty());
            }
            SettingsGroup::Player => {
                assert!(!handles(&shell, "SectionView::row-slider").is_empty());
            }
            SettingsGroup::Lyrics => {
                assert!(!scroll_until_found(&shell, "SectionView::prov-drag").is_empty());
            }
            SettingsGroup::Internet => {
                assert!(!handles(&shell, "SectionView::row-test").is_empty());
            }
            SettingsGroup::Storage => {
                assert!(!handles(&shell, "SectionView::row-field").is_empty());
            }
            SettingsGroup::Privacy => {
                assert!(!handles(&shell, "SectionView::row-toggle").is_empty());
            }
            SettingsGroup::Account => {
                assert_eq!(scroll_until_found(&shell, "SectionView::acct-connect").len(), 1);
                assert_eq!(handles(&shell, "SectionView::acct-status").len(), 1);
                assert!(handles(&shell, "SectionView::acct-help").len() == 1);
                assert!(!handles(&shell, "SectionView::row-toggle").is_empty());
                state.borrow_mut().begin_account_work();
                madrigal_ui::refresh_settings(&shell, &state.borrow());
                assert!(shell.get_settings_account_busy());
                state.borrow_mut().finish_account_work();
                state.borrow_mut().set_account_error("settings.errNoFirefox");
                madrigal_ui::refresh_settings(&shell, &state.borrow());
                assert_eq!(
                    shell.get_settings_account_error().as_str(),
                    "settings.errNoFirefox"
                );
                state.borrow_mut().set_account_connected("Nombre", "nombre@example.com");
                madrigal_ui::refresh_settings(&shell, &state.borrow());
                assert!(shell.get_settings_account_connected());
                assert_eq!(shell.get_settings_account_name().as_str(), "Nombre");
                assert_eq!(shell.get_settings_account_email().as_str(), "nombre@example.com");
                assert!(shell.get_settings_account_error().is_empty());
                state.borrow_mut().clear_account();
                madrigal_ui::refresh_settings(&shell, &state.borrow());
                assert!(!shell.get_settings_account_connected());
            }
            SettingsGroup::Integrations => {
                assert!(!scroll_until_found(&shell, "SectionView::row-test").is_empty());
            }
            SettingsGroup::Ai => {
                assert!(!handles(&shell, "SectionView::row-test").is_empty());
            }
            SettingsGroup::Updates => {
                assert!(!handles(&shell, "SectionView::row-toggle").is_empty());
            }
        }
    }

    kit::click(&groups[group_index(SettingsGroup::Appearance) as usize]);
    let before = read_settings(&settings_path).appearance.dynamic_theme;
    let rows = row_snapshot(&shell);
    toggle_at(&shell, &rows, "dynamic_theme");
    assert_eq!(read_settings(&settings_path).appearance.dynamic_theme, !before);

    let rows = row_snapshot(&shell);
    choose_option(&shell, &rows, "theme", "dark");
    assert_eq!(read_settings(&settings_path).appearance.theme, madrigal_settings::Theme::Dark);
    let cycles = handles(&shell, "SectionView::row-choice-cycle");
    assert!(!cycles.is_empty());
    kit::click(&cycles[0]);
    assert_eq!(read_settings(&settings_path).appearance.theme, madrigal_settings::Theme::Matugen);

    shell
        .invoke_settings_edited("custom_theme_color".into(), "#ff0000".into());
    assert_eq!(
        read_settings(&settings_path).appearance.custom_theme_color,
        "#ff0000"
    );
    kit::click(&groups[group_index(SettingsGroup::Player) as usize]);
    shell.invoke_settings_stepped("volume".into(), 104.0);
    assert_eq!(read_settings(&settings_path).player.volume, 104);

    kit::click(&groups[group_index(SettingsGroup::Lyrics) as usize]);
    let order_before: Vec<String> =
        provider_names(&read_settings(&settings_path).lyrics.provider_order);
    let drags = scroll_until_found(&shell, "SectionView::prov-drag");
    let drag = &drags[drags.len() / 2];
    let start = drag.absolute_position();
    let size = drag.size();
    drag.mock_drag(
        slint::LogicalPosition::new(start.x, start.y + size.height),
        slint::platform::PointerEventButton::Left,
    );
    kit::mock_advance(Duration::from_millis(100));
    let order_after: Vec<String> =
        provider_names(&read_settings(&settings_path).lyrics.provider_order);
    let moved = adjacent_swap(&order_before, &order_after).expect("drag swaps one adjacent pair");
    shell.invoke_settings_provider_moved("provider_order".into(), (moved + 1) as i32, -52.0);
    let restored: Vec<String> =
        provider_names(&read_settings(&settings_path).lyrics.provider_order);
    assert_eq!(restored, order_before, "drag reorder roundtrip");

    let ups = scroll_until_found(&shell, "SectionView::prov-up");
    kit::click(&ups[0]);
    let mut order_after: Vec<String> =
        provider_names(&read_settings(&settings_path).lyrics.provider_order);
    for _ in 0..5 {
        if adjacent_swap(&order_before, &order_after).is_some() {
            break;
        }
        scroll_down(&shell, 200.0);
        let ups = scroll_until_found(&shell, "SectionView::prov-up");
        kit::click(&ups[0]);
        order_after = provider_names(&read_settings(&settings_path).lyrics.provider_order);
    }
    let moved = adjacent_swap(&order_before, &order_after).expect("button swaps one adjacent pair");
    shell.invoke_settings_provider_moved("provider_order".into(), moved as i32, 52.0);
    let restored: Vec<String> =
        provider_names(&read_settings(&settings_path).lyrics.provider_order);
    assert_eq!(restored, order_before, "button reorder roundtrip");

    let toggles = scroll_until_found(&shell, "SectionView::prov-toggle");
    kit::click(&toggles[0]);
    let disabled: Vec<String> =
        provider_names(&read_settings(&settings_path).lyrics.providers_disabled);
    assert_eq!(disabled.len(), 1, "one provider disabled");
    shell.invoke_settings_provider_toggled(
        "provider_order".into(),
        disabled[0].clone().into(),
        true,
    );
    assert!(
        read_settings(&settings_path).lyrics.providers_disabled.is_empty(),
        "provider enabled"
    );

    kit::click(&groups[group_index(SettingsGroup::Integrations) as usize]);
    shell.invoke_settings_edited("maloja_api_key".into(), "k3".into());
    assert_eq!(read_settings(&settings_path).integrations.maloja_api_key, "k3");

    kit::click(&groups[group_index(SettingsGroup::Ai) as usize]);
    shell.invoke_settings_edited("api_key".into(), "k3".into());
    assert_eq!(read_settings(&settings_path).ai.api_key, "k3");
    let table = shell.get_settings_rows();
    let rows = row_snapshot(&shell);
    let secret = rows.iter().position(|(key, _)| key == "api_key").unwrap();
    assert_eq!(table.row_data(secret).unwrap().display.as_str(), MASKED_SECRET);
    let reveals = scroll_until_found(&shell, "SectionView::row-reveal");
    assert_eq!(reveals.len(), 1);
    kit::click(&reveals[0]);
    let table = shell.get_settings_rows();
    assert_eq!(table.row_data(secret).unwrap().display.as_str(), "k3");

    kit::click(&groups[group_index(SettingsGroup::Internet) as usize]);
    let tests = handles(&shell, "SectionView::row-test");
    assert!(!tests.is_empty());
    kit::click(&tests[0]);
    assert_eq!(state.borrow().test_calls.last().unwrap(), "po");
    assert!(!shell.get_settings_test_result().as_str().is_empty());
    assert!(shell.get_settings_po_done());

    kit::click_by_id(&shell, "SettingsPage::diag-touch").unwrap();
    assert!(shell.get_settings_show_diag());
    assert_eq!(shell.get_settings_diag_entry_count(), 3);
    kit::click_by_id(&shell, "DiagnosticsPanel::diag-level-warn").unwrap();
    assert_eq!(shell.get_settings_diag_entry_count(), 2);
    kit::click_by_id(&shell, "DiagnosticsPanel::diag-level-error").unwrap();
    assert_eq!(shell.get_settings_diag_entry_count(), 1);
    kit::click_by_id(&shell, "DiagnosticsPanel::diag-level-all").unwrap();
    assert_eq!(shell.get_settings_diag_entry_count(), 3);
    kit::click_by_id(&shell, "DiagnosticsPanel::diag-export-btn").unwrap();
    assert!(shell.get_settings_diag_exported());
    kit::snapshot_to(shell.window(), &kit::snapshot_dir(tmp.path()).join("settings-diagnostics.png"))
        .unwrap();
    let exported = std::fs::read_to_string(state.borrow().diag_export_path.clone()).unwrap();
    assert!(exported.contains("first-entry"));
    assert!(exported.contains("third-entry"));
    assert!(shell.get_settings_diag_latency().as_str().contains("200"));
    let single_key = shell.get_settings_diag_single();
    let single: SharedString = shell
        .global::<madrigal_ui::I18n>()
        .invoke_trl("en".into(), single_key);
    assert!(single.as_str().contains("Primary"), "single status: {single}");

    shell.set_settings_show_diag(false);
    let snapshot_dir = kit::snapshot_dir(tmp.path());
    for group in all_groups() {
        let tabs = handles(&shell, "SettingsPage::group-touch");
        kit::click(&tabs[group_index(group) as usize]);
        kit::mock_advance(Duration::from_millis(50));

        let page = kit::find_by_id(&shell, "PageHost::settings-page").unwrap();
        let title = kit::find_by_id(&shell, "SettingsPage::section-title").unwrap();
        let page_pos = page.absolute_position();
        let page_size = page.size();
        let frames = handles(&shell, "SectionView::row-frame");
        assert!(!frames.is_empty(), "rows missing for {group:?}");
        for frame in &frames {
            let pos = frame.absolute_position();
            let size = frame.size();
            assert_eq!(size.height, 54.0, "row height for {group:?}");
            assert!((pos.x - title.absolute_position().x).abs() < 1.0, "row left inset for {group:?}");
            assert!(
                (pos.x + size.width - (page_pos.x + page_size.width - 16.0)).abs() < 1.0,
                "row right inset for {group:?}"
            );
        }
        for pair in frames.windows(2) {
            let first = pair[0].absolute_position();
            let second = pair[1].absolute_position();
            assert!(
                second.y + 1.0 >= first.y + pair[0].size().height,
                "row overlap for {group:?}"
            );
        }

        let label = format!("settings {group:?}");
        kit::lint_layout_at_sizes(&shell, &label);
        let name = format!("settings-{:?}.png", group).to_lowercase();
        kit::snapshot_to(shell.window(), &snapshot_dir.join(name)).unwrap();
    }

    kit::mock_advance(Duration::from_millis(600));
    let shot = tmp.path().join("settings.png");
    kit::snapshot_to(shell.window(), &shot).unwrap();
    let bytes = std::fs::read(&shot).unwrap();
    assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    assert!(bytes.len() > 4096, "snapshot too small: {}", bytes.len());
}

fn provider_names(order: &[madrigal_settings::LyricsProvider]) -> Vec<String> {
    order
        .iter()
        .map(provider_json)
        .collect()
}

fn provider_json(provider: &madrigal_settings::LyricsProvider) -> String {
    serde_json::to_value(provider)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}
