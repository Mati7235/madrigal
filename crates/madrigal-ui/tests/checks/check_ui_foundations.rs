use madrigal_testkit as kit;
use madrigal_ui::{DemoGallery, ThemeKind, apply_theme, set_lang, wire_i18n};
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
fn ui_foundations() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", tmp.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", tmp.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", tmp.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", tmp.path().join("state"));

    kit::init_headless();
    let gallery = DemoGallery::new().unwrap();
    let lang = wire_i18n(&gallery, madrigal_i18n::Lang::Es);

    let es_home: slint::SharedString = gallery.global::<madrigal_ui::I18n>().invoke_trl(
        "es".into(),
        "nav.home".into(),
    );
    assert_eq!(es_home.as_str(), "Inicio");
    set_lang(&gallery, &lang, madrigal_i18n::Lang::En);
    let en_home: slint::SharedString = gallery.global::<madrigal_ui::I18n>().invoke_trl(
        "en".into(),
        "nav.home".into(),
    );
    assert_eq!(en_home.as_str(), "Home");
    set_lang(&gallery, &lang, madrigal_i18n::Lang::Es);

    apply_theme(&gallery, ThemeKind::Light);
    assert_eq!(gallery.get_theme_kind(), 0);
    let light = gallery.get_palette();
    expect_rgb(light.surface, 0xff, 0xff, 0xff);
    expect_rgb(light.primary, 0x0b, 0x6b, 0xcb);
    expect_rgb(light.on_primary, 0xff, 0xff, 0xff);
    expect_rgb(light.primary_container, 0xd6, 0xe7, 0xff);
    expect_rgb(light.surface_container_highest, 0xd8, 0xe3, 0xf2);
    expect_rgb(light.error, 0xba, 0x1a, 0x1a);
    expect_rgb(light.outline_variant, 0xc4, 0xc6, 0xd0);

    apply_theme(&gallery, ThemeKind::Dark);
    assert_eq!(gallery.get_theme_kind(), 1);
    let dark = gallery.get_palette();
    expect_rgb(dark.surface, 0x0b, 0x08, 0x10);
    expect_rgb(dark.primary, 0xcf, 0xbc, 0xff);
    expect_rgb(dark.on_primary, 0x38, 0x1e, 0x72);
    assert_ne!(
        (dark.surface.red(), dark.surface.green(), dark.surface.blue()),
        (light.surface.red(), light.surface.green(), light.surface.blue())
    );

    apply_theme(&gallery, ThemeKind::PureBlack);
    assert_eq!(gallery.get_theme_kind(), 2);
    let black = gallery.get_palette();
    expect_rgb(black.surface, 0x00, 0x00, 0x00);
    expect_rgb(black.background, 0x00, 0x00, 0x00);
    expect_rgb(black.primary, 0xcf, 0xbc, 0xff);

    apply_theme(&gallery, ThemeKind::Light);
    assert_eq!(gallery.get_demo_clicks(), 0);
    kit::click_by_id(&gallery, "DemoGallery::filled-btn").unwrap();
    assert_eq!(gallery.get_demo_clicks(), 1);
    kit::click_by_id(&gallery, "DemoGallery::icon-filled").unwrap();
    assert_eq!(gallery.get_demo_clicks(), 2);

    assert!(!gallery.get_chip_on());
    kit::click_by_id(&gallery, "DemoGallery::chip-songs").unwrap();
    assert!(gallery.get_chip_on());

    assert_eq!(gallery.get_card_clicks(), 0);
    kit::click_by_id(&gallery, "DemoGallery::card-demo").unwrap();
    assert_eq!(gallery.get_card_clicks(), 1);

    let missing = kit::click_by_id(&gallery, "DemoGallery::no-existe");
    assert!(matches!(missing, Err(kit::Error::ElementNotFound(_))));

    kit::mock_advance(Duration::from_millis(600));

    gallery.show().unwrap();
    let shot = kit::snapshot_dir(tmp.path()).join("gallery.png");
    kit::snapshot_to(gallery.window(), &shot).unwrap();
    let bytes = std::fs::read(&shot).unwrap();
    assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    assert!(bytes.len() > 4096, "snapshot too small: {}", bytes.len());
}
