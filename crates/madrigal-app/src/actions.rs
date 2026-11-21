use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use madrigal_core::{PlaylistId, Track, TrackIdentity};
use madrigal_downloads::{DownloadState, JobOptions};
use madrigal_i18n::tr;
use madrigal_library::{DuplicatePolicy, Library, PlaylistListFilter};
use madrigal_settings::Settings;
use madrigal_ui::{
    track_source_index, DetailKind, DetailRef, MainShell, TrackMenuEntry, TrackMenuRequest,
};
use rusqlite::Connection;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::downloads::Downloads;
use crate::playback::Playback;
use crate::ui::lang_from_settings;

pub const MENU_REFRESH: &str = "menu-refresh";

pub struct TrackActions {
    pub conn: Rc<RefCell<Connection>>,
    pub settings: Settings,
    pub downloads: Rc<Downloads>,
    pub core: Option<Arc<Mutex<Playback>>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PromptUse {
    None,
    Playlist,
    PlaylistWithTrack,
}

impl From<&str> for PromptUse {
    fn from(value: &str) -> Self {
        match value {
            "library" => Self::Playlist,
            "track-menu-playlist" => Self::PlaylistWithTrack,
            _ => Self::None,
        }
    }
}

pub fn track_from_request(request: &TrackMenuRequest) -> Option<Track> {
    let identity = TrackIdentity::parse_prefixed(request.id.as_str());
    if identity.id.is_empty() {
        return None;
    }
    let artists = request
        .artist
        .split(", ")
        .map(str::trim)
        .filter(|artist| !artist.is_empty())
        .map(str::to_owned)
        .collect();
    let album = request
        .album
        .trim()
        .to_owned();
    let local_path = (identity.source == madrigal_core::TrackSource::Local)
        .then(|| identity.id.clone());
    Some(Track {
        identity,
        title: request.title.to_string(),
        artists,
        album: (!album.is_empty()).then_some(album),
        duration_secs: u32::try_from(request.duration_secs)
            .ok()
            .filter(|secs| *secs > 0),
        thumbnail: None,
        track_number: None,
        local_path,
        set_video_id: None,
    })
}

pub fn playlist_entries(lib: &Library<'_>) -> Vec<TrackMenuEntry> {
    let filter = PlaylistListFilter {
        tag: Some(String::new()),
        include_hidden: false,
        kind: None,
    };
    let mut playlists = lib.list_playlists(&filter).unwrap_or_default();
    playlists.sort_by_key(|playlist| playlist.title.to_lowercase());
    playlists
        .into_iter()
        .map(|playlist| TrackMenuEntry {
            id: playlist.id.as_str().to_owned().into(),
            title: playlist.title.into(),
        })
        .collect()
}

pub fn refresh_library(shell: &MainShell) {
    shell.set_lib_action_count(shell.get_lib_action_count() + 1);
    shell.set_lib_last_action(MENU_REFRESH.into());
    shell.set_lib_last_arg("".into());
}

fn prompt_title(settings: &Settings) -> String {
    tr(
        lang_from_settings(settings.appearance.language),
        "playlists.create.name",
    )
    .to_owned()
}

fn open_prompt(shell: &MainShell, settings: &Settings, purpose: &str) {
    shell.set_prompt_purpose(purpose.into());
    shell.invoke_open_prompt(prompt_title(settings).into());
}

fn with_library<T>(
    conn: &Rc<RefCell<Connection>>,
    settings: &Settings,
    action: impl FnOnce(&Library<'_>) -> T,
) -> T {
    let conn = conn.borrow();
    let lib = Library::new(&conn, settings.clone());
    action(&lib)
}

fn set_liked(shell: &MainShell, identity: &TrackIdentity, liked: bool) {
    if shell.get_track_id().as_str() == identity.id.as_str()
        && shell.get_track_source() == track_source_index(identity.source)
    {
        shell.set_liked(liked);
    }
}

pub fn wire_track_actions(shell: &MainShell, actions: TrackActions) {
    let pending: Rc<RefCell<Option<Track>>> = Rc::new(RefCell::new(None));

    shell.on_track_menu_opened({
        let conn = Rc::clone(&actions.conn);
        let settings = actions.settings.clone();
        let downloads = Rc::clone(&actions.downloads);
        let weak = shell.as_weak();
        move |request| {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            let identity = TrackIdentity::parse_prefixed(request.id.as_str());
            if identity.id.is_empty() {
                return;
            }
            let (liked, playlists, song) = with_library(&conn, &settings, |lib| {
                (
                    lib.is_liked(&identity).unwrap_or(false),
                    playlist_entries(lib),
                    lib.song(&identity).ok().flatten(),
                )
            });
            shell.set_track_menu_liked(liked);
            shell.set_track_menu_playlists(ModelRc::new(VecModel::from_slice(&playlists)));
            let mut enriched = request;
            if let Some(song) = song {
                if enriched.artist.is_empty() {
                    enriched.artist = song.artist_text().into();
                }
                if enriched.album.is_empty() {
                    enriched.album = song.album.unwrap_or_default().into();
                }
                if enriched.title.is_empty() {
                    enriched.title = song.title.into();
                }
                if enriched.duration_secs == 0 {
                    enriched.duration_secs = song.duration_secs.unwrap_or_default() as i32;
                }
            }
            shell.set_track_menu_track(enriched);
            let (state, progress) = downloads.menu_state(&identity);
            shell.set_track_menu_download_state(state);
            shell.set_track_menu_download_progress(progress);
        }
    });

    shell.on_track_menu_action({
        let conn = Rc::clone(&actions.conn);
        let settings = actions.settings.clone();
        let downloads = Rc::clone(&actions.downloads);
        let core = actions.core.clone();
        let pending = Rc::clone(&pending);
        let weak = shell.as_weak();
        move |action, arg| {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            let request = shell.get_track_menu_track();
            let Some(track) = track_from_request(&request) else {
                return;
            };
            match action.as_str() {
                "play-next" => {
                    if let Some(core) = core.as_ref() {
                        if let Err(message) = lock(core).enqueue_track_next(&track) {
                            eprintln!("menu: play next: {message}");
                        }
                    }
                }
                "queue" => {
                    if let Some(core) = core.as_ref() {
                        if let Err(message) = lock(core).enqueue_track_end(&track) {
                            eprintln!("menu: queue: {message}");
                        }
                    }
                }
                "like" => {
                    let liked = with_library(&conn, &settings, |lib| {
                        let liked = lib.is_liked(&track.identity).unwrap_or(false);
                        if let Err(err) = lib.set_liked(&track.identity, !liked) {
                            eprintln!("menu: like: {err}");
                        }
                        liked
                    });
                    shell.set_track_menu_liked(!liked);
                    set_liked(&shell, &track.identity, !liked);
                    refresh_library(&shell);
                }
                "download" => {
                    if downloads.state(&track.identity) == Some(DownloadState::Completed) {
                        shell.set_page(4);
                    } else {
                        downloads.enqueue(track.clone(), JobOptions::default());
                        downloads.refresh(&shell);
                    }
                }
                "playlist" => {
                    let playlist = PlaylistId::new(arg.to_string());
                    with_library(&conn, &settings, |lib| {
                        if let Err(err) =
                            lib.add_tracks(&playlist, std::slice::from_ref(&track), DuplicatePolicy::Skip)
                        {
                            eprintln!("menu: add to playlist: {err}");
                        }
                    });
                    refresh_library(&shell);
                }
                "playlist-new" => {
                    *pending.borrow_mut() = Some(track);
                    open_prompt(&shell, &settings, "track-menu-playlist");
                }
                "artist" if !request.artist.is_empty() => {
                    shell.invoke_detail_requested(DetailRef {
                        kind: DetailKind::Artist,
                        id: request.artist.clone(),
                    });
                }
                "album" if !request.album.is_empty() => {
                    shell.invoke_detail_requested(DetailRef {
                        kind: DetailKind::Album,
                        id: request.album.clone(),
                    });
                }
                _ => {}
            }
        }
    });

    shell.on_lib_new_playlist_requested({
        let settings = actions.settings.clone();
        let weak = shell.as_weak();
        move || {
            if let Some(shell) = weak.upgrade() {
                open_prompt(&shell, &settings, "library");
            }
        }
    });

    shell.on_prompt_confirmed({
        let conn = Rc::clone(&actions.conn);
        let settings = actions.settings.clone();
        let pending = Rc::clone(&pending);
        let weak = shell.as_weak();
        move |text| {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            let purpose = PromptUse::from(shell.get_prompt_purpose().as_str());
            shell.set_prompt_purpose("".into());
            if purpose == PromptUse::None {
                return;
            }
            let title = text.trim();
            if title.is_empty() {
                return;
            }
            let created = with_library(&conn, &settings, |lib| {
                let playlist = match lib.create_playlist(title) {
                    Ok(playlist) => playlist,
                    Err(err) => {
                        eprintln!("menu: create playlist: {err}");
                        return false;
                    }
                };
                if purpose == PromptUse::PlaylistWithTrack {
                    if let Some(track) = pending.borrow_mut().take() {
                        if let Err(err) = lib.add_tracks(
                            &playlist.id,
                            &[track],
                            DuplicatePolicy::Skip,
                        ) {
                            eprintln!("menu: add to new playlist: {err}");
                        }
                    }
                }
                true
            });
            if created {
                refresh_library(&shell);
            }
        }
    });
}

fn lock(core: &Arc<Mutex<Playback>>) -> std::sync::MutexGuard<'_, Playback> {
    match core.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
