use crate::{Lyrics, ParseError};

pub fn parse(input: &str) -> Result<Lyrics, ParseError> {
    if input.trim().is_empty() {
        return Err(ParseError::EmptyInput);
    }
    let lyrics = crate::lrc::parse_core(input, true);
    if !lyrics.synced {
        return Err(ParseError::NoSyncedLines);
    }
    Ok(lyrics)
}
