use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use slint::{ModelRc, VecModel};

use crate::{
    DownloadRow, HistoryRow, LibCardRow, LibPlaylistRow, LibSongRow, MainShell, StatRow,
};

pub const CHIP_LIBRARY: &str = "library";
pub const CHIP_PLAYLISTS: &str = "playlists";
pub const CHIP_SONGS: &str = "songs";
pub const CHIP_ARTISTS: &str = "artists";
pub const CHIP_ALBUMS: &str = "albums";
pub const CHIP_HISTORY: &str = "history";
pub const CHIP_STATS: &str = "stats";

const KNOWN_CHIPS: &[&str] = &[
    CHIP_LIBRARY,
    CHIP_PLAYLISTS,
    CHIP_SONGS,
    CHIP_ARTISTS,
    CHIP_ALBUMS,
    CHIP_HISTORY,
    CHIP_STATS,
];

pub fn default_chip_order() -> Vec<String> {
    KNOWN_CHIPS.iter().map(|chip| chip.to_string()).collect()
}

pub fn chip_order(settings: &madrigal_settings::Settings) -> Vec<String> {
    let mut order: Vec<String> = settings
        .content
        .library_chip_order
        .split(',')
        .map(str::trim)
        .filter(|chip| KNOWN_CHIPS.contains(chip))
        .map(str::to_owned)
        .collect();
    for chip in KNOWN_CHIPS {
        if !order.iter().any(|item| item == chip) {
            order.push(chip.to_string());
        }
    }
    order
}

pub fn save_chip_order(settings: &mut madrigal_settings::Settings, order: &[String]) {
    let mut seen = HashSet::new();
    let mut clean: Vec<String> = Vec::new();
    for chip in order {
        if KNOWN_CHIPS.contains(&chip.as_str()) && seen.insert(chip.clone()) {
            clean.push(chip.clone());
        }
    }
    for chip in KNOWN_CHIPS {
        if seen.insert(chip.to_string()) {
            clean.push(chip.to_string());
        }
    }
    settings.content.library_chip_order = clean.join(",");
}

pub fn move_chip(order: &mut Vec<String>, from: usize, to: usize) -> bool {
    if from >= order.len() || to >= order.len() || from == to {
        return false;
    }
    let chip = order.remove(from);
    order.insert(to, chip);
    true
}

pub fn song_sort_for_settings(
    settings: &madrigal_settings::Settings,
) -> (madrigal_library::SongSort, madrigal_library::SortDir) {
    let sort = match settings.content.song_sort_type {
        madrigal_settings::SongSortType::CreateDate => madrigal_library::SongSort::DateAdded,
        madrigal_settings::SongSortType::Name => madrigal_library::SongSort::Name,
        madrigal_settings::SongSortType::Artist => madrigal_library::SongSort::Artist,
        madrigal_settings::SongSortType::PlayTime => madrigal_library::SongSort::PlayTime,
    };
    let dir = if settings.content.song_sort_descending {
        madrigal_library::SortDir::Desc
    } else {
        madrigal_library::SortDir::Asc
    };
    (sort, dir)
}

pub fn song_rows(
    lib: &madrigal_library::Library<'_>,
    sort: madrigal_library::SongSort,
    dir: madrigal_library::SortDir,
    selected: &HashSet<String>,
) -> Vec<LibSongRow> {
    lib.songs(madrigal_library::SongScope::Library, sort, dir)
        .unwrap_or_default()
        .into_iter()
        .map(|track| {
            let id = track.identity.to_storage_string();
            let liked = lib.is_liked(&track.identity).unwrap_or(false);
            let subtitle = track.artist_text();
            LibSongRow {
                id: id.clone().into(),
                title: track.title.clone().into(),
                subtitle: subtitle.into(),
                liked,
                selected: selected.contains(&id),
            }
        })
        .collect()
}

pub fn playlist_rows(
    lib: &madrigal_library::Library<'_>,
    tag: Option<&str>,
) -> Vec<LibPlaylistRow> {
    let filter = madrigal_library::PlaylistListFilter {
        tag: Some(tag.unwrap_or("").to_owned()),
        include_hidden: false,
        kind: None,
    };
    let mut playlists = lib.list_playlists(&filter).unwrap_or_default();
    playlists.sort_by_key(|playlist| playlist.title.to_lowercase());
    playlists
        .into_iter()
        .map(|playlist| {
            let tags = lib.playlist_tags(&playlist.id).unwrap_or_default().join(", ");
            let count = playlist.track_count.unwrap_or(0);
            LibPlaylistRow {
                id: playlist.id.as_str().to_owned().into(),
                title: playlist.title.into(),
                subtitle: format!("{count} tracks").into(),
                tags: tags.into(),
            }
        })
        .collect()
}

pub fn artist_cards(lib: &madrigal_library::Library<'_>) -> Vec<LibCardRow> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let songs = lib
        .songs(
            madrigal_library::SongScope::Library,
            madrigal_library::SongSort::Name,
            madrigal_library::SortDir::Asc,
        )
        .unwrap_or_default();
    for track in &songs {
        for artist in &track.artists {
            *counts.entry(artist.clone()).or_default() += 1;
        }
    }
    let mut cards: Vec<LibCardRow> = counts
        .into_iter()
        .map(|(name, count)| LibCardRow {
            id: name.clone().into(),
            title: name.into(),
            subtitle: format!("{count} songs").into(),
        })
        .collect();
    cards.sort_by(|a, b| a.title.as_str().cmp(b.title.as_str()));
    cards
}

pub fn album_cards(lib: &madrigal_library::Library<'_>) -> Vec<LibCardRow> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let songs = lib
        .songs(
            madrigal_library::SongScope::Library,
            madrigal_library::SongSort::Name,
            madrigal_library::SortDir::Asc,
        )
        .unwrap_or_default();
    for track in &songs {
        if let Some(album) = track.album.as_deref().filter(|name| !name.is_empty()) {
            *counts.entry(album.to_owned()).or_default() += 1;
        }
    }
    let mut cards: Vec<LibCardRow> = counts
        .into_iter()
        .map(|(name, count)| LibCardRow {
            id: name.clone().into(),
            title: name.into(),
            subtitle: format!("{count} songs").into(),
        })
        .collect();
    cards.sort_by(|a, b| a.title.as_str().cmp(b.title.as_str()));
    cards
}

pub fn history_stamp(played_at: i64, now: i64, lang: madrigal_i18n::Lang) -> String {
    use chrono::{Datelike, Local};
    let played = chrono::DateTime::from_timestamp(played_at, 0)
        .map(|when| when.with_timezone(&Local))
        .unwrap_or_else(Local::now);
    let now = chrono::DateTime::from_timestamp(now, 0)
        .map(|when| when.with_timezone(&Local))
        .unwrap_or_else(Local::now);
    let clock = played.format("%H:%M").to_string();
    if played.date_naive() == now.date_naive() {
        return format!("{} {clock}", madrigal_i18n::tr(lang, "detail.today"));
    }
    if Some(played.date_naive()) == now.date_naive().pred_opt() {
        return format!("{} {clock}", madrigal_i18n::tr(lang, "detail.yesterday"));
    }
    let month_key = format!("date.month.{:02}", played.month());
    let month = madrigal_i18n::tr(lang, &month_key);
    match lang {
        madrigal_i18n::Lang::Es => format!("{} {month} {clock}", played.day()),
        madrigal_i18n::Lang::En => format!("{month} {}, {clock}", played.day()),
    }
}

pub fn history_rows(
    lib: &madrigal_library::Library<'_>,
    lang: madrigal_i18n::Lang,
) -> Vec<HistoryRow> {
    let now = chrono::Local::now().timestamp();
    let titles: HashMap<String, (String, String)> = lib
        .songs(
            madrigal_library::SongScope::Library,
            madrigal_library::SongSort::DateAdded,
            madrigal_library::SortDir::Asc,
        )
        .unwrap_or_default()
        .into_iter()
        .map(|track| {
            let key = track.identity.to_storage_string();
            let artist = track.artist_text();
            (key, (track.title.clone(), artist))
        })
        .collect();
    lib.listen_history_entries(100)
        .unwrap_or_default()
        .into_iter()
        .map(|entry| {
            let key = entry.event.track.to_storage_string();
            let fallback = titles.get(&key);
            let title = entry
                .title
                .or_else(|| fallback.map(|(title, _)| title.clone()))
                .unwrap_or_else(|| key.clone());
            let artist = entry
                .artist
                .or_else(|| fallback.map(|(_, artist)| artist.clone()))
                .unwrap_or_default();
            HistoryRow {
                id: key.into(),
                title: title.into(),
                subtitle: format!(
                    "{artist} · {}",
                    history_stamp(entry.event.played_at, now, lang)
                )
                .into(),
            }
        })
        .collect()
}

fn format_listen_secs(total: u64) -> String {
    if total >= 3600 {
        format!("{}h {:02}m", total / 3600, total % 3600 / 60)
    } else {
        format!("{}m {:02}s", total / 60, total % 60)
    }
}

pub fn stats_totals(lib: &madrigal_library::Library<'_>) -> String {
    let stats = lib.stats().unwrap_or_default();
    format!(
        "{} plays · {} listened",
        stats.total_plays,
        format_listen_secs(stats.total_listen_secs)
    )
}

pub fn stats_streaks(lib: &madrigal_library::Library<'_>) -> String {
    let streaks = lib.streaks().unwrap_or_default();
    format!(
        "streak {}d · best {}d",
        streaks.current_days, streaks.longest_days
    )
}

pub fn top_track_rows(
    lib: &madrigal_library::Library<'_>,
    range: madrigal_settings::TopRange,
) -> Vec<StatRow> {
    lib.top_tracks(range, 5)
        .unwrap_or_default()
        .into_iter()
        .map(|top| StatRow {
            title: top.track.title.into(),
            value: top.plays.to_string().into(),
        })
        .collect()
}

pub fn top_artist_rows(
    lib: &madrigal_library::Library<'_>,
    range: madrigal_settings::TopRange,
) -> Vec<StatRow> {
    lib.top_artists(range, 5)
        .unwrap_or_default()
        .into_iter()
        .map(|top| StatRow {
            title: top.name.into(),
            value: top.plays.to_string().into(),
        })
        .collect()
}

pub fn top_album_rows(
    lib: &madrigal_library::Library<'_>,
    range: madrigal_settings::TopRange,
) -> Vec<StatRow> {
    lib.top_albums(range, 5)
        .unwrap_or_default()
        .into_iter()
        .map(|top| StatRow {
            title: top.name.into(),
            value: top.plays.to_string().into(),
        })
        .collect()
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 { shifted } else { shifted - 146_096 } / 146_097;
    let doe = (shifted - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let full = if month <= 2 { year + 1 } else { year };
    (full as i32, month, day)
}

pub fn event_years(lib: &madrigal_library::Library<'_>) -> Vec<String> {
    let mut years: Vec<i64> = lib
        .connection()
        .prepare("SELECT DISTINCT CAST(strftime('%Y', played_at, 'unixepoch') AS INTEGER) AS year FROM events ORDER BY year ASC")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| row.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default();
    if years.is_empty() {
        let (year, _, _) = civil_from_days(madrigal_library::now_unix() / 86_400);
        years.push(year as i64);
    }
    years.iter().map(|year| year.to_string()).collect()
}

pub fn year_index(years: &[String], year: i32) -> usize {
    let wanted = year.to_string();
    years.iter().position(|item| *item == wanted).unwrap_or(0)
}

pub fn year_model(
    lib: &madrigal_library::Library<'_>,
    year: i32,
) -> (Vec<StatRow>, Vec<StatRow>, Vec<StatRow>) {
    let music = lib.year_in_music(year, 5).unwrap_or_default();
    let tracks = music
        .top_tracks
        .into_iter()
        .map(|top| StatRow {
            title: top.track.title.into(),
            value: top.plays.to_string().into(),
        })
        .collect();
    let artists = music
        .top_artists
        .into_iter()
        .map(|top| StatRow {
            title: top.name.into(),
            value: top.plays.to_string().into(),
        })
        .collect();
    let albums = music
        .top_albums
        .into_iter()
        .map(|top| StatRow {
            title: top.name.into(),
            value: top.plays.to_string().into(),
        })
        .collect();
    (tracks, artists, albums)
}

pub fn download_state_index(state: madrigal_downloads::DownloadState) -> i32 {
    match state {
        madrigal_downloads::DownloadState::Queued => 0,
        madrigal_downloads::DownloadState::Downloading => 1,
        madrigal_downloads::DownloadState::Paused => 2,
        madrigal_downloads::DownloadState::Completed => 3,
        madrigal_downloads::DownloadState::Failed => 4,
        madrigal_downloads::DownloadState::Quarantined => 5,
    }
}

fn format_speed(speed_bps: Option<f64>) -> String {
    match speed_bps {
        Some(speed) if speed > 0.0 => {
            if speed >= 1_048_576.0 {
                format!("{:.1} MB/s", speed / 1_048_576.0)
            } else {
                format!("{:.0} KB/s", speed / 1024.0)
            }
        }
        _ => String::new(),
    }
}

pub struct DownloadItem {
    pub record: madrigal_downloads::DownloadRecord,
    pub state: madrigal_downloads::DownloadState,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bps: Option<f64>,
}

fn download_row(item: &DownloadItem) -> DownloadRow {
    let progress = match item.total_bytes {
        Some(total) if total > 0 => item.downloaded_bytes as f32 / total as f32,
        _ => {
            if item.state == madrigal_downloads::DownloadState::Completed {
                1.0
            } else {
                0.0
            }
        }
    };
    DownloadRow {
        id: item.record.id.clone().into(),
        title: item.record.title.clone().into(),
        subtitle: item.record.artists.join(", ").into(),
        state: download_state_index(item.state),
        progress,
        speed: format_speed(item.speed_bps).into(),
    }
}

pub fn download_models(items: &[DownloadItem]) -> (Vec<DownloadRow>, Vec<DownloadRow>) {
    let mut active = Vec::new();
    let mut failed = Vec::new();
    for item in items {
        match item.state {
            madrigal_downloads::DownloadState::Failed
            | madrigal_downloads::DownloadState::Quarantined => failed.push(download_row(item)),
            _ => active.push(download_row(item)),
        }
    }
    (active, failed)
}

pub struct LibraryFixture {
    pub root: tempfile::TempDir,
    pub conn: rusqlite::Connection,
    pub settings: Rc<RefCell<madrigal_settings::Settings>>,
}

impl LibraryFixture {
    pub fn library(&self) -> madrigal_library::Library<'_> {
        let settings = Rc::clone(&self.settings);
        madrigal_library::Library::with_settings_source(&self.conn, move || settings.borrow().clone())
    }
}

fn fixture_track(id: &str, title: &str, artists: &[&str], album: Option<&str>) -> madrigal_core::Track {
    madrigal_core::Track {
        identity: madrigal_core::TrackIdentity::youtube(id),
        title: title.to_owned(),
        artists: artists.iter().map(|name| name.to_string()).collect(),
        album: album.map(str::to_owned),
        duration_secs: Some(200),
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

pub fn load_library_fixture() -> LibraryFixture {
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    madrigal_paths::ensure_dirs().expect("dirs");
    let conn = madrigal_db::open(&madrigal_paths::db_path()).expect("open");
    let settings = Rc::new(RefCell::new(madrigal_settings::Settings::default()));
    let fixture = LibraryFixture { root, conn, settings };
    {
        let lib = fixture.library();

    let delta = fixture_track("ddd111", "Delta", &["Alice"], Some("Hits"));
    let alpha = fixture_track("aaa111", "Alpha", &["Alice"], Some("Hits"));
    let echo = fixture_track("eee111", "Echo", &["Bob"], Some("Deep"));
    let bravo = fixture_track("bbb111", "Bravo", &["Carol"], Some("Deep"));
    let charlie = fixture_track("ccc111", "Charlie", &["Alice", "Bob"], Some("Hits"));
    for track in [&delta, &alpha, &echo, &bravo, &charlie] {
        madrigal_db::upsert_song(&fixture.conn, track).expect("seed song");
    }

    let rock = lib.create_playlist("Rock night").expect("playlist");
    lib.add_tracks(&rock.id, &[delta.clone(), alpha.clone()], madrigal_library::DuplicatePolicy::Skip)
        .expect("add tracks");
    lib.assign_tag(&rock.id, "rock").expect("tag");
    lib.set_cover(&rock.id, Some("rock.png")).expect("cover");
    let chill = lib.create_playlist("Chill mix").expect("playlist");
    lib.add_tracks(&chill.id, &[echo.clone(), charlie.clone()], madrigal_library::DuplicatePolicy::Skip)
        .expect("add tracks");
    lib.assign_tag(&chill.id, "chill").expect("tag");

    lib.set_liked(&delta.identity, true).expect("like");
    lib.set_liked(&charlie.identity, true).expect("like");

    let midnight = madrigal_library::now_unix() / 86_400 * 86_400;
    let plays = [
        (&delta, 2, 200u32, 5u32),
        (&alpha, 1, 200, 1),
        (&echo, 1, 200, 4),
        (&bravo, 0, 200, 0),
        (&charlie, 0, 200, 2),
    ];
    for (track, day_ago, duration, count) in plays {
        for slot in 0..count {
            let event = madrigal_core::Event {
                track: track.identity.clone(),
                played_at: midnight - day_ago * 86_400 + 3600 + slot as i64,
                duration_secs: Some(duration),
                completed: true,
            };
            lib.record_play(&event).expect("record");
        }
    }
    }
    fixture
}

pub fn load_downloads_fixture() -> Vec<DownloadItem> {
    let states = [
        madrigal_downloads::DownloadState::Queued,
        madrigal_downloads::DownloadState::Downloading,
        madrigal_downloads::DownloadState::Paused,
        madrigal_downloads::DownloadState::Completed,
        madrigal_downloads::DownloadState::Failed,
        madrigal_downloads::DownloadState::Quarantined,
    ];
    states
        .into_iter()
        .enumerate()
        .map(|(index, state)| {
            let track = fixture_track(
                &format!("dl{index}"),
                &format!("Download {index}"),
                &["Alice"],
                None,
            );
            let record = madrigal_downloads::DownloadRecord::of(
                &track,
                std::path::Path::new(&format!("/tmp/dl{index}.mp3")),
                state,
            );
            DownloadItem {
                record,
                state,
                downloaded_bytes: if state == madrigal_downloads::DownloadState::Downloading {
                    512 * 1024
                } else {
                    0
                },
                total_bytes: Some(1024 * 1024),
                speed_bps: if state == madrigal_downloads::DownloadState::Downloading {
                    Some(256.0 * 1024.0)
                } else {
                    None
                },
            }
        })
        .collect()
}

pub fn matches_query(query: &str, fields: &[&str]) -> bool {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    fields
        .iter()
        .any(|field| field.to_lowercase().contains(&needle))
}

pub fn filter_songs(rows: Vec<LibSongRow>, query: &str) -> Vec<LibSongRow> {
    rows.into_iter()
        .filter(|row| matches_query(query, &[row.title.as_str(), row.subtitle.as_str()]))
        .collect()
}

pub fn filter_playlists(rows: Vec<LibPlaylistRow>, query: &str) -> Vec<LibPlaylistRow> {
    rows.into_iter()
        .filter(|row| {
            matches_query(
                query,
                &[row.title.as_str(), row.subtitle.as_str(), row.tags.as_str()],
            )
        })
        .collect()
}

pub fn filter_cards(rows: Vec<LibCardRow>, query: &str) -> Vec<LibCardRow> {
    rows.into_iter()
        .filter(|row| matches_query(query, &[row.title.as_str(), row.subtitle.as_str()]))
        .collect()
}

pub fn filter_history(rows: Vec<HistoryRow>, query: &str) -> Vec<HistoryRow> {
    rows.into_iter()
        .filter(|row| matches_query(query, &[row.title.as_str(), row.subtitle.as_str()]))
        .collect()
}

pub fn filter_downloads(rows: Vec<DownloadRow>, query: &str) -> Vec<DownloadRow> {
    rows.into_iter()
        .filter(|row| matches_query(query, &[row.title.as_str(), row.subtitle.as_str()]))
        .collect()
}

pub fn downloaded_rows(lib: &madrigal_library::Library<'_>) -> Vec<DownloadRow> {
    let entries = lib
        .connection()
        .prepare("SELECT song_key, title, artists FROM downloads ORDER BY created_at DESC")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default();
    let completed = download_state_index(madrigal_downloads::DownloadState::Completed);
    entries
        .into_iter()
        .map(|(key, title, artists)| DownloadRow {
            id: key.into(),
            title: title.into(),
            subtitle: serde_json::from_str::<Vec<String>>(&artists)
                .unwrap_or_default()
                .join(", ")
                .into(),
            state: completed,
            progress: 1.0,
            speed: "".into(),
        })
        .collect()
}

#[derive(Default)]
pub struct LibraryModels {
    pub chips: Vec<String>,
    pub sort_options: Vec<String>,
    pub sort: i32,
    pub tags: Vec<String>,
    pub songs: Vec<LibSongRow>,
    pub playlists: Vec<LibPlaylistRow>,
    pub cards: Vec<LibCardRow>,
    pub history: Vec<HistoryRow>,
    pub history_paused: bool,
    pub stats_totals: String,
    pub stats_streaks: String,
    pub stat_tracks: Vec<StatRow>,
    pub stat_artists: Vec<StatRow>,
    pub stat_albums: Vec<StatRow>,
    pub years: Vec<String>,
    pub year: i32,
    pub downloads: Vec<DownloadRow>,
    pub failed_downloads: Vec<DownloadRow>,
    pub selected_count: i32,
    pub liked: Option<bool>,
    pub recent_searches: Vec<String>,
}

fn string_model(values: &[String]) -> ModelRc<slint::SharedString> {
    ModelRc::new(VecModel::from(
        values
            .iter()
            .map(|value| slint::SharedString::from(value.as_str()))
            .collect::<Vec<_>>(),
    ))
}

fn row_model<T>(values: &[T]) -> ModelRc<T>
where
    T: Clone + 'static,
{
    ModelRc::new(VecModel::from(values.to_vec()))
}

pub fn apply_library(shell: &MainShell, models: &LibraryModels) {
    shell.set_lib_chips(string_model(&models.chips));
    shell.set_lib_sort_options(string_model(&models.sort_options));
    shell.set_lib_sort(models.sort);
    shell.set_lib_tags(string_model(&models.tags));
    shell.set_lib_songs(row_model(&models.songs));
    shell.set_lib_playlists(row_model(&models.playlists));
    shell.set_lib_cards(row_model(&models.cards));
    shell.set_lib_history(row_model(&models.history));
    shell.set_lib_history_paused(models.history_paused);
    shell.set_lib_stats_totals(models.stats_totals.as_str().into());
    shell.set_lib_stats_streaks(models.stats_streaks.as_str().into());
    shell.set_lib_stat_tracks(row_model(&models.stat_tracks));
    shell.set_lib_stat_artists(row_model(&models.stat_artists));
    shell.set_lib_stat_albums(row_model(&models.stat_albums));
    shell.set_lib_years(string_model(&models.years));
    shell.set_lib_year(models.year);
    shell.set_lib_selected_count(models.selected_count);
    shell.set_dl_rows(row_model(&models.downloads));
    shell.set_dl_failed(row_model(&models.failed_downloads));
    shell.set_search_recent(string_model(&models.recent_searches));
    if let Some(liked) = models.liked {
        shell.set_liked(liked);
    }
}
