use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use crossbeam_channel::{unbounded, Receiver, Sender};
use image::RgbaImage;
use madrigal_artwork::{bucket_for, sized_url, ArtworkCache, Source};
use slint::{ComponentHandle, Model, ModelRc};

use crate::player_ui::QueueTabData;
use crate::{CatalogItem, CatalogSection, DownloadRow, MainShell, QueueRow};

type CacheKey = (String, u32);
type Done = Box<dyn FnOnce(RgbaImage) + Send + 'static>;

struct Job {
    key: CacheKey,
    url: String,
}

struct ArtworkService {
    tx: Sender<Job>,
    pending: Arc<Mutex<HashMap<CacheKey, Vec<Done>>>>,
}

thread_local! {
    static SERVICE: RefCell<Option<ArtworkService>> = const { RefCell::new(None) };
    static NOW_PLAYING: RefCell<String> = const { RefCell::new(String::new()) };
}

pub fn init_artwork(root: PathBuf, disk_budget: usize) {
    SERVICE.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }
        let cache = Arc::new(ArtworkCache::new(root, disk_budget));
        let pending: Arc<Mutex<HashMap<CacheKey, Vec<Done>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = unbounded::<Job>();
        for index in 0..2 {
            let cache = Arc::clone(&cache);
            let pending = Arc::clone(&pending);
            let rx = rx.clone();
            let _ = thread::Builder::new()
                .name(format!("madrigal-artwork-{index}"))
                .spawn(move || worker(cache, pending, rx));
        }
        *slot.borrow_mut() = Some(ArtworkService { tx, pending });
    });
}

fn worker(
    cache: Arc<ArtworkCache>,
    pending: Arc<Mutex<HashMap<CacheKey, Vec<Done>>>>,
    rx: Receiver<Job>,
) {
    for job in rx {
        let bucket = job.key.1;
        let image = cache.get(&job.key.0, Source::Url(job.url), bucket);
        let callbacks = pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&job.key));
        if let (Ok(image), Some(callbacks)) = (image, callbacks) {
            for done in callbacks {
                done(image.clone());
            }
        }
    }
}

fn enabled() -> bool {
    SERVICE.with(|slot| slot.borrow().is_some())
}

fn request(url: &str, bucket: u32, done: Done) {
    let load = sized_url(url, bucket);
    let key = (load.clone(), bucket);
    SERVICE.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(service) = borrowed.as_mut() else {
            return;
        };
        let mut pending = match service.pending.lock() {
            Ok(pending) => pending,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(callbacks) = pending.get_mut(&key) {
            callbacks.push(done);
            return;
        }
        pending.insert(key.clone(), vec![done]);
        drop(pending);
        let _ = service.tx.send(Job { key, url: load });
    });
}

fn scale(shell: &MainShell) -> f32 {
    shell.window().scale_factor()
}

fn patch_catalog(
    shell: &MainShell,
    get: fn(&MainShell) -> ModelRc<CatalogItem>,
    index: usize,
    thumbnail: String,
    image: RgbaImage,
) {
    let model = get(shell);
    let Some(mut item) = model.row_data(index) else {
        return;
    };
    if item.thumbnail.as_str() != thumbnail {
        return;
    }
    item.cover = to_slint_image(&image);
    model.set_row_data(index, item);
}

fn request_catalog_item(
    shell: &MainShell,
    get: fn(&MainShell) -> ModelRc<CatalogItem>,
    index: usize,
    item: &CatalogItem,
    bucket: u32,
) {
    let thumbnail = item.thumbnail.to_string();
    if thumbnail.is_empty() || item.cover.size().width > 0 {
        return;
    }
    let load = thumbnail.clone();
    let weak = shell.as_weak();
    request(
        &load,
        bucket,
        Box::new(move |image| {
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(shell) = weak.upgrade() {
                    patch_catalog(&shell, get, index, thumbnail, image);
                }
            });
        }),
    );
}

fn request_section_item(
    shell: &MainShell,
    get: fn(&MainShell) -> ModelRc<CatalogSection>,
    section_index: usize,
    item_index: usize,
    item: &CatalogItem,
    bucket: u32,
) {
    let thumbnail = item.thumbnail.to_string();
    if thumbnail.is_empty() || item.cover.size().width > 0 {
        return;
    }
    let load = thumbnail.clone();
    let weak = shell.as_weak();
    request(
        &load,
        bucket,
        Box::new(move |image| {
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(shell) = weak.upgrade() else {
                    return;
                };
                let sections = get(&shell);
                let Some(section) = sections.row_data(section_index) else {
                    return;
                };
                let Some(mut item) = section.items.row_data(item_index) else {
                    return;
                };
                if item.thumbnail.as_str() != thumbnail {
                    return;
                }
                item.cover = to_slint_image(&image);
                section.items.set_row_data(item_index, item);
                sections.set_row_data(section_index, section);
            });
        }),
    );
}

pub fn resolve_catalog(
    shell: &MainShell,
    get: fn(&MainShell) -> ModelRc<CatalogItem>,
    rows_bucket: u32,
    cards_bucket: u32,
) {
    if !enabled() {
        return;
    }
    let model = get(shell);
    for index in 0..model.row_count() {
        let Some(item) = model.row_data(index) else {
            continue;
        };
        let bucket = if item.playable {
            rows_bucket
        } else {
            cards_bucket
        };
        request_catalog_item(shell, get, index, &item, bucket);
    }
}

pub fn resolve_catalog_single(
    shell: &MainShell,
    get: fn(&MainShell) -> ModelRc<CatalogItem>,
    bucket: u32,
) {
    if !enabled() {
        return;
    }
    let model = get(shell);
    for index in 0..model.row_count() {
        if let Some(item) = model.row_data(index) {
            request_catalog_item(shell, get, index, &item, bucket);
        }
    }
}

pub fn resolve_sections(
    shell: &MainShell,
    get: fn(&MainShell) -> ModelRc<CatalogSection>,
    rows_bucket: u32,
    cards_bucket: u32,
) {
    if !enabled() {
        return;
    }
    let sections = get(shell);
    for section_index in 0..sections.row_count() {
        let Some(section) = sections.row_data(section_index) else {
            continue;
        };
        for item_index in 0..section.items.row_count() {
            let Some(item) = section.items.row_data(item_index) else {
                continue;
            };
            let bucket = if item.playable {
                rows_bucket
            } else {
                cards_bucket
            };
            request_section_item(shell, get, section_index, item_index, &item, bucket);
        }
    }
}

pub fn resolve_home(shell: &MainShell) {
    let scale = scale(shell);
    let rows = bucket_for(36.0, scale);
    let cards = bucket_for(118.0, scale);
    resolve_sections(shell, MainShell::get_home_sections, rows, cards);
    let quick_picks = if shell.get_home_card_layout() {
        bucket_for(118.0, scale)
    } else {
        rows
    };
    resolve_catalog_single(shell, MainShell::get_home_quick_picks, quick_picks);
}

pub fn resolve_search(shell: &MainShell) {
    let scale = scale(shell);
    resolve_catalog(
        shell,
        MainShell::get_search_results,
        bucket_for(36.0, scale),
        bucket_for(118.0, scale),
    );
}

pub fn resolve_detail(shell: &MainShell) {
    let scale = scale(shell);
    resolve_catalog_single(
        shell,
        MainShell::get_detail_tracks,
        bucket_for(36.0, scale),
    );
    resolve_sections(
        shell,
        MainShell::get_detail_sections,
        bucket_for(36.0, scale),
        bucket_for(118.0, scale),
    );
}

pub fn resolve_releases(shell: &MainShell) {
    let cards = bucket_for(118.0, scale(shell));
    resolve_catalog_single(shell, MainShell::get_explore_releases_albums, cards);
    resolve_catalog_single(shell, MainShell::get_explore_releases_videos, cards);
}

pub fn resolve_browse(shell: &MainShell) {
    let scale = scale(shell);
    resolve_sections(
        shell,
        MainShell::get_explore_browse_sections,
        bucket_for(36.0, scale),
        bucket_for(36.0, scale),
    );
}

fn patch_queue(
    shell: &MainShell,
    get: fn(&MainShell) -> ModelRc<QueueRow>,
    index: usize,
    thumbnail: String,
    image: RgbaImage,
) {
    let model = get(shell);
    let Some(mut row) = model.row_data(index) else {
        return;
    };
    if row.thumbnail.as_str() != thumbnail {
        return;
    }
    row.cover = to_slint_image(&image);
    model.set_row_data(index, row);
}

fn resolve_queue_model(
    shell: &MainShell,
    get: fn(&MainShell) -> ModelRc<QueueRow>,
    rows: &[crate::QueueRowData],
    bucket: u32,
) {
    for (index, row) in rows.iter().enumerate() {
        if row.thumbnail.is_empty() {
            continue;
        }
        let thumbnail = row.thumbnail.clone();
        let load = thumbnail.clone();
        let weak = shell.as_weak();
        request(
            &load,
            bucket,
            Box::new(move |image| {
                let weak = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(shell) = weak.upgrade() {
                        patch_queue(&shell, get, index, thumbnail, image);
                    }
                });
            }),
        );
    }
}

pub fn resolve_queue(shell: &MainShell, tabs: &QueueTabData) {
    if !enabled() {
        return;
    }
    let bucket = bucket_for(36.0, scale(shell));
    resolve_queue_model(shell, MainShell::get_queue_all, &tabs.all, bucket);
    resolve_queue_model(shell, MainShell::get_queue_searched, &tabs.searched, bucket);
    resolve_queue_model(shell, MainShell::get_queue_continue, &tabs.cont, bucket);
    resolve_queue_model(shell, MainShell::get_queue_auto, &tabs.auto, bucket);
}

fn patch_download(
    shell: &MainShell,
    get: fn(&MainShell) -> ModelRc<DownloadRow>,
    index: usize,
    thumbnail: String,
    image: RgbaImage,
) {
    let model = get(shell);
    let Some(mut row) = model.row_data(index) else {
        return;
    };
    if row.thumbnail.as_str() != thumbnail {
        return;
    }
    row.cover = to_slint_image(&image);
    model.set_row_data(index, row);
}

fn resolve_download_model(shell: &MainShell, get: fn(&MainShell) -> ModelRc<DownloadRow>) {
    let bucket = bucket_for(36.0, scale(shell));
    let model = get(shell);
    for index in 0..model.row_count() {
        let Some(row) = model.row_data(index) else {
            continue;
        };
        let thumbnail = row.thumbnail.to_string();
        if thumbnail.is_empty() || row.cover.size().width > 0 {
            continue;
        }
        let weak = shell.as_weak();
        let load = thumbnail.clone();
        request(
            &load,
            bucket,
            Box::new(move |image| {
                let weak = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(shell) = weak.upgrade() {
                        patch_download(&shell, get, index, thumbnail, image);
                    }
                });
            }),
        );
    }
}

pub fn resolve_downloads(shell: &MainShell) {
    if !enabled() {
        return;
    }
    resolve_download_model(shell, MainShell::get_dl_rows);
    resolve_download_model(shell, MainShell::get_dl_done);
    resolve_download_model(shell, MainShell::get_dl_failed);
}

pub fn set_now_playing_cover(shell: &MainShell, thumbnail: &str) {
    if !enabled() {
        return;
    }
    let bucket = bucket_for(300.0, scale(shell));
    let url = if thumbnail.is_empty() {
        String::new()
    } else {
        sized_url(thumbnail, bucket)
    };
    let changed = NOW_PLAYING.with(|slot| {
        let mut current = slot.borrow_mut();
        if *current == url {
            return false;
        }
        *current = url.clone();
        true
    });
    if !changed {
        return;
    }
    shell.set_cover(slint::Image::default());
    if url.is_empty() {
        return;
    }
    let weak = shell.as_weak();
    let load = url.clone();
    request(
        &load,
        bucket,
        Box::new(move |image| {
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if !NOW_PLAYING.with(|slot| *slot.borrow() == url) {
                    return;
                }
                if let Some(shell) = weak.upgrade() {
                    shell.set_cover(to_slint_image(&image));
                }
            });
        }),
    );
}

pub fn to_slint_image(img: &RgbaImage) -> slint::Image {
    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        img.as_raw(),
        img.width(),
        img.height(),
    );
    slint::Image::from_rgba8(buffer)
}

fn generations() -> &'static Mutex<HashMap<String, u64>> {
    static GENERATIONS: std::sync::OnceLock<Mutex<HashMap<String, u64>>> =
        std::sync::OnceLock::new();
    GENERATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_generation(slot: &str) -> u64 {
    let mut map = generations().lock().expect("generation lock poisoned");
    let generation = map.entry(slot.to_string()).or_insert(0);
    *generation = generation.wrapping_add(1);
    *generation
}

fn is_current(slot: &str, generation: u64) -> bool {
    generations()
        .lock()
        .map(|map| map.get(slot) == Some(&generation))
        .unwrap_or(false)
}

pub fn apply_cover<C>(
    component: &C,
    slot: &str,
    image: RgbaImage,
    setter: impl FnOnce(&C, slint::Image) + Send + 'static,
) where
    C: ComponentHandle + 'static,
{
    let slot = slot.to_string();
    let generation = next_generation(&slot);
    let weak = component.as_weak();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(component) = weak.upgrade() {
            if is_current(&slot, generation) {
                setter(&component, to_slint_image(&image));
            }
        }
    });
}
