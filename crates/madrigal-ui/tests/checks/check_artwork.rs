use image::{Rgba, RgbaImage};
use madrigal_artwork::{bucket_for, sized_url, ArtworkCache, Source};
use madrigal_testkit as kit;
use madrigal_ui::{
    apply_cover, init_artwork, resolve_detail, resolve_search, set_now_playing_cover,
    to_slint_image, CatalogItem, CoverTestWindow, DetailKind, MainShell,
};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::collections::HashSet;
use std::io;
use std::time::{Duration, Instant};

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

    let shell = MainShell::new().unwrap();
    let bucket = bucket_for(36.0, shell.window().scale_factor());
    let url = sized_url(
        "https://yt3.googleusercontent.com/catalog-art=w120-h120-l90-rj",
        bucket,
    );
    let art = checkerboard(Rgba([10, 200, 40, 255]), Rgba([240, 200, 10, 255]));
    cache.get(&url, Source::Bytes(png(&art)), bucket).unwrap();
    init_artwork(tmp.path().to_path_buf(), 1 << 20);
    let item = CatalogItem {
        kind: DetailKind::Album,
        id: "art-1".into(),
        title: "Art".into(),
        subtitle: String::new().into(),
        duration: String::new().into(),
        playable: true,
        artist: String::new().into(),
        album: String::new().into(),
        thumbnail: url.into(),
        cover: Default::default(),
        duration_secs: 0,
    };
    shell.set_search_results(ModelRc::new(VecModel::from_slice(&[item])));
    resolve_search(&shell);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        pump_events();
        let loaded = shell
            .get_search_results()
            .row_data(0)
            .is_some_and(|row| row.cover.size().width > 0);
        if loaded {
            let cover = shell.get_search_results().row_data(0).unwrap().cover;
            assert_eq!(cover.size().width, bucket);
            break;
        }
        assert!(Instant::now() < deadline, "catalog cover never loaded");
        std::thread::sleep(Duration::from_millis(10));
    }

    let now_bucket = bucket_for(300.0, shell.window().scale_factor());
    let thumbnail = "https://yt3.googleusercontent.com/now-playing=w120-h120-l90-rj";
    let now_url = sized_url(thumbnail, now_bucket);
    let art = checkerboard(Rgba([220, 10, 90, 255]), Rgba([10, 210, 90, 255]));
    cache
        .get(&now_url, Source::Bytes(png(&art)), now_bucket)
        .unwrap();
    set_now_playing_cover(&shell, thumbnail);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        pump_events();
        let cover = shell.get_cover();
        if cover.size().width > 0 {
            assert_eq!(cover.size().width, now_bucket);
            break;
        }
        assert!(Instant::now() < deadline, "now playing cover never loaded");
        std::thread::sleep(Duration::from_millis(10));
    }

    let row_bucket = bucket_for(36.0, shell.window().scale_factor());
    let row_thumbnail = "https://yt3.googleusercontent.com/detail-row=w120-h120-l90-rj";
    let row_url = sized_url(row_thumbnail, row_bucket);
    let row = CatalogItem {
        kind: DetailKind::Album,
        id: "row-1".into(),
        title: "Row".into(),
        subtitle: String::new().into(),
        duration: String::new().into(),
        playable: true,
        artist: String::new().into(),
        album: String::new().into(),
        thumbnail: row_thumbnail.into(),
        cover: Default::default(),
        duration_secs: 0,
    };
    cache
        .get(&row_url, Source::Bytes(png(&checkerboard(
            Rgba([10, 90, 220, 255]),
            Rgba([220, 200, 10, 255]),
        ))), row_bucket)
        .unwrap();
    shell.set_detail_tracks(ModelRc::new(VecModel::from_slice(&[row])));
    resolve_detail(&shell);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        pump_events();
        let loaded = shell
            .get_detail_tracks()
            .row_data(0)
            .is_some_and(|row| row.cover.size().width > 0);
        if loaded {
            let cover = shell.get_detail_tracks().row_data(0).unwrap().cover;
            assert_eq!(cover.size().width, row_bucket);
            break;
        }
        assert!(Instant::now() < deadline, "detail row cover never loaded");
        std::thread::sleep(Duration::from_millis(10));
    }
}

