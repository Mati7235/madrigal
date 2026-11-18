use madrigal_core::TrackIdentity;
use madrigal_db::{cache_lyrics, fetch_lyrics, open};
use madrigal_lyrics::translation::{
    TranslationError, TranslationProvider, attach_translation, decode_cache, encode_cache,
    translate_chain, translate_lyrics,
};
use madrigal_lyrics::lrc_enhanced;

struct ScriptProvider {
    name: String,
    output: Result<Vec<String>, String>,
}

impl TranslationProvider for ScriptProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn translate_lines(
        &self,
        _lines: &[String],
        _target: &str,
    ) -> Result<Vec<String>, TranslationError> {
        match &self.output {
            Ok(texts) => Ok(texts.clone()),
            Err(message) => Err(TranslationError::Provider(format!("{}: {message}", self.name))),
        }
    }
}

fn failing(name: &str) -> ScriptProvider {
    ScriptProvider {
        name: name.to_string(),
        output: Err("boom".to_string()),
    }
}

fn canned(name: &str, texts: &[&str]) -> ScriptProvider {
    ScriptProvider {
        name: name.to_string(),
        output: Ok(texts.iter().map(|text| text.to_string()).collect()),
    }
}

#[test]
fn mock_chain_discards_timings_and_persists_in_db() {
    let lyrics = lrc_enhanced::parse("[00:10.00]<00:10.00>Hola <00:10.60>mundo\n[00:15.00]Adios\n")
        .expect("sample");
    let first = failing("first");
    let second = canned("second", &["Hello world", "Goodbye"]);
    let translated = translate_lyrics(&lyrics, &[&first, &second], "en").expect("chain");
    assert_eq!(translated.lines.len(), 2);
    assert_eq!(translated.lines[0].text, "Hola mundo");
    assert_eq!(
        translated.lines[0].markers.translation.as_deref(),
        Some("Hello world")
    );
    assert_eq!(
        translated.lines[1].markers.translation.as_deref(),
        Some("Goodbye")
    );
    assert_eq!(translated.lines[0].start_ms, 10_000);
    assert_eq!(translated.lines[1].start_ms, 15_000);
    assert_eq!(translated.lines[0].words, lyrics.lines[0].words);

    let source: Vec<String> = lyrics.lines.iter().map(|line| line.text.clone()).collect();
    let chained = translate_chain(&[&first, &second], &source, "en").expect("fallback");
    assert_eq!(chained, vec!["Hello world".to_string(), "Goodbye".to_string()]);
    assert!(matches!(
        translate_chain(&[&first], &source, "en"),
        Err(TranslationError::Provider(_))
    ));
    assert!(matches!(
        translate_chain(&[&second], &source, "  "),
        Err(TranslationError::EmptyTarget)
    ));
    assert!(matches!(
        translate_chain(&[], &source, "en"),
        Err(TranslationError::NoProvider)
    ));
    assert!(matches!(
        attach_translation(&lyrics, &["solo".to_string()]),
        Err(TranslationError::MismatchedLines { .. })
    ));

    let texts: Vec<String> = translated
        .lines
        .iter()
        .filter_map(|line| line.markers.translation.clone())
        .collect();
    let payload = encode_cache("en", &texts);
    let dir = tempfile::tempdir().expect("tempdir");
    let conn = open(&dir.path().join("lyrics.db")).expect("open");
    let identity = TrackIdentity::youtube("VALIDID0001");
    cache_lyrics(&conn, &identity, &payload, 1_700_000_000).expect("cache");
    let stored = fetch_lyrics(&conn, &identity).expect("fetch").expect("row");
    let (target, lines) = decode_cache(&stored).expect("decode");
    assert_eq!(target, "en");
    assert_eq!(lines, texts);
    assert!(decode_cache("not json").is_none());
}
