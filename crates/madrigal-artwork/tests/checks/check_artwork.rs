use image::{Rgba, RgbaImage};
use madrigal_artwork::{ArtworkCache, ArtworkError, Source};
use std::io;

fn sample_png() -> Vec<u8> {
    let mut image = RgbaImage::new(48, 48);
    for y in 0..48 {
        for x in 0..48 {
            image.put_pixel(
                x,
                y,
                if (x / 6 + y / 6) % 2 == 0 {
                    Rgba([240, 30, 20, 255])
                } else {
                    Rgba([20, 40, 240, 255])
                },
            );
        }
    }
    let mut bytes = Vec::new();
    image
        .write_to(&mut io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    bytes
}

#[test]
fn artwork_check() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_CACHE_HOME", tmp.path().join("cache"));
    let cache = ArtworkCache::with_memory_budget(tmp.path().into(), 1 << 20, 1 << 20);

    let first = cache.get("remote-track", Source::Bytes(sample_png()), 128).unwrap();
    assert_eq!(first.dimensions(), (128, 128));
    let second = cache.get("remote-track", Source::Bytes(vec![0]), 128).unwrap();
    assert_eq!(first.as_raw(), second.as_raw());

    let file = tmp.path().join("art.png");
    std::fs::write(&file, sample_png()).unwrap();
    let from_file = cache.get("local-track", Source::File(file), 96).unwrap();
    assert_eq!(from_file.dimensions(), (96, 96));
    assert!(matches!(
        cache.get("bad", Source::Bytes(vec![9, 9]), 384),
        Err(ArtworkError::Decode(_)) | Err(ArtworkError::Io(_))
    ));
    assert!(cache.disk_bytes() > 0);
}
