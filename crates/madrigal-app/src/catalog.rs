use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

use madrigal_core::{Track, TrackIdentity};
use madrigal_innertube::{
    AuthCatalogClient, BrowsePage, CatalogClient, ChartsPage, Error as InnertubeError, HomeFeed,
    MoodListing, NewReleases, SearchFilter, SearchOutcome,
};
use madrigal_settings::{Content, SearchSource};
use madrigal_ui::{
    album_results, apply_browse, apply_charts_entry, apply_home, apply_home_chip, apply_moods,
    apply_releases, apply_search_results, apply_suggestions, artist_results, home_chips,
    song_results, CatalogItem, DetailKind, MainShell,
};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::playback::Playback;

const EXPLORE_BROWSE_ID: &str = "FEmusic_explore";
const SOURCE_LOCAL: i32 = 0;
const SOURCE_ONLINE: i32 = 1;

pub type HomeSlot = Mutex<Option<HomeFeed>>;
pub type Delivery = Arc<dyn Fn(CatalogEvent) + Send + Sync>;

pub trait CatalogSource {
    fn home(&mut self) -> Result<HomeFeed, InnertubeError>;
    fn suggestions(&mut self, query: &str) -> Result<Vec<String>, InnertubeError>;
    fn search(
        &mut self,
        query: &str,
        filter: SearchFilter,
    ) -> Result<SearchOutcome, InnertubeError>;
    fn explore(&mut self, country: &str) -> Result<ExplorePages, InnertubeError>;
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
}

#[derive(Default)]
pub struct Generations {
    home: AtomicU64,
    suggestions: AtomicU64,
    search: AtomicU64,
    explore: AtomicU64,
}

impl Generations {
    fn counter(&self, slot: CatalogSlot) -> &AtomicU64 {
        match slot {
            CatalogSlot::Home => &self.home,
            CatalogSlot::Suggestions => &self.suggestions,
            CatalogSlot::Search => &self.search,
            CatalogSlot::Explore => &self.explore,
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
    Explore {
        request_id: u64,
        pages: ExplorePages,
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
    Explore {
        id: u64,
        country: String,
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
            Request::Search { id, .. } => Some((CatalogSlot::Search, *id)),
            Request::Explore { id, .. } => Some((CatalogSlot::Explore, *id)),
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
    for request in requests {
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
            Request::Explore { id, country } => settle(
                slot,
                id,
                source.explore(&country).map(|pages| CatalogEvent::Explore {
                    request_id: id,
                    pages,
                }),
            ),
            Request::SetSession { .. } => continue,
        };
        delivery(event);
    }
}

fn lock(home: &HomeSlot) -> MutexGuard<'_, Option<HomeFeed>> {
    match home.lock() {
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
    show_chips: bool,
) {
    match event {
        CatalogEvent::Home { request_id, feed } => {
            if !generations.is_current(CatalogSlot::Home, request_id) {
                return;
            }
            apply_home(shell, &feed);
            shell.set_home_chips(ModelRc::new(VecModel::from_slice(&home_chips(
                &feed, show_chips,
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
                    apply_search_results(shell, artist_results(&artists));
                }
                SearchOutcome::Albums(albums) => {
                    apply_search_results(shell, album_results(&albums));
                }
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
                CatalogSlot::Search | CatalogSlot::Suggestions => clear_search(shell),
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
        let id = self.generations.begin(CatalogSlot::Search);
        match filter_for_tab(tab) {
            Some(filter) if !query.is_empty() && shell.get_search_source() == SOURCE_ONLINE => {
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
    requests: Sender<Request>,
}

impl Wiring {
    pub fn generations(&self) -> Arc<Generations> {
        Arc::clone(&self.generations)
    }

    pub fn home(&self) -> Arc<HomeSlot> {
        Arc::clone(&self.home)
    }

    pub fn set_session(&self, cookie: Option<String>) {
        let _ = self.requests.send(Request::SetSession { cookie });
    }
}

pub fn wire_catalog(shell: &MainShell, content: &Content) -> Result<Wiring, String> {
    let generations = Arc::new(Generations::default());
    let home: Arc<HomeSlot> = Arc::new(Mutex::new(None));
    let show_chips = content.show_home_category_chips;
    let weak = shell.as_weak();
    let delivery: Delivery = {
        let generations = Arc::clone(&generations);
        let home = Arc::clone(&home);
        Arc::new(move |event| {
            let weak = weak.clone();
            let generations = Arc::clone(&generations);
            let home = Arc::clone(&home);
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(shell) = weak.upgrade() {
                    apply_catalog_event(&shell, event, &generations, &home, show_chips);
                }
            });
        })
    };
    let source = LiveSource::new()?;
    wire_shared(
        shell,
        content,
        Box::new(source),
        delivery,
        generations,
        home,
    )
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
    )
}

fn wire_shared(
    shell: &MainShell,
    content: &Content,
    source: Box<dyn CatalogSource + Send>,
    delivery: Delivery,
    generations: Arc<Generations>,
    home: Arc<HomeSlot>,
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
    link.send(Request::Explore {
        id,
        country: content.content_country.clone(),
    });

    Ok(Wiring {
        generations,
        home,
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
}

struct LiveSource {
    runtime: tokio::runtime::Runtime,
    client: Option<CatalogClient>,
    auth: Option<AuthCatalogClient>,
}

impl LiveSource {
    fn new() -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| err.to_string())?;
        Ok(Self {
            runtime,
            client: None,
            auth: None,
        })
    }

    fn parts(&mut self) -> Result<(&tokio::runtime::Runtime, &CatalogClient), InnertubeError> {
        if self.client.is_none() {
            self.client = Some(self.runtime.block_on(CatalogClient::new())?);
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

impl CatalogSource for LiveSource {
    fn set_session(&mut self, cookie: Option<String>) {
        self.auth = cookie
            .as_deref()
            .and_then(|cookie| CatalogClient::with_cookies(cookie).ok());
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
}
