use madrigal_lyrics::karaoke::{line_at, next_line_start, progress_within, word_at};
use madrigal_lyrics::{LineMarkers, LyricLine, Lyrics, WordTiming};

fn sample() -> Lyrics {
    Lyrics {
        lines: vec![
            LyricLine {
                start_ms: 0,
                end_ms: None,
                text: "Alpha".to_string(),
                words: Some(vec![
                    WordTiming {
                        start_ms: 0,
                        end_ms: None,
                        text: "A".to_string(),
                    },
                    WordTiming {
                        start_ms: 500,
                        end_ms: Some(900),
                        text: "lpha".to_string(),
                    },
                ]),
                markers: LineMarkers::default(),
            },
            LyricLine {
                start_ms: 2000,
                end_ms: None,
                text: "Beta".to_string(),
                words: None,
                markers: LineMarkers::default(),
            },
            LyricLine {
                start_ms: 5000,
                end_ms: Some(7000),
                text: "Gamma".to_string(),
                words: None,
                markers: LineMarkers::default(),
            },
        ],
        synced: true,
    }
}

fn approx(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn boundary_math_lookahead_and_sweeps() {
    let lyrics = sample();
    assert_eq!(line_at(&lyrics, -1), None);
    assert_eq!(line_at(&lyrics, 0), Some(0));
    assert_eq!(line_at(&lyrics, 1999), Some(0));
    assert_eq!(line_at(&lyrics, 2000), Some(1));
    assert_eq!(line_at(&lyrics, 4999), Some(1));
    assert_eq!(line_at(&lyrics, 5000), Some(2));
    assert_eq!(line_at(&lyrics, 99_999), Some(2));
    assert_eq!(line_at(&lyrics, 1500), Some(0));

    let first = &lyrics.lines[0];
    assert_eq!(word_at(first, -1), None);
    assert_eq!(word_at(first, 0), Some(0));
    assert_eq!(word_at(first, 499), Some(0));
    assert_eq!(word_at(first, 500), Some(1));
    assert_eq!(word_at(first, 50_000), Some(1));
    assert_eq!(word_at(&lyrics.lines[1], 2500), None);

    assert_eq!(next_line_start(&lyrics, -1), Some(0));
    assert_eq!(next_line_start(&lyrics, 0), Some(2000));
    assert_eq!(next_line_start(&lyrics, 1999), Some(2000));
    assert_eq!(next_line_start(&lyrics, 2000), Some(5000));
    assert_eq!(next_line_start(&lyrics, 5000), None);
    assert_eq!(next_line_start(&lyrics, 6000), None);

    approx(progress_within(first, -10), 0.0);
    approx(progress_within(first, 0), 0.0);
    approx(progress_within(first, 250), 0.5);
    approx(progress_within(first, 500), 0.0);
    approx(progress_within(first, 700), 0.5);
    approx(progress_within(first, 900), 1.0);
    approx(progress_within(first, 5000), 1.0);

    let plain = &lyrics.lines[1];
    approx(progress_within(plain, 1999), 0.0);
    approx(progress_within(plain, 2000), 1.0);
    approx(progress_within(plain, 9999), 1.0);

    let ranged = &lyrics.lines[2];
    approx(progress_within(ranged, 4000), 0.0);
    approx(progress_within(ranged, 5000), 0.0);
    approx(progress_within(ranged, 6000), 0.5);
    approx(progress_within(ranged, 7000), 1.0);
    approx(progress_within(ranged, 8000), 1.0);

    let empty = Lyrics::default();
    assert_eq!(line_at(&empty, 1000), None);
    assert_eq!(next_line_start(&empty, 1000), None);
}
