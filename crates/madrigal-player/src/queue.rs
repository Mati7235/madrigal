use madrigal_core::{Track, TrackIdentity};
use madrigal_settings::RepeatMode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackMeta {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub duration_secs: Option<u32>,
    #[serde(default)]
    pub thumbnail: Option<String>,
}

impl TrackMeta {
    pub fn from_track(track: &Track) -> Self {
        Self {
            title: track.title.clone(),
            artist: track.artist_text(),
            album: track.album.clone().unwrap_or_default(),
            duration_secs: track.duration_secs,
            thumbnail: track.thumbnail.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueEntry {
    #[serde(flatten)]
    pub identity: TrackIdentity,
    #[serde(flatten, default)]
    pub meta: TrackMeta,
}

impl QueueEntry {
    pub fn new(identity: TrackIdentity) -> Self {
        Self {
            identity,
            meta: TrackMeta::default(),
        }
    }

    pub fn with_meta(identity: TrackIdentity, meta: TrackMeta) -> Self {
        Self { identity, meta }
    }
}

impl From<TrackIdentity> for QueueEntry {
    fn from(identity: TrackIdentity) -> Self {
        Self::new(identity)
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Queue {
    items: Vec<QueueEntry>,
    order: Vec<usize>,
    cursor: Option<usize>,
    repeat: RepeatMode,
    shuffle: bool,
    permanent_shuffle: bool,
    locked: bool,
    rng: u64,
}

impl Queue {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            order: Vec::new(),
            cursor: None,
            repeat: RepeatMode::Off,
            shuffle: false,
            permanent_shuffle: false,
            locked: false,
            rng: 0x1234_5678_9ABC_DEF0,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn repeat(&self) -> RepeatMode {
        self.repeat
    }

    pub fn set_repeat(&mut self, mode: RepeatMode) {
        self.repeat = mode;
    }

    pub fn is_shuffled(&self) -> bool {
        self.shuffle || self.permanent_shuffle
    }

    pub fn permanent_shuffle(&self) -> bool {
        self.permanent_shuffle
    }

    pub fn locked(&self) -> bool {
        self.locked
    }

    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }

    pub fn set_shuffle(&mut self, enabled: bool) {
        let was = self.is_shuffled();
        self.shuffle = enabled;
        if enabled && !was {
            let seed = self.next_seed();
            self.apply_shuffle(seed);
        } else if !self.is_shuffled() {
            self.apply_identity();
        }
    }

    pub fn reshuffle(&mut self, seed: u64) {
        self.shuffle = true;
        self.apply_shuffle(seed);
    }

    pub fn set_permanent_shuffle(&mut self, enabled: bool) {
        let was = self.is_shuffled();
        self.permanent_shuffle = enabled;
        if enabled && !was {
            let seed = self.next_seed();
            self.apply_shuffle(seed);
        } else if !self.is_shuffled() {
            self.apply_identity();
        }
    }

    pub fn replace(&mut self, tracks: Vec<TrackIdentity>, start: usize) -> bool {
        self.replace_entries(tracks.into_iter().map(QueueEntry::new).collect(), start)
    }

    pub fn replace_entries(&mut self, entries: Vec<QueueEntry>, start: usize) -> bool {
        if self.locked {
            return false;
        }
        self.items = entries;
        if self.items.is_empty() {
            self.order.clear();
            self.cursor = None;
            return true;
        }
        let start = start.min(self.items.len() - 1);
        if self.is_shuffled() {
            let seed = self.next_seed();
            self.order = self.deal(seed);
            self.cursor = self.order.iter().position(|&x| x == start);
        } else {
            self.order = (0..self.items.len()).collect();
            self.cursor = Some(start);
        }
        true
    }

    pub fn push(&mut self, track: TrackIdentity) -> bool {
        self.insert(self.items.len(), track)
    }

    pub fn insert(&mut self, at: usize, track: TrackIdentity) -> bool {
        self.insert_entry(at, QueueEntry::new(track))
    }

    pub fn push_entry(&mut self, entry: QueueEntry) -> bool {
        self.insert_entry(self.items.len(), entry)
    }

    pub fn insert_entry(&mut self, at: usize, entry: QueueEntry) -> bool {
        if self.locked {
            return false;
        }
        let at = at.min(self.items.len());
        self.items.insert(at, entry);
        for slot in self.order.iter_mut() {
            if *slot >= at {
                *slot += 1;
            }
        }
        self.order.push(at);
        if self.cursor.is_none() {
            self.cursor = self.order.iter().position(|&x| x == at);
        }
        true
    }

    pub fn remove(&mut self, index: usize) -> bool {
        if self.locked || index >= self.items.len() {
            return false;
        }
        let current_item = self.cursor.and_then(|c| self.order.get(c).copied());
        let current_pos = self.cursor;
        self.items.remove(index);
        self.order.retain(|&x| x != index);
        for slot in self.order.iter_mut() {
            if *slot > index {
                *slot -= 1;
            }
        }
        if self.order.is_empty() {
            self.cursor = None;
        } else {
            match current_item {
                Some(item) if item == index => {
                    let pos = current_pos.unwrap_or(0).min(self.order.len() - 1);
                    self.cursor = Some(pos);
                }
                Some(item) => {
                    let fixed = if item > index { item - 1 } else { item };
                    self.cursor = self.order.iter().position(|&x| x == fixed);
                }
                None => {}
            }
        }
        true
    }

    pub fn move_item(&mut self, from: usize, to: usize) -> bool {
        if self.locked || from >= self.items.len() || to >= self.items.len() {
            return false;
        }
        if from == to {
            return true;
        }
        let current_item = self.cursor.and_then(|c| self.order.get(c).copied());
        let item = self.items.remove(from);
        self.items.insert(to, item);
        let map = |old: usize| {
            if old == from {
                to
            } else {
                let mut n = if old > from { old - 1 } else { old };
                if n >= to {
                    n += 1;
                }
                n
            }
        };
        for slot in self.order.iter_mut() {
            *slot = map(*slot);
        }
        self.cursor = current_item.and_then(|item| self.order.iter().position(|&x| x == map(item)));
        true
    }

    pub fn clear(&mut self) -> bool {
        if self.locked {
            return false;
        }
        self.items.clear();
        self.order.clear();
        self.cursor = None;
        true
    }

    pub fn current_entry(&self) -> Option<&QueueEntry> {
        self.current_index().and_then(|i| self.items.get(i))
    }

    pub fn current(&self) -> Option<&TrackIdentity> {
        self.current_entry().map(|entry| &entry.identity)
    }

    pub fn current_index(&self) -> Option<usize> {
        self.cursor.and_then(|c| self.order.get(c).copied())
    }

    pub fn items(&self) -> &[QueueEntry] {
        &self.items
    }

    pub fn order_indices(&self) -> &[usize] {
        &self.order
    }

    pub fn ordered(&self) -> Vec<&QueueEntry> {
        self.order.iter().filter_map(|&i| self.items.get(i)).collect()
    }

    pub fn play_at(&mut self, index: usize) -> Option<&TrackIdentity> {
        if index >= self.items.len() {
            return None;
        }
        self.cursor = self.order.iter().position(|&x| x == index);
        self.current()
    }

    pub fn advance(&mut self) -> Option<&TrackIdentity> {
        match self.step_index(self.cursor) {
            Some(next) => {
                self.cursor = Some(next);
                self.current()
            }
            None => None,
        }
    }

    pub fn back(&mut self) -> Option<&TrackIdentity> {
        match self.cursor {
            None => None,
            Some(0) => self.current(),
            Some(c) => {
                self.cursor = Some(c - 1);
                self.current()
            }
        }
    }

    pub fn peek_next(&self) -> Option<&TrackIdentity> {
        self.step_index(self.cursor)
            .and_then(|n| self.order.get(n))
            .and_then(|&i| self.items.get(i))
            .map(|entry| &entry.identity)
    }

    fn step_index(&self, from: Option<usize>) -> Option<usize> {
        match from {
            None if self.order.is_empty() => None,
            None => Some(0),
            Some(c) => match self.repeat {
                RepeatMode::One => Some(c),
                _ if c + 1 < self.order.len() => Some(c + 1),
                RepeatMode::All => Some(0),
                RepeatMode::Off => None,
            },
        }
    }

    fn next_seed(&mut self) -> u64 {
        splitmix64(&mut self.rng)
    }

    fn deal(&self, seed: u64) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.items.len()).collect();
        let mut state = seed;
        for i in (1..order.len()).rev() {
            let j = (splitmix64(&mut state) % (i as u64 + 1)) as usize;
            order.swap(i, j);
        }
        order
    }

    fn apply_shuffle(&mut self, seed: u64) {
        let keep = self.cursor.and_then(|c| self.order.get(c).copied());
        let mut order = self.deal(seed);
        if let (Some(item), Some(c)) = (keep, self.cursor) {
            if c < order.len() {
                if let Some(found) = order.iter().position(|&x| x == item) {
                    order.swap(found, c);
                }
            }
        }
        self.order = order;
    }

    fn apply_identity(&mut self) {
        let keep = self.cursor.and_then(|c| self.order.get(c).copied());
        self.order = (0..self.items.len()).collect();
        self.cursor = keep;
    }
}

impl Default for Queue {
    fn default() -> Self {
        Self::new()
    }
}
