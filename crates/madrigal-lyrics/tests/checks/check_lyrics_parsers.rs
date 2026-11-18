use madrigal_lyrics::{ParseError, lrc, lrc_enhanced, sylt, to_lrc, ttml};
use madrigal_lyrics::sylt::SyltEntry;

fn isolate_xdg() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for (key, sub) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let path = dir.path().join(sub);
        std::fs::create_dir_all(&path).expect("xdg subdir");
        std::env::set_var(key, &path);
    }
    dir
}

fn fixture(name: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&path).expect("read fixture")
}

fn sylt_fixture() -> Vec<SyltEntry> {
    fixture("sylt.tsv")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (stamp, text) = line.split_once('\t').expect("sylt row");
            SyltEntry::new(stamp.parse().expect("sylt stamp"), text)
        })
        .collect()
}

#[test]
fn plain_lrc_timings_and_metadata() {
    let _xdg = isolate_xdg();
    let lyrics = lrc::parse(&fixture("plain.lrc"));
    assert!(lyrics.synced);
    assert!(!lyrics.has_words());
    assert_eq!(lyrics.lines.len(), 4);
    assert!(lyrics.lines.iter().all(|line| line.words.is_none()));
    assert!(
        lyrics
            .lines
            .iter()
            .all(|line| !line.text.contains("Artista"))
    );
    let starts: Vec<i64> = lyrics.lines.iter().map(|line| line.start_ms).collect();
    assert_eq!(starts, vec![12300, 15800, 62000, 80500]);
    assert_eq!(lyrics.lines[0].text, "Primera línea");
    assert_eq!(lyrics.lines[3].text, "Estribillo repetido");
    assert_eq!(
        lyrics.plain_text(),
        "Primera línea\nSegunda línea\nEstribillo repetido\nEstribillo repetido"
    );
}

#[test]
fn offset_tag_shifts_timings() {
    let _xdg = isolate_xdg();
    let lyrics = lrc::parse("[offset:+2000]\n[00:10.00]hi\n[00:20.00]bye\n");
    assert_eq!(lyrics.lines.len(), 2);
    assert_eq!(lyrics.lines[0].start_ms, 12000);
    assert_eq!(lyrics.lines[1].start_ms, 22000);
    let lyrics = lrc::parse("[offset:-500]\n[00:10.00]hi\n");
    assert_eq!(lyrics.lines[0].start_ms, 9500);
    let lyrics = lrc::parse("[offset:+1000]\n[ar:Someone]\n[00:10.00]hi\n");
    assert_eq!(lyrics.lines.len(), 1);
    assert_eq!(lyrics.lines[0].start_ms, 11000);
}

#[test]
fn enhanced_lrc_word_timings() {
    let _xdg = isolate_xdg();
    let lyrics = lrc_enhanced::parse(&fixture("enhanced.lrc")).expect("enhanced");
    assert!(lyrics.synced);
    assert!(lyrics.has_words());
    assert_eq!(lyrics.lines.len(), 1);
    let line = &lyrics.lines[0];
    assert_eq!(line.start_ms, 10000);
    assert_eq!(line.text, "Hola mundo cruel");
    let words = line.words.as_ref().expect("words");
    assert_eq!(words.len(), 3);
    assert_eq!(
        words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>(),
        vec!["Hola", "mundo", "cruel"]
    );
    assert_eq!(
        words.iter().map(|word| word.start_ms).collect::<Vec<_>>(),
        vec![10000, 10600, 11200]
    );
    let roundtrip = lrc_enhanced::parse(&to_lrc(&lyrics)).expect("roundtrip");
    assert_eq!(roundtrip.lines.len(), 1);
    assert_eq!(roundtrip.lines[0].text, "Hola mundo cruel");
    assert_eq!(
        roundtrip.lines[0]
            .words
            .as_ref()
            .expect("roundtrip words")
            .len(),
        3
    );
}

#[test]
fn ttml_chorus_excluded() {
    let _xdg = isolate_xdg();
    let lyrics = ttml::parse(&fixture("chorus.ttml")).expect("ttml");
    assert!(lyrics.synced);
    assert_eq!(lyrics.lines.len(), 2);
    assert_eq!(lyrics.lines[0].start_ms, 12340);
    assert_eq!(lyrics.lines[0].text, "Hola mundo");
    assert_eq!(lyrics.lines[1].start_ms, 15800);
    assert_eq!(lyrics.lines[1].text, "Principal");
    assert!(
        lyrics
            .lines
            .iter()
            .all(|line| !line.text.contains("coro"))
    );
    assert_eq!(to_lrc(&lyrics), "[00:12.34]Hola mundo\n[00:15.80]Principal");
}

#[test]
fn ttml_word_timings_and_markers() {
    let _xdg = isolate_xdg();
    let lyrics = ttml::parse(&fixture("words.ttml")).expect("ttml");
    assert_eq!(lyrics.lines.len(), 1);
    let line = &lyrics.lines[0];
    assert_eq!(line.start_ms, 10000);
    assert_eq!(line.end_ms, Some(13000));
    assert_eq!(line.text, "Hola mundo cruel");
    assert_eq!(line.markers.background, vec!["coro".to_string()]);
    assert_eq!(line.markers.romanization, None);
    let words = line.words.as_ref().expect("words");
    assert_eq!(words.len(), 3);
    assert_eq!(
        words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>(),
        vec!["Hola", "mundo", "cruel"]
    );
    assert_eq!(
        words.iter().map(|word| word.start_ms).collect::<Vec<_>>(),
        vec![10000, 10600, 11200]
    );
    assert_eq!(
        words
            .iter()
            .map(|word| word.end_ms)
            .collect::<Vec<_>>(),
        vec![Some(10600), Some(11200), Some(12000)]
    );
    assert!(words.iter().all(|word| word.text != "coro"));
    assert_eq!(
        to_lrc(&lyrics),
        "[00:10.00]<00:10.00>Hola <00:10.60>mundo <00:11.20>cruel"
    );
    let roundtrip = lrc_enhanced::parse(&to_lrc(&lyrics)).expect("roundtrip");
    assert_eq!(roundtrip.lines[0].text, "Hola mundo cruel");
    assert_eq!(
        roundtrip.lines[0]
            .words
            .as_ref()
            .expect("roundtrip words")
            .len(),
        3
    );
}

#[test]
fn ttml_parse_time_units() {
    let _xdg = isolate_xdg();
    assert_eq!(ttml::parse_time("00:12.340"), Some(12.34));
    assert_eq!(ttml::parse_time("15.8s"), Some(15.8));
    assert_eq!(ttml::parse_time("500ms"), Some(0.5));
    assert_eq!(ttml::parse_time("2m"), Some(120.0));
    assert_eq!(ttml::parse_time("1h"), Some(3600.0));
    assert_eq!(ttml::parse_time("90f"), Some(3.0));
    assert_eq!(ttml::parse_time("1:02:03"), Some(3723.0));
    assert_eq!(ttml::parse_time(""), None);
    assert_eq!(ttml::parse_time("not-a-time"), None);
}

#[test]
fn sylt_entries_sorted() {
    let _xdg = isolate_xdg();
    let mut entries = sylt_fixture();
    entries.reverse();
    let lyrics = sylt::parse(&entries).expect("sylt");
    assert!(lyrics.synced);
    assert!(lyrics.has_words());
    assert_eq!(lyrics.lines.len(), 2);
    assert_eq!(lyrics.lines[0].start_ms, 12340);
    assert_eq!(lyrics.lines[0].text, "Hola");
    assert_eq!(lyrics.lines[1].start_ms, 45670);
    assert_eq!(lyrics.lines[1].text, "Mundo");
    for line in &lyrics.lines {
        let words = line.words.as_ref().expect("sylt words");
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].start_ms, line.start_ms);
        assert_eq!(words[0].text, line.text);
    }
    assert_eq!(
        to_lrc(&lyrics),
        "[00:12.34]<00:12.34>Hola\n[00:45.67]<00:45.67>Mundo"
    );
}

#[test]
fn malformed_input_leniency() {
    let _xdg = isolate_xdg();
    let empty = lrc::parse("");
    assert!(!empty.synced);
    assert!(empty.lines.is_empty());
    let junk = lrc::parse("[00:0x.00]junk\n[unclosed\nplain");
    assert!(!junk.synced);
    assert_eq!(junk.lines.len(), 3);
    assert!(junk.lines.iter().all(|line| line.start_ms == 0));
    assert_eq!(lrc::parse("[ar:Someone]\n[ti:Title]\n").lines.len(), 0);
    assert_eq!(ttml::parse(""), Err(ParseError::EmptyInput));
    assert_eq!(
        ttml::parse("<html>not lyrics</html>"),
        Err(ParseError::NoSyncedLines)
    );
    assert_eq!(
        ttml::parse("<tt><body><p>no begin</p></body></tt>"),
        Err(ParseError::NoSyncedLines)
    );
    assert_eq!(lrc_enhanced::parse(""), Err(ParseError::EmptyInput));
    assert_eq!(
        lrc_enhanced::parse("just plain text"),
        Err(ParseError::NoSyncedLines)
    );
    assert_eq!(sylt::parse(&[]), Err(ParseError::EmptyInput));
    assert_eq!(
        sylt::parse(&[SyltEntry::new(1000, "   ")]),
        Err(ParseError::NoSyncedLines)
    );
}

#[test]
fn to_lrc_plain_roundtrip() {
    let _xdg = isolate_xdg();
    let lyrics = lrc::parse(&fixture("plain.lrc"));
    let rendered = to_lrc(&lyrics);
    assert_eq!(
        rendered,
        "[00:12.30]Primera línea\n[00:15.80]Segunda línea\n[01:02.00]Estribillo repetido\n[01:20.50]Estribillo repetido"
    );
    let back = lrc::parse(&rendered);
    assert_eq!(
        back.lines
            .iter()
            .map(|line| line.start_ms)
            .collect::<Vec<_>>(),
        vec![12300, 15800, 62000, 80500]
    );
}
