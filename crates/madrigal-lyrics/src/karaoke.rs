use crate::{LyricLine, Lyrics};

pub fn line_at(lyrics: &Lyrics, t_ms: i64) -> Option<usize> {
    let mut active = None;
    for (index, line) in lyrics.lines.iter().enumerate() {
        if line.start_ms <= t_ms {
            active = Some(index);
        } else {
            break;
        }
    }
    active
}

pub fn word_at(line: &LyricLine, t_ms: i64) -> Option<usize> {
    let words = line.words.as_ref().filter(|words| !words.is_empty())?;
    if t_ms < words[0].start_ms {
        return None;
    }
    let mut active = 0;
    for (index, word) in words.iter().enumerate() {
        if word.start_ms <= t_ms {
            active = index;
        } else {
            break;
        }
    }
    Some(active)
}

pub fn next_line_start(lyrics: &Lyrics, t_ms: i64) -> Option<i64> {
    lyrics
        .lines
        .iter()
        .map(|line| line.start_ms)
        .filter(|start| *start > t_ms)
        .min()
}

pub fn progress_within(line: &LyricLine, t_ms: i64) -> f32 {
    match line.words.as_ref().filter(|words| !words.is_empty()) {
        Some(words) => word_progress(words, t_ms),
        None => plain_progress(line, t_ms),
    }
}

fn word_progress(words: &[crate::WordTiming], t_ms: i64) -> f32 {
    if t_ms < words[0].start_ms {
        return 0.0;
    }
    let mut active = 0;
    for (index, word) in words.iter().enumerate() {
        if word.start_ms <= t_ms {
            active = index;
        } else {
            break;
        }
    }
    let word = &words[active];
    let end = word
        .end_ms
        .or_else(|| words.get(active + 1).map(|next| next.start_ms));
    match end {
        Some(end) if end > word.start_ms => {
            ((t_ms - word.start_ms) as f32 / (end - word.start_ms) as f32).clamp(0.0, 1.0)
        }
        _ => 1.0,
    }
}

fn plain_progress(line: &LyricLine, t_ms: i64) -> f32 {
    match line.end_ms {
        Some(end) if end > line.start_ms => {
            ((t_ms - line.start_ms) as f32 / (end - line.start_ms) as f32).clamp(0.0, 1.0)
        }
        _ => {
            if t_ms < line.start_ms {
                0.0
            } else {
                1.0
            }
        }
    }
}
