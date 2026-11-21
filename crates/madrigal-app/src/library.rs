use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use madrigal_core::{Event, PlaylistId, TrackIdentity};
use madrigal_library::{Library, SongSort, SortDir};
use madrigal_scrobble::Eligibility;
use madrigal_settings::{Settings, SongSortType};
use madrigal_ui::{
    album_cards, apply_library, artist_cards, chip_order, downloaded_rows, event_years,
    filter_cards, filter_downloads, filter_history, filter_playlists, filter_songs, history_rows,
    playlist_rows, song_rows, stats_streaks, stats_totals, top_album_rows, top_artist_rows,
    top_track_rows, track_source_from_index, year_model, LibraryModels, MainShell,
};
use rusqlite::Connection;
use slint::ComponentHandle;

use crate::store::Store;
use crate::ui::lang_from_settings;

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const HISTORY_MIN_PLAY_MS: i32 = 5_000;

const SORT_KEYS: [&str; 5] = [
    "library.sort.recent",
    "library.sort.name",
    "library.sort.artist",
    "library.sort.plays",
    "library.sort.playtime",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Observed {
    pub query: String,
    pub tab: i32,
    pub tag: String,
    pub sort: i32,
    pub year: i32,
    pub multiselect: bool,
    pub action_count: i32,
    pub last_action: String,
    pub last_arg: String,
    pub like_count: i32,
    pub track_id: String,
    pub track_source: i32,
    pub track_title: String,
    pub track_artist: String,
    pub position_ms: i32,
    pub duration_ms: i32,
    pub search_submit_count: i32,
    pub search_submit: String,
    pub search_remove_count: i32,
    pub search_remove: String,
    pub search_clear_count: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryView {
    pub query: String,
    pub tab: i32,
    pub tag: String,
    pub sort: i32,
    pub year: i32,
    pub selected: Vec<String>,
    pub current: Option<String>,
}

pub fn sort_for_index(index: i32) -> (SongSort, SortDir) {
    match index {
        1 => (SongSort::Name, SortDir::Asc),
        2 => (SongSort::Artist, SortDir::Asc),
        3 => (SongSort::PlayCount, SortDir::Desc),
        4 => (SongSort::PlayTime, SortDir::Desc),
        _ => (SongSort::DateAdded, SortDir::Desc),
    }
}

pub fn sort_index_for_settings(sort: SongSortType) -> i32 {
    match sort {
        SongSortType::CreateDate => 0,
        SongSortType::Name => 1,
        SongSortType::Artist => 2,
        SongSortType::PlayTime => 4,
    }
}

pub fn sort_labels(lang: madrigal_i18n::Lang) -> Vec<String> {
    SORT_KEYS
        .iter()
        .map(|key| madrigal_i18n::tr(lang, key).to_owned())
        .collect()
}

fn track_identity(source: i32, id: &str) -> Option<TrackIdentity> {
    (!id.is_empty()).then(|| TrackIdentity::new(track_source_from_index(source), id))
}

pub fn build_models(conn: &Connection, settings: &Settings, view: &LibraryView) -> LibraryModels {
    let lib = Library::new(conn, settings.clone());
    let chips = chip_order(settings);
    let recent_searches: Vec<String> = lib
        .search_history(10)
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.query)
        .collect();
    let tab = usize::try_from(view.tab)
        .ok()
        .and_then(|index| chips.get(index).cloned())
        .unwrap_or_else(|| madrigal_ui::CHIP_LIBRARY.to_owned());
    let selected: HashSet<String> = view.selected.iter().cloned().collect();
    let (sort, dir) = sort_for_index(view.sort);

    let mut models = LibraryModels {
        chips,
        sort_options: sort_labels(lang_from_settings(settings.appearance.language)),
        sort: view.sort,
        tags: lib.all_tags().unwrap_or_default(),
        selected_count: selected.len() as i32,
        recent_searches,
        ..LibraryModels::default()
    };

    match tab.as_str() {
        madrigal_ui::CHIP_PLAYLISTS => {
            let tag = (!view.tag.trim().is_empty()).then_some(view.tag.as_str());
            models.playlists = filter_playlists(playlist_rows(&lib, tag), &view.query);
        }
        madrigal_ui::CHIP_ARTISTS => {
            models.cards = filter_cards(artist_cards(&lib), &view.query);
        }
        madrigal_ui::CHIP_ALBUMS => {
            models.cards = filter_cards(album_cards(&lib), &view.query);
        }
        madrigal_ui::CHIP_HISTORY => {
            models.history = filter_history(history_rows(&lib), &view.query);
            models.history_paused = settings.privacy.pause_listen_history;
        }
        madrigal_ui::CHIP_STATS => {
            let range = settings.content.top_range;
            models.stats_totals = stats_totals(&lib);
            models.stats_streaks = stats_streaks(&lib);
            models.stat_tracks = top_track_rows(&lib, range);
            models.stat_artists = top_artist_rows(&lib, range);
            models.stat_albums = top_album_rows(&lib, range);
            let years = event_years(&lib);
            let last = years.len().saturating_sub(1);
            let index = usize::try_from(view.year).map_or(last, |year| year.min(last));
            models.year = index as i32;
            if let Some(year) = years.get(index).and_then(|value| value.parse::<i32>().ok()) {
                let (tracks, artists, albums) = year_model(&lib, year);
                models.stat_tracks = tracks;
                models.stat_artists = artists;
                models.stat_albums = albums;
            }
            models.years = years;
        }
        _ => {
            models.songs = filter_songs(song_rows(&lib, sort, dir, &selected), &view.query);
        }
    }

    models.downloads = filter_downloads(downloaded_rows(&lib), &view.query);
    if let Some(key) = view.current.as_deref() {
        let identity = TrackIdentity::parse_prefixed(key);
        models.liked = Some(lib.is_liked(&identity).unwrap_or(false));
    }
    models
}

struct Current {
    identity: TrackIdentity,
    title: String,
    artist: String,
    max_ms: i32,
    duration_ms: i32,
    recorded: bool,
}

pub struct Worker {
    conn: Connection,
    settings: Settings,
    seen: Observed,
    selected: HashSet<String>,
    opened: Option<String>,
    current: Option<Current>,
}

impl Worker {
    pub fn new(conn: Connection, settings: Settings) -> Self {
        let seen = Observed {
            sort: sort_index_for_settings(settings.content.song_sort_type),
            ..Observed::default()
        };
        Self {
            conn,
            settings,
            seen,
            selected: HashSet::new(),
            opened: None,
            current: None,
        }
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    fn library(&self) -> Library<'_> {
        Library::new(&self.conn, self.settings.clone())
    }

    fn view(&self) -> LibraryView {
        LibraryView {
            query: self.seen.query.clone(),
            tab: self.seen.tab,
            tag: self.seen.tag.clone(),
            sort: self.seen.sort,
            year: self.seen.year,
            selected: self.selected.iter().cloned().collect(),
            current: self
                .current
                .as_ref()
                .map(|current| current.identity.to_storage_string()),
        }
    }

    pub fn models(&self) -> LibraryModels {
        build_models(&self.conn, &self.settings, &self.view())
    }

    pub fn handle(&mut self, obs: Observed) -> Option<LibraryModels> {
        let mut dirty = obs.query != self.seen.query
            || obs.tab != self.seen.tab
            || obs.tag != self.seen.tag
            || obs.sort != self.seen.sort
            || obs.year != self.seen.year;
        if self.seen.multiselect && !obs.multiselect {
            self.selected.clear();
            dirty = true;
        }
        if obs.action_count != self.seen.action_count {
            dirty |= self.action(&obs.last_action, &obs.last_arg, obs.multiselect);
        }
        if obs.search_submit_count != self.seen.search_submit_count {
            let lib = self.library();
            if let Err(err) = lib.record_search(&obs.search_submit) {
                eprintln!("library: record search: {err}");
            }
            dirty = true;
        }
        if obs.search_remove_count != self.seen.search_remove_count {
            let lib = self.library();
            if let Err(err) = lib.remove_search(&obs.search_remove) {
                eprintln!("library: remove search: {err}");
            }
            dirty = true;
        }
        if obs.search_clear_count != self.seen.search_clear_count {
            let lib = self.library();
            if let Err(err) = lib.clear_search_history() {
                eprintln!("library: clear searches: {err}");
            }
            dirty = true;
        }
        if obs.track_id != self.seen.track_id || obs.track_source != self.seen.track_source {
            self.finish_current();
            self.current =
                track_identity(obs.track_source, &obs.track_id).map(|identity| Current {
                    identity,
                    title: obs.track_title.clone(),
                    artist: obs.track_artist.clone(),
                    max_ms: 0,
                    duration_ms: 0,
                    recorded: false,
                });
            dirty = true;
        }
        if let Some(current) = self.current.as_mut() {
            current.max_ms = current.max_ms.max(obs.position_ms);
            current.duration_ms = current.duration_ms.max(obs.duration_ms);
        }
        if self
            .current
            .as_ref()
            .is_some_and(|current| !current.recorded && current.max_ms >= HISTORY_MIN_PLAY_MS)
        {
            self.finish_current_early();
            dirty = true;
        }
        if obs.like_count != self.seen.like_count {
            self.toggle_current_like();
            dirty = true;
        }
        self.seen = obs;
        dirty.then(|| self.models())
    }

    fn action(&mut self, action: &str, arg: &str, multiselect: bool) -> bool {
        match action {
            "song-toggled" => {
                let identity = TrackIdentity::parse_prefixed(arg);
                let lib = self.library();
                let liked = lib.is_liked(&identity).unwrap_or(false);
                if let Err(err) = lib.set_liked(&identity, !liked) {
                    eprintln!("library: like: {err}");
                }
                true
            }
            "song-chosen" => {
                if !multiselect {
                    return false;
                }
                if !self.selected.remove(arg) {
                    self.selected.insert(arg.to_owned());
                }
                true
            }
            "selection-cleared" => {
                self.selected.clear();
                true
            }
            "playlist-opened" => {
                self.opened = (!arg.is_empty()).then(|| arg.to_owned());
                false
            }
            "tag-created" => self.playlist_tag(arg, true),
            "tag-removed" => self.playlist_tag(arg, false),
            "menu-refresh" => true,
            "history-cleared" => {
                if let Err(err) = self.library().clear_listen_history() {
                    eprintln!("library: clear history: {err}");
                }
                true
            }
            _ => false,
        }
    }

    fn playlist_tag(&mut self, tag: &str, add: bool) -> bool {
        let Some(id) = self.opened.clone() else {
            return false;
        };
        let id = PlaylistId::new(id);
        let lib = self.library();
        let result = if add {
            lib.assign_tag(&id, tag)
        } else {
            lib.remove_tag(&id, tag)
        };
        if let Err(err) = result {
            eprintln!("library: tag: {err}");
        }
        true
    }

    fn toggle_current_like(&mut self) {
        let Some(current) = self.current.as_ref() else {
            return;
        };
        let identity = current.identity.clone();
        let lib = self.library();
        let liked = lib.is_liked(&identity).unwrap_or(false);
        if let Err(err) = lib.set_liked(&identity, !liked) {
            eprintln!("library: like: {err}");
        }
    }

    fn finish_current_early(&mut self) {
        let Some(mut current) = self.current.take() else {
            return;
        };
        if current.recorded {
            self.current = Some(current);
            return;
        }
        current.recorded = true;
        let duration_secs = (current.duration_ms / 1000).max(0) as u64;
        let event = Event {
            track: current.identity.clone(),
            played_at: madrigal_library::now_unix(),
            duration_secs: Some(duration_secs as u32),
            completed: current.max_ms + 2000 >= current.duration_ms,
        };
        {
            let lib = self.library();
            if let Err(err) =
                lib.record_play_meta(&event, Some(&current.title), Some(&current.artist))
            {
                eprintln!("library: record play: {err}");
            }
        }
        self.current = Some(current);
    }

    fn finish_current(&mut self) {
        let Some(current) = self.current.take() else {
            return;
        };
        if current.recorded {
            return;
        }
        let duration_secs = (current.duration_ms / 1000).max(0) as u64;
        let played_secs = (current.max_ms / 1000).max(0) as u64;
        let eligibility = Eligibility::from_integrations(&self.settings.integrations);
        if !eligibility.eligible(duration_secs, played_secs) {
            return;
        }
        let event = Event {
            track: current.identity,
            played_at: madrigal_library::now_unix(),
            duration_secs: Some(duration_secs as u32),
            completed: current.max_ms + 2000 >= current.duration_ms,
        };
        if let Err(err) =
            self.library()
                .record_play_meta(&event, Some(&current.title), Some(&current.artist))
        {
            eprintln!("library: record play: {err}");
        }
    }
}

pub struct LibraryService {
    _poll: slint::Timer,
    _worker: Option<JoinHandle<()>>,
}

fn observe(shell: &MainShell) -> Observed {
    Observed {
        query: shell.get_local_query().to_string(),
        tab: shell.get_lib_tab(),
        tag: shell.get_lib_active_tag().to_string(),
        sort: shell.get_lib_sort(),
        year: shell.get_lib_year(),
        multiselect: shell.get_lib_multiselect(),
        action_count: shell.get_lib_action_count(),
        last_action: shell.get_lib_last_action().to_string(),
        last_arg: shell.get_lib_last_arg().to_string(),
        like_count: shell.get_like_count(),
        track_id: shell.get_track_id().to_string(),
        track_source: shell.get_track_source(),
        track_title: shell.get_track_title().to_string(),
        track_artist: shell.get_track_artist().to_string(),
        position_ms: shell.get_position_ms(),
        duration_ms: shell.get_duration_ms(),
        search_submit_count: shell.get_search_submit_count(),
        search_submit: shell.get_search_last_submitted().to_string(),
        search_remove_count: shell.get_search_remove_count(),
        search_remove: shell.get_search_last_removed().to_string(),
        search_clear_count: shell.get_search_clear_count(),
    }
}

fn deliver(weak: &slint::Weak<MainShell>, models: LibraryModels) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(shell) = weak.upgrade() {
            apply_library(&shell, &models);
        }
    });
}

fn worker_loop(rx: Receiver<Observed>, settings: Settings, shell: slint::Weak<MainShell>) {
    let conn = match madrigal_db::open_default() {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("library: db: {err}");
            return;
        }
    };
    let mut worker = Worker::new(conn, settings);
    deliver(&shell, worker.models());
    while let Ok(obs) = rx.recv() {
        if let Some(models) = worker.handle(obs) {
            deliver(&shell, models);
        }
    }
}

pub fn wire_library(shell: &MainShell, store: &Store) -> LibraryService {
    let settings = store.settings().clone();
    let lang = lang_from_settings(settings.appearance.language);
    let initial_sort = sort_index_for_settings(settings.content.song_sort_type);
    let initial = LibraryModels {
        chips: chip_order(&settings),
        sort_options: sort_labels(lang),
        sort: initial_sort,
        year: -1,
        ..LibraryModels::default()
    };
    apply_library(shell, &initial);
    shell.set_lib_sort(initial_sort);
    shell.set_lib_year(-1);

    let (tx, rx) = mpsc::channel::<Observed>();
    let worker_weak = shell.as_weak();
    let worker = thread::Builder::new()
        .name("madrigal-library".to_owned())
        .spawn(move || worker_loop(rx, settings, worker_weak))
        .ok();

    let weak = shell.as_weak();
    let poll = slint::Timer::default();
    poll.start(slint::TimerMode::Repeated, POLL_INTERVAL, move || {
        if let Some(shell) = weak.upgrade() {
            let _ = tx.send(observe(&shell));
        }
    });

    LibraryService {
        _poll: poll,
        _worker: worker,
    }
}
