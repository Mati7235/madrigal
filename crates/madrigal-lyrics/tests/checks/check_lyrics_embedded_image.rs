use lofty::tag::{ItemKey, Tag, TagType};
use madrigal_lyrics::embedded::{
    extract, extract_lyrics, lyrics_from_tag, sylt_entries_from_bytes,
};
use madrigal_lyrics::share_image::{ShareOptions, render, render_png};
use madrigal_lyrics::Lyrics;

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn embedded_fixtures_and_share_image() {
    let plain_path = fixture("embedded_uslt.mp3");
    let plain = extract(&plain_path).expect("extract uslt");
    assert!(plain.synced.is_none());
    assert_eq!(
        plain.unsynced.as_deref(),
        Some("First line\nSecond line")
    );
    let plain_lyrics = extract_lyrics(&plain_path).expect("lyrics uslt").expect("some");
    assert!(!plain_lyrics.synced);
    assert_eq!(plain_lyrics.lines.len(), 2);
    assert!(plain_lyrics.lines.iter().all(|line| line.start_ms == 0));
    assert_eq!(plain_lyrics.lines[0].text, "First line");

    let synced_path = fixture("embedded_sylt.mp3");
    let synced = extract(&synced_path).expect("extract sylt");
    assert!(synced.unsynced.is_none());
    let synced_lyrics = synced.synced.clone().expect("synced lyrics");
    assert!(synced_lyrics.synced);
    assert_eq!(synced_lyrics.lines.len(), 2);
    assert_eq!(synced_lyrics.lines[0].start_ms, 1200);
    assert_eq!(synced_lyrics.lines[0].text, "Hola");
    assert_eq!(synced_lyrics.lines[1].start_ms, 3400);
    assert_eq!(synced_lyrics.lines[1].text, "Mundo");
    for line in &synced_lyrics.lines {
        let words = line.words.as_ref().expect("sylt words");
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].start_ms, line.start_ms);
    }
    let via_helper = extract_lyrics(&synced_path).expect("lyrics sylt").expect("some");
    assert_eq!(via_helper, synced_lyrics);

    assert!(sylt_entries_from_bytes(b"not an id3 tag").is_empty());
    assert!(sylt_entries_from_bytes(b"ID3").is_empty());

    let mut tag = Tag::new(TagType::VorbisComments);
    tag.insert_text(ItemKey::Lyrics, "vorbis words".to_string());
    assert_eq!(lyrics_from_tag(&tag).as_deref(), Some("vorbis words"));
    let mut mp4 = Tag::new(TagType::Mp4Ilst);
    mp4.insert_text(ItemKey::Lyrics, "mp4 words".to_string());
    assert_eq!(lyrics_from_tag(&mp4).as_deref(), Some("mp4 words"));
    let empty_tag = Tag::new(TagType::Id3v2);
    assert!(lyrics_from_tag(&empty_tag).is_none());

    let options = ShareOptions {
        width: 320,
        height: 400,
        base: [18, 42, 67],
        active: Some(0),
        title: "Titulo".to_string(),
        artist: "Artista".to_string(),
    };
    let image = render(&plain_lyrics, &options);
    assert_eq!((image.width(), image.height()), (320, 400));
    let background = *image.get_pixel(0, 0);
    let distinct = image.pixels().filter(|pixel| **pixel != background).count();
    assert!(distinct > 1000, "only {distinct} painted pixels");

    let png = render_png(&Lyrics::default(), &options);
    assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    assert!(png.len() > 512);

    let active_none = ShareOptions {
        active: None,
        ..options.clone()
    };
    let other = render(&plain_lyrics, &active_none);
    assert!(other.pixels().any(|pixel| *pixel != *other.get_pixel(0, 0)));
}
