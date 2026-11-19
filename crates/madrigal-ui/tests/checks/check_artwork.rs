use image::{Rgba, RgbaImage};
use madrigal_artwork::{ArtworkCache, Source};
use madrigal_testkit as kit;
use madrigal_ui::{apply_cover, to_slint_image, CoverTestWindow};
use slint::ComponentHandle;
use std::collections::HashSet;
use std::io;
use std::time::Duration;

fn checkerboard(a: Rgba<u8>, b: Rgba<u8>) -> RgbaImage {
    let mut image = RgbaImage::new(48, 48);
    for y in 0..48 {
        for x in 0..48 {
            image.put_pixel(x, y, if (x / 8 + y / 8) % 2 == 0 { a } else { b });
        }
    }
    image
}

fn png(image: &RgbaImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    bytes
}

fn pump_events() {
    slint::invoke_from_event_loop(|| slint::quit_event_loop().unwrap()).unwrap();
    slint::run_event_loop().unwrap();
}

#[test]
fn artwork_ui_check() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", tmp.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", tmp.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", tmp.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", tmp.path().join("state"));

    kit::init_headless();
    let window = CoverTestWindow::new().unwrap();

    let art = checkerboard(Rgba([230, 20, 30, 255]), Rgba([20, 30, 220, 255]));
    let cache = ArtworkCache::with_memory_budget(tmp.path().into(), 1 << 20, 1 << 20);
    let decoded = cache.get("ui-cover", Source::Bytes(png(&art)), 96).unwrap();
    let slint_image = to_slint_image(&decoded);
    assert_eq!((slint_image.size().width, slint_image.size().height), (96, 96));

    window.set_source(slint_image);
    window.show().unwrap();
    let shot = kit::snapshot_dir(tmp.path()).join("cover.png");
    kit::snapshot_to(window.window(), &shot).unwrap();
    let pixels = image::open(&shot).unwrap().to_rgba8();
    let colors: HashSet<_> = pixels.pixels().map(|pixel| pixel.0).collect();
    assert!(colors.len() > 1, "cover rendered as a single color: {colors:?}");

    window.set_source(slint::Image::default());
    kit::mock_advance(Duration::from_millis(50));
    let empty = kit::snapshot_dir(tmp.path()).join("cover-placeholder.png");
    kit::snapshot_to(window.window(), &empty).unwrap();
    assert_ne!(std::fs::read(&shot).unwrap(), std::fs::read(&empty).unwrap());

    let red = RgbaImage::from_pixel(96, 96, Rgba([255, 0, 0, 255]));
    let blue = RgbaImage::from_pixel(96, 96, Rgba([0, 0, 255, 255]));
    apply_cover(&window, "hero", red, |w, img| w.set_source(img));
    apply_cover(&window, "hero", blue, |w, img| w.set_source(img));
    pump_events();
    let guard = kit::snapshot_dir(tmp.path()).join("cover-generation.png");
    kit::snapshot_to(window.window(), &guard).unwrap();
    let center = image::open(&guard).unwrap().to_rgba8().get_pixel(48, 48).0;
    assert_eq!(center, [0, 0, 255, 255], "stale cover overwrote newer one");
}
