use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

use madrigal_artwork::{ArtworkCache, Source as ArtworkSource};
use madrigal_core::{Album, Artist, Track, TrackIdentity};
use madrigal_innertube::{
    AlbumPage, ArtistPage, AuthCatalogClient, BrowsePage, CatalogClient, ChartsPage,
    Error as InnertubeError, HomeFeed, Locale, MoodListing, NewReleases, PlaylistPage, SearchFilter,
    SearchOutcome,
};
use madrigal_settings::{Content, SearchSource, Settings};
use madrigal_ui::{
    album_results, apply_album_detail, apply_artist_detail, apply_browse, apply_browse_detail,
    apply_charts_entry, apply_detail_error, apply_home, apply_home_chip, apply_moods,
    apply_playlist_detail, apply_releases, apply_search_partial, apply_search_results,
    apply_suggestions, artist_results, begin_detail_load, home_chips, song_results, CatalogItem,
    DetailKind, DetailRef, MainShell,
};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::playback::Playback;

const EXPLORE_BROWSE_ID: &str = "FEmusic_explore";
const MOODS_BROWSE_ID: &str = "FEmusic_moods_and_genres_category";
const SOURCE_LOCAL: i32 = 0;
const SOURCE_ONLINE: i32 = 1;

pub type HomeSlot = Mutex<Option<HomeFeed>>;
pub type Delivery = Arc<dyn Fn(CatalogEvent) + Send + Sync>;

#[derive(Default)]
pub struct SearchAggregate {
    request_id: u64,
    songs: Vec<Track>,
    artists: Vec<Artist>,
    albums: Vec<Album>,
    failed: bool,
}

pub type SearchSlot = Mutex<SearchAggregate>;

impl SearchAggregate {
    fn apply(&mut self, request_id: u64, outcome: Option<SearchOutcome>) {
        if self.request_id != request_id {
            self.request_id = request_id;
            self.songs.clear();
            self.artists.clear();
            self.albums.clear();
            self.failed = false;
        }
        match outcome {
            Some(SearchOutcome::Songs(songs)) => self.songs = songs,
            Some(SearchOutcome::Artists(artists)) => self.artists = artists,
            Some(SearchOutcome::Albums(albums)) => self.albums = albums,
            None => self.failed = true,
        }
    }

    fn items(&self) -> Vec<CatalogItem> {
        madrigal_ui::mixed_results(&self.songs, &self.artists, &self.albums)
    }

    fn is_empty(&self) -> bool {
        self.songs.is_empty() && self.artists.is_empty() && self.albums.is_empty()
    }

    fn failed(&self) -> bool {
        self.failed
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DetailPage {
    Album(AlbumPage),
    Artist(ArtistPage),
    Playlist(PlaylistPage),
    Browse(BrowsePage),
}

impl DetailPage {
    pub fn cover(&self) -> Option<&str> {
        match self {
            Self::Album(page) => page.album.thumbnail.as_deref(),
            Self::Artist(page) => page.artist.thumbnail.as_deref(),
            Self::Playlist(page) => page.playlist.thumbnail.as_deref(),
            Self::Browse(page) => page
                .sections
                .iter()
                .flat_map(|section| section.items.iter())
                .find_map(|item| match item {
                    madrigal_innertube::BrowseItem::Track(track) => track.thumbnail.as_deref(),
                    madrigal_innertube::BrowseItem::Album(album) => album.thumbnail.as_deref(),
                    madrigal_innertube::BrowseItem::Artist(artist) => artist.thumbnail.as_deref(),
                    madrigal_innertube::BrowseItem::Playlist(playlist) => {
                        playlist.thumbnail.as_deref()
                    }
                }),
        }
    }
}

pub trait CatalogSource {
    fn home(&mut self) -> Result<HomeFeed, InnertubeError>;
    fn suggestions(&mut self, query: &str) -> Result<Vec<String>, InnertubeError>;
    fn search(
        &mut self,
        query: &str,
        filter: SearchFilter,
    ) -> Result<SearchOutcome, InnertubeError>;
    fn explore(&mut self, country: &str) -> Result<ExplorePages, InnertubeError>;
    fn detail(&mut self, kind: DetailKind, id: &str) -> Result<DetailPage, InnertubeError>;
    fn set_session(&mut self, _cookie: Option<String>) {}
}

pub struct ExplorePages {
    pub moods: MoodListing,
    pub releases: NewReleases,
    pub browse: BrowsePage,
    pub charts: ChartsPage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSlot {
    Home,
    Suggestions,
    Search,
    Explore,
    Detail,
}

#[derive(Default)]
pub struct Generations {
    home: AtomicU64,
    suggestions: AtomicU64,
    search: AtomicU64,
    explore: AtomicU64,
    detail: AtomicU64,
}

impl Generations {
    fn counter(&self, slot: CatalogSlot) -> &AtomicU64 {
        match slot {
            CatalogSlot::Home => &self.home,
            CatalogSlot::Suggestions => &self.suggestions,
            CatalogSlot::Search => &self.search,
            CatalogSlot::Explore => &self.explore,
            CatalogSlot::Detail => &self.detail,
        }
    }

    pub fn begin(&self, slot: CatalogSlot) -> u64 {
        self.counter(slot).fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn is_current(&self, slot: CatalogSlot, id: u64) -> bool {
        self.counter(slot).load(Ordering::SeqCst) == id
    }
}

pub enum CatalogEvent {
    Home {
        request_id: u64,
        feed: HomeFeed,
    },
    Suggestions {
        request_id: u64,
        query: String,
        suggestions: Vec<String>,
    },
    Search {
        request_id: u64,
        outcome: SearchOutcome,
    },
    SearchKind {
        request_id: u64,
        outcome: Option<SearchOutcome>,
    },
    SearchDone {
        request_id: u64,
    },
    Explore {
        request_id: u64,
        pages: ExplorePages,
    },
    Detail {
        request_id: u64,
        page: DetailPage,
        title: String,
    },
    Failed {
        slot: CatalogSlot,
        request_id: u64,
        message: String,
    },
}

enum Request {
    Home {
        id: u64,
    },
    Suggestions {
        id: u64,
        query: String,
    },
    Search {
        id: u64,
        query: String,
        filter: SearchFilter,
    },
    SearchMixed {
        id: u64,
        query: String,
    },
    Explore {
        id: u64,
        country: String,
    },
    Detail {
        id: u64,
        reference: DetailRef,
    },
    SetSession {
        cookie: Option<String>,
    },
}

impl Request {
    fn slot_and_id(&self) -> Option<(CatalogSlot, u64)> {
        match self {
            Request::Home { id } => Some((CatalogSlot::Home, *id)),
            Request::Suggestions { id, .. } => Some((CatalogSlot::Suggestions, *id)),
            Request::Search { id, .. } | Request::SearchMixed { id, .. } => {
                Some((CatalogSlot::Search, *id))
            }
            Request::Explore { id, .. } => Some((CatalogSlot::Explore, *id)),
            Request::Detail { id, .. } => Some((CatalogSlot::Detail, *id)),
            Request::SetSession { .. } => None,
        }
    }
}

fn settle(
    slot: CatalogSlot,
    request_id: u64,
    result: Result<CatalogEvent, InnertubeError>,
) -> CatalogEvent {
    result.unwrap_or_else(|err| CatalogEvent::Failed {
        slot,
        request_id,
        message: err.to_string(),
    })
}

fn run_worker(
    mut source: Box<dyn CatalogSource + Send>,
    requests: Receiver<Request>,
    generations: Arc<Generations>,
    delivery: Delivery,
) {
    'requests: for request in requests {
        if let Request::SetSession { cookie } = request {
            source.set_session(cookie);
            continue;
        }
        let Some((slot, id)) = request.slot_and_id() else {
            continue;
        };
        if !generations.is_current(slot, id) {
            continue;
        }
        let event = match request {
            Request::Home { id } => settle(
                slot,
                id,
                source.home().map(|feed| CatalogEvent::Home {
                    request_id: id,
                    feed,
                }),
            ),
            Request::Suggestions { id, query } => settle(
                slot,
                id,
                source
                    .suggestions(&query)
                    .map(|suggestions| CatalogEvent::Suggestions {
                        request_id: id,
                        query,
                        suggestions,
                    }),
            ),
            Request::Search { id, query, filter } => settle(
                slot,
                id,
                source
                    .search(&query, filter)
                    .map(|outcome| CatalogEvent::Search {
                        request_id: id,
                        outcome,
                    }),
            ),
            Request::SearchMixed { id, query } => {
                for filter in [
                    SearchFilter::Songs,
                    SearchFilter::Artists,
                    SearchFilter::Albums,
                ] {
                    if !generations.is_current(slot, id) {
                        continue 'requests;
                    }
                    delivery(CatalogEvent::SearchKind {
                        request_id: id,
                        outcome: search_kind(source.as_mut(), &query, filter),
                    });
                }
                if generations.is_current(slot, id) {
                    delivery(CatalogEvent::SearchDone { request_id: id });
                }
                continue;
            }
            Request::Explore { id, country } => settle(
                slot,
                id,
                source.explore(&country).map(|pages| CatalogEvent::Explore {
                    request_id: id,
                    pages,
                }),
            ),
            Request::Detail { id, reference } => settle(
                slot,
                id,
                source
                    .detail(reference.kind, reference.id.as_str())
                    .map(|page| CatalogEvent::Detail {
                        request_id: id,
                        page,
                        title: reference.title.to_string(),
                    }),
            ),
            Request::SetSession { .. } => continue,
        };
        delivery(event);
    }
}

fn search_kind(
    source: &mut (dyn CatalogSource + Send),
    query: &str,
    filter: SearchFilter,
) -> Option<SearchOutcome> {
    match source.search(query, filter) {
        Ok(SearchOutcome::Songs(songs)) if filter == SearchFilter::Songs => {
            Some(SearchOutcome::Songs(songs))
        }
        Ok(SearchOutcome::Artists(artists)) if filter == SearchFilter::Artists => {
            Some(SearchOutcome::Artists(artists))
        }
        Ok(SearchOutcome::Albums(albums)) if filter == SearchFilter::Albums => {
            Some(SearchOutcome::Albums(albums))
        }
        _ => None,
    }
}

fn lock(home: &HomeSlot) -> MutexGuard<'_, Option<HomeFeed>> {
    match home.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn lock_search(search: &SearchSlot) -> MutexGuard<'_, SearchAggregate> {
    match search.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn clear_search(shell: &MainShell) {
    apply_search_results(shell, Vec::new());
    apply_suggestions(shell, "", &[]);
}

pub fn apply_catalog_event(
    shell: &MainShell,
    event: CatalogEvent,
    generations: &Generations,
    home: &HomeSlot,
    search: &SearchSlot,
    show_chips: bool,
) {
    match event {
        CatalogEvent::Home { request_id, feed } => {
            if !generations.is_current(CatalogSlot::Home, request_id) {
                return;
            }
            apply_home(shell, &feed);
            shell.set_home_chips(ModelRc::new(VecModel::from_slice(&home_chips(
                &feed,
                show_chips,
                madrigal_ui::shell_lang(shell),
            ))));
            *lock(home) = Some(feed);
        }
        CatalogEvent::Suggestions {
            request_id,
            query,
            suggestions,
        } => {
            if generations.is_current(CatalogSlot::Suggestions, request_id) {
                apply_suggestions(shell, &query, &suggestions);
            }
        }
        CatalogEvent::Search {
            request_id,
            outcome,
        } => {
            if !generations.is_current(CatalogSlot::Search, request_id) {
                return;
            }
            match outcome {
                SearchOutcome::Songs(tracks) => apply_search_results(shell, song_results(&tracks)),
                SearchOutcome::Artists(artists) => {
                    let lang = madrigal_ui::lang_from_code(
                        shell.global::<madrigal_ui::I18n>().get_lang().as_str(),
                    );
                    apply_search_results(shell, artist_results(&artists, lang));
                }
                SearchOutcome::Albums(albums) => {
                    apply_search_results(shell, album_results(&albums));
                }
            }
        }
        CatalogEvent::SearchKind {
            request_id,
            outcome,
        } => {
            if !generations.is_current(CatalogSlot::Search, request_id) {
                return;
            }
            let items = {
                let mut aggregate = lock_search(search);
                aggregate.apply(request_id, outcome);
                aggregate.items()
            };
            apply_search_partial(shell, items);
        }
        CatalogEvent::SearchDone { request_id } => {
            if !generations.is_current(CatalogSlot::Search, request_id) {
                return;
            }
            let (empty, failed) = {
                let aggregate = lock_search(search);
                (aggregate.is_empty(), aggregate.failed())
            };
            if empty && failed {
                apply_search_results(shell, Vec::new());
                shell.set_search_error(true);
            } else {
                shell.set_search_done(true);
            }
        }
        CatalogEvent::Explore { request_id, pages } => {
            if !generations.is_current(CatalogSlot::Explore, request_id) {
                return;
            }
            apply_moods(shell, &pages.moods);
            apply_releases(shell, &pages.releases);
            apply_browse(shell, &pages.browse);
            apply_charts_entry(shell, &pages.charts);
        }
        CatalogEvent::Detail {
            request_id,
            page,
            title,
        } => {
            if !generations.is_current(CatalogSlot::Detail, request_id) {
                return;
            }
            match page {
                DetailPage::Album(page) => apply_album_detail(shell, &page),
                DetailPage::Artist(page) => apply_artist_detail(shell, &page),
                DetailPage::Playlist(page) => apply_playlist_detail(shell, &page),
                DetailPage::Browse(page) => apply_browse_detail(shell, &page, &title),
            }
        }
        CatalogEvent::Failed {
            slot,
            request_id,
            message,
        } => {
            if !generations.is_current(slot, request_id) {
                return;
            }
            eprintln!("catalog {slot:?}: {message}");
            match slot {
                CatalogSlot::Search => {
                    apply_search_results(shell, Vec::new());
                    shell.set_search_error(true);
                }
                CatalogSlot::Suggestions => apply_suggestions(shell, "", &[]),
                CatalogSlot::Detail => apply_detail_error(shell, &message),
                CatalogSlot::Home | CatalogSlot::Explore => {}
            }
        }
    }
}

#[derive(Clone)]
struct Link {
    generations: Arc<Generations>,
    home: Arc<HomeSlot>,
    requests: Sender<Request>,
}

impl Link {
    fn send(&self, request: Request) {
        let _ = self.requests.send(request);
    }

    fn query(&self, shell: &MainShell, query: &str) {
        let query = query.trim();
        if query.is_empty() || shell.get_search_source() == SOURCE_LOCAL {
            self.invalidate_search();
            clear_search(shell);
            return;
        }
        let id = self.generations.begin(CatalogSlot::Suggestions);
        self.send(Request::Suggestions {
            id,
            query: query.to_owned(),
        });
        self.filtered_search(shell, shell.get_search_filter_tab(), query);
    }

    fn filtered_search(&self, shell: &MainShell, tab: i32, query: &str) {
        shell.set_search_error(false);
        let id = self.generations.begin(CatalogSlot::Search);
        let online = !query.is_empty() && shell.get_search_source() == SOURCE_ONLINE;
        if tab == 0 && online {
            shell.set_search_done(false);
            self.send(Request::SearchMixed {
                id,
                query: query.to_owned(),
            });
            return;
        }
        match filter_for_tab(tab) {
            Some(filter) if online => {
                shell.set_search_done(false);
                self.send(Request::Search {
                    id,
                    query: query.to_owned(),
                    filter,
                });
            }
            _ => apply_search_results(shell, Vec::new()),
        }
    }

    fn invalidate_search(&self) {
        self.generations.begin(CatalogSlot::Search);
        self.generations.begin(CatalogSlot::Suggestions);
    }

    fn open_detail(&self, shell: &MainShell, reference: DetailRef) {
        let id = self.generations.begin(CatalogSlot::Detail);
        begin_detail_load(shell, &reference);
        self.send(Request::Detail { id, reference });
    }

    fn retry_detail(&self, shell: &MainShell) {
        let reference = shell.get_detail_ref();
        self.open_detail(shell, reference);
    }
}

fn filter_for_tab(tab: i32) -> Option<SearchFilter> {
    match tab {
        0 | 1 => Some(SearchFilter::Songs),
        2 => Some(SearchFilter::Artists),
        3 => Some(SearchFilter::Albums),
        _ => None,
    }
}

pub fn playable_track(item: &CatalogItem) -> Option<Track> {
    if !item.playable || item.kind == DetailKind::Podcast || item.id.is_empty() {
        return None;
    }
    let non_empty = |text: &str| (!text.is_empty()).then(|| text.to_owned());
    Some(Track {
        identity: TrackIdentity::youtube(item.id.as_str()),
        title: item.title.to_string(),
        artists: item
            .artist
            .split(", ")
            .map(str::trim)
            .filter(|artist| !artist.is_empty())
            .map(str::to_owned)
            .collect(),
        album: non_empty(item.album.as_str()),
        duration_secs: u32::try_from(item.duration_secs)
            .ok()
            .filter(|secs| *secs > 0),
        thumbnail: non_empty(item.thumbnail.as_str()),
        track_number: None,
        local_path: None,
        set_video_id: None,
    })
}

pub fn wire_play_requests(shell: &MainShell, core: &Arc<Mutex<Playback>>) {
    shell.on_play_requested({
        let core = Arc::clone(core);
        move |item| {
            let Some(track) = playable_track(&item) else {
                return;
            };
            let mut playback = match core.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if let Err(message) = playback.enqueue_track(&track) {
                playback.report(message);
            }
        }
    });
}

pub struct Wiring {
    generations: Arc<Generations>,
    home: Arc<HomeSlot>,
    search: Arc<SearchSlot>,
    requests: Sender<Request>,
}

impl Wiring {
    pub fn generations(&self) -> Arc<Generations> {
        Arc::clone(&self.generations)
    }

    pub fn home(&self) -> Arc<HomeSlot> {
        Arc::clone(&self.home)
    }

    pub fn search(&self) -> Arc<SearchSlot> {
        Arc::clone(&self.search)
    }

    pub fn set_session(&self, cookie: Option<String>) {
        let _ = self.requests.send(Request::SetSession { cookie });
    }
}

pub fn wire_catalog(
    shell: &MainShell,
    settings: &Settings,
    ui_language: &str,
) -> Result<Wiring, String> {
    let content = &settings.content;
    let generations = Arc::new(Generations::default());
    let home: Arc<HomeSlot> = Arc::new(Mutex::new(None));
    let search: Arc<SearchSlot> = Arc::new(Mutex::new(SearchAggregate::default()));
    let show_chips = settings.content.show_home_category_chips;
    let artwork = Arc::new(ArtworkCache::new(
        ArtworkCache::default_dir(),
        (settings.storage.max_image_cache_mb.max(0) as usize) * 1024 * 1024,
    ));
    let weak = shell.as_weak();
    let delivery: Delivery = {
        let generations = Arc::clone(&generations);
        let home = Arc::clone(&home);
        let search = Arc::clone(&search);
        let artwork = Arc::clone(&artwork);
        Arc::new(move |event| {
            let cover = detail_cover(&event);
            let weak = weak.clone();
            let generations = Arc::clone(&generations);
            let home = Arc::clone(&home);
            let search = Arc::clone(&search);
            let artwork = Arc::clone(&artwork);
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(shell) = weak.upgrade() {
                    apply_catalog_event(&shell, event, &generations, &home, &search, show_chips);
                    if let Some((request_id, url)) = cover {
                        if generations.is_current(CatalogSlot::Detail, request_id) {
                            request_detail_cover(&shell, &artwork, request_id, url, &generations);
                        }
                    }
                }
            });
        })
    };
    let system_country = madrigal_settings::system_country();
    let locale = Locale::new(
        content.resolved_language(ui_language),
        content.resolved_country(system_country.as_deref()),
    );
    let source = LiveSource::new(locale)?;
    wire_shared(
        shell,
        &settings.content,
        Box::new(source),
        delivery,
        generations,
        home,
        search,
    )
}

fn detail_cover(event: &CatalogEvent) -> Option<(u64, String)> {
    match event {
        CatalogEvent::Detail {
            request_id,
            page,
            ..
        } => page.cover().map(|url| (*request_id, url.to_owned())),
        _ => None,
    }
}

fn request_detail_cover(
    shell: &MainShell,
    cache: &Arc<ArtworkCache>,
    request_id: u64,
    url: String,
    generations: &Arc<Generations>,
) {
    let weak = shell.as_weak();
    let cache = Arc::clone(cache);
    let generations = Arc::clone(generations);
    let _ = thread::Builder::new()
        .name("madrigal-detail-art".to_owned())
        .spawn(move || {
            let Ok(image) = cache.get(&url, ArtworkSource::Url(url.clone()), 384) else {
                return;
            };
            if !generations.is_current(CatalogSlot::Detail, request_id) {
                return;
            }
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(shell) = weak.upgrade() {
                    if generations.is_current(CatalogSlot::Detail, request_id) {
                        shell.set_detail_cover(madrigal_ui::to_slint_image(&image));
                    }
                }
            });
        });
}

pub fn wire_catalog_with(
    shell: &MainShell,
    content: &Content,
    source: Box<dyn CatalogSource + Send>,
    delivery: Delivery,
) -> Result<Wiring, String> {
    wire_shared(
        shell,
        content,
        source,
        delivery,
        Arc::new(Generations::default()),
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new(SearchAggregate::default())),
    )
}

fn wire_shared(
    shell: &MainShell,
    content: &Content,
    source: Box<dyn CatalogSource + Send>,
    delivery: Delivery,
    generations: Arc<Generations>,
    home: Arc<HomeSlot>,
    search: Arc<SearchSlot>,
) -> Result<Wiring, String> {
    let (requests, inbox) = mpsc::channel();
    let worker_generations = Arc::clone(&generations);
    thread::Builder::new()
        .name("madrigal-catalog".to_owned())
        .spawn(move || run_worker(source, inbox, worker_generations, delivery))
        .map_err(|err| err.to_string())?;

    let link = Link {
        generations: Arc::clone(&generations),
        home: Arc::clone(&home),
        requests: requests.clone(),
    };
    shell.set_search_source(match content.search_source {
        SearchSource::Local => SOURCE_LOCAL,
        SearchSource::Online => SOURCE_ONLINE,
    });
    register_callbacks(shell, &link);

    let id = generations.begin(CatalogSlot::Home);
    link.send(Request::Home { id });
    let id = generations.begin(CatalogSlot::Explore);
    let system_country = madrigal_settings::system_country();
    link.send(Request::Explore {
        id,
        country: content.resolved_country(system_country.as_deref()),
    });

    Ok(Wiring {
        generations,
        home,
        search,
        requests,
    })
}

fn register_callbacks(shell: &MainShell, link: &Link) {
    shell.on_chip_selected({
        let weak = shell.as_weak();
        let home = Arc::clone(&link.home);
        move |chip| {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            match lock(&home).as_ref() {
                Some(feed) => apply_home_chip(&shell, chip, feed),
                None => shell.set_home_current_chip(chip),
            }
        }
    });

    shell.on_query_changed({
        let weak = shell.as_weak();
        let link = link.clone();
        move |text| {
            if let Some(shell) = weak.upgrade() {
                link.query(&shell, &text);
            }
        }
    });

    shell.on_suggestion_accepted({
        let weak = shell.as_weak();
        let link = link.clone();
        move |text| {
            if let Some(shell) = weak.upgrade() {
                shell.set_search_query(text.clone());
                link.query(&shell, &text);
            }
        }
    });

    shell.on_filter_selected({
        let weak = shell.as_weak();
        let link = link.clone();
        move |tab| {
            if let Some(shell) = weak.upgrade() {
                shell.set_search_filter_tab(tab);
                let query = shell.get_search_query();
                link.filtered_search(&shell, tab, query.trim());
            }
        }
    });

    shell.on_search_retry({
        let weak = shell.as_weak();
        let link = link.clone();
        move || {
            if let Some(shell) = weak.upgrade() {
                let query = shell.get_search_query();
                link.query(&shell, &query);
            }
        }
    });

    shell.on_source_selected({
        let weak = shell.as_weak();
        let link = link.clone();
        move |source| {
            if let Some(shell) = weak.upgrade() {
                shell.set_search_source(source);
                if source == SOURCE_LOCAL {
                    link.invalidate_search();
                    clear_search(&shell);
                } else {
                    let query = shell.get_search_query();
                    link.query(&shell, &query);
                }
            }
        }
    });

    shell.on_detail_opened({
        let weak = shell.as_weak();
        let link = link.clone();
        move |reference| {
            if let Some(shell) = weak.upgrade() {
                link.open_detail(&shell, reference);
            }
        }
    });

    shell.on_detail_retried({
        let weak = shell.as_weak();
        let link = link.clone();
        move || {
            if let Some(shell) = weak.upgrade() {
                link.retry_detail(&shell);
            }
        }
    });

    shell.on_detail_closed({
        let link = link.clone();
        move || {
            link.generations.begin(CatalogSlot::Detail);
        }
    });
}

struct LiveSource {
    runtime: tokio::runtime::Runtime,
    locale: Locale,
    client: Option<CatalogClient>,
    auth: Option<AuthCatalogClient>,
}

impl LiveSource {
    fn new(locale: Locale) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| err.to_string())?;
        Ok(Self {
            runtime,
            locale,
            client: None,
            auth: None,
        })
    }

    fn parts(&mut self) -> Result<(&tokio::runtime::Runtime, &CatalogClient), InnertubeError> {
        if self.client.is_none() {
            let locale = self.locale.clone();
            self.client = Some(self.runtime.block_on(CatalogClient::with_locale(locale))?);
        }
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| InnertubeError::Fixture("catalog client missing".to_owned()))?;
        Ok((&self.runtime, client))
    }

    fn anon_home(&mut self) -> Result<HomeFeed, InnertubeError> {
        let (runtime, client) = self.parts()?;
        runtime.block_on(client.get_home())
    }
}

async fn fetch_explore(
    client: &CatalogClient,
    country: &str,
) -> Result<ExplorePages, InnertubeError> {
    Ok(ExplorePages {
        moods: client.mood_listing().await?,
        releases: client.new_releases().await?,
        browse: client.browse(EXPLORE_BROWSE_ID, None).await?,
        charts: client.charts(country).await?,
    })
}

async fn fetch_explore_auth(
    client: &AuthCatalogClient,
    country: &str,
) -> Result<ExplorePages, InnertubeError> {
    Ok(ExplorePages {
        moods: client.mood_listing().await?,
        releases: client.new_releases().await?,
        browse: client.browse(EXPLORE_BROWSE_ID, None).await?,
        charts: client.charts(country).await?,
    })
}

impl LiveSource {
    fn album_detail(&mut self, id: &str) -> Result<AlbumPage, InnertubeError> {
        if let Some(auth) = &self.auth {
            if let Ok(page) = self.runtime.block_on(auth.album_page(id)) {
                return Ok(page);
            }
        }
        let (runtime, client) = self.parts()?;
        runtime.block_on(client.album_page(id))
    }

    fn artist_detail(&mut self, id: &str) -> Result<ArtistPage, InnertubeError> {
        if let Some(auth) = &self.auth {
            if let Ok(page) = self.runtime.block_on(auth.artist_page(id)) {
                return Ok(page);
            }
        }
        let (runtime, client) = self.parts()?;
        runtime.block_on(client.artist_page(id))
    }

    fn playlist_detail(&mut self, id: &str) -> Result<PlaylistPage, InnertubeError> {
        if let Some(auth) = &self.auth {
            if let Ok(page) = self.runtime.block_on(auth.playlist_page(id)) {
                return Ok(page);
            }
        }
        let (runtime, client) = self.parts()?;
        runtime.block_on(client.playlist_page(id))
    }

    fn browse_detail(&mut self, id: &str) -> Result<BrowsePage, InnertubeError> {
        if let Some(country) = id.strip_prefix("charts:") {
            if let Some(auth) = &self.auth {
                if let Ok(page) = self.runtime.block_on(auth.charts_page(country)) {
                    return Ok(page);
                }
            }
            let (runtime, client) = self.parts()?;
            return runtime.block_on(client.charts_page(country));
        }
        if let Some(auth) = &self.auth {
            if let Ok(page) = self
                .runtime
                .block_on(auth.browse(MOODS_BROWSE_ID, Some(id)))
            {
                return Ok(page);
            }
        }
        let (runtime, client) = self.parts()?;
        runtime.block_on(client.browse(MOODS_BROWSE_ID, Some(id)))
    }
}

impl CatalogSource for LiveSource {
    fn set_session(&mut self, cookie: Option<String>) {
        self.auth = cookie.as_deref().and_then(|cookie| {
            CatalogClient::with_cookies_locale(cookie, self.locale.clone()).ok()
        });
    }

    fn home(&mut self) -> Result<HomeFeed, InnertubeError> {
        if let Some(auth) = &self.auth {
            let result = self.runtime.block_on(auth.get_home());
            if result.is_ok() {
                return result;
            }
        }
        self.anon_home()
    }

    fn suggestions(&mut self, query: &str) -> Result<Vec<String>, InnertubeError> {
        if let Some(auth) = &self.auth {
            let result = self.runtime.block_on(auth.suggestions(query));
            if result.is_ok() {
                return result;
            }
        }
        let (runtime, client) = self.parts()?;
        runtime.block_on(client.suggestions(query))
    }

    fn search(
        &mut self,
        query: &str,
        filter: SearchFilter,
    ) -> Result<SearchOutcome, InnertubeError> {
        if let Some(auth) = &self.auth {
            let result = self.runtime.block_on(auth.search(query, filter));
            if result.is_ok() {
                return result;
            }
        }
        let (runtime, client) = self.parts()?;
        runtime.block_on(client.search(query, filter))
    }

    fn explore(&mut self, country: &str) -> Result<ExplorePages, InnertubeError> {
        if let Some(auth) = &self.auth {
            let result = self.runtime.block_on(fetch_explore_auth(auth, country));
            if result.is_ok() {
                return result;
            }
        }
        let (runtime, client) = self.parts()?;
        runtime.block_on(fetch_explore(client, country))
    }

    fn detail(&mut self, kind: DetailKind, id: &str) -> Result<DetailPage, InnertubeError> {
        match kind {
            DetailKind::Album => self.album_detail(id).map(DetailPage::Album),
            DetailKind::Artist => self.artist_detail(id).map(DetailPage::Artist),
            DetailKind::Playlist => self.playlist_detail(id).map(DetailPage::Playlist),
            DetailKind::Browse => self.browse_detail(id).map(DetailPage::Browse),
            DetailKind::Podcast => Err(InnertubeError::Fixture(
                "podcast pages are not supported".to_owned(),
            )),
        }
    }
}
