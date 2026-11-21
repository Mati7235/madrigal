use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use madrigal_core::{TrackIdentity, TrackSource};
use madrigal_lyrics::Lyrics;
use madrigal_ui::{
    lyrics_delivery, maybe_fetch_lyrics, wire_lyrics_fetch, CachedLyrics, ChainSource,
    DbLyricsCache, LyricsCache, LyricsFetcher, MainShell,
};
use slint::ComponentHandle;

use crate::playback::POLL_INTERVAL;

thread_local! {
    static TRACK_POLL: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
}

pub struct EmbeddedFirstCache {
    db: DbLyricsCache,
}

impl EmbeddedFirstCache {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            db: DbLyricsCache::new(path),
        }
    }

    pub fn default_profile() -> Self {
        Self::new(madrigal_paths::db_path())
    }
}

impl LyricsCache for EmbeddedFirstCache {
    fn lookup(&self, identity: &TrackIdentity) -> Option<CachedLyrics> {
        if identity.source == TrackSource::Local {
            if let Some(lyrics) = embedded_lyrics(Path::new(&identity.id)) {
                return Some(CachedLyrics::Hit(lyrics));
            }
        }
        self.db.lookup(identity)
    }
}

fn embedded_lyrics(path: &Path) -> Option<Lyrics> {
    let pair = madrigal_lyrics::embedded::extract(path).ok()?;
    if let Some(synced) = pair.synced.filter(|lyrics| !lyrics.lines.is_empty()) {
        return Some(synced);
    }
    pair.unsynced
        .filter(|text| !text.trim().is_empty())
        .map(|text| madrigal_lyrics::lrc::parse(&text))
}

pub fn wire(shell: &MainShell) {
    let fetcher = Rc::new(LyricsFetcher::new(
        Box::new(EmbeddedFirstCache::default_profile()),
        Box::new(ChainSource::default_profile()),
        lyrics_delivery(shell),
    ));
    wire_with(shell, fetcher);
}

pub fn wire_with(shell: &MainShell, fetcher: Rc<LyricsFetcher>) {
    wire_lyrics_fetch(shell, fetcher);
    let timer = slint::Timer::default();
    let weak = shell.as_weak();
    timer.start(slint::TimerMode::Repeated, POLL_INTERVAL, move || {
        if let Some(shell) = weak.upgrade() {
            maybe_fetch_lyrics(&shell);
        }
    });
    TRACK_POLL.with(|slot| *slot.borrow_mut() = Some(timer));
}
