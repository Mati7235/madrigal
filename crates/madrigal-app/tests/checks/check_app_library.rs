use std::path::PathBuf;

use madrigal_app::library::{Observed, Worker};
use madrigal_innertube::CatalogClient;
use madrigal_library::{DuplicatePolicy, Library};
use madrigal_settings::Settings;

const HISTORY_TAB: i32 = 5;
const PLAYLISTS_TAB: i32 = 1;
const STATS_TAB: i32 = 6;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../madrigal-innertube/tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).expect("read fixture")
}

#[test]
fn worker_drives_lists_likes_history_and_filters() {
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, root.path());
    madrigal_paths::ensure_dirs().expect("dirs");
    let conn = madrigal_db::open(&madrigal_paths::db_path()).expect("open");

    let tracks = CatalogClient::parse_search_songs(&fixture("search_songs_beatles.json"))
        .expect("parse songs");
    assert!(tracks.len() >= 5);
    for track in &tracks {
        madrigal_db::upsert_song(&conn, track).expect("seed song");
    }
    let mut worker = Worker::new(conn, Settings::default());
    let mut obs = Observed::default();

    let models = worker.models();
    assert_eq!(models.songs.len(), tracks.len());
    assert!(models.songs.iter().all(|row| !row.selected && !row.liked));

    let needle = "tomorrow";
    let expected = tracks
        .iter()
        .filter(|track| {
            track.title.to_lowercase().contains(needle)
                || track.artist_text().to_lowercase().contains(needle)
        })
        .count();
    assert!(expected >= 1);
    obs.query = "Tomorrow".to_owned();
    let models = worker.handle(obs.clone()).expect("query refresh");
    assert_eq!(models.songs.len(), expected);
    assert!(models.songs.iter().all(|row| {
        row.title.to_lowercase().contains(needle) || row.subtitle.to_lowercase().contains(needle)
    }));
    obs.query.clear();
    let models = worker.handle(obs.clone()).expect("query cleared");
    assert_eq!(models.songs.len(), tracks.len());

    let playlist = {
        let lib = Library::new(worker.connection(), Settings::default());
        let playlist = lib.create_playlist("Beatles mix").expect("playlist");
        lib.add_tracks(&playlist.id, &tracks[..2], DuplicatePolicy::Skip)
            .expect("add tracks");
        playlist
    };
    obs.tab = PLAYLISTS_TAB;
    let models = worker.handle(obs.clone()).expect("tab refresh");
    assert_eq!(models.playlists.len(), 1);
    assert_eq!(models.playlists[0].title.as_str(), "Beatles mix");

    obs.tab = 0;
    obs.multiselect = true;
    obs.action_count += 1;
    obs.last_action = "song-chosen".to_owned();
    obs.last_arg = tracks[0].identity.to_storage_string();
    let models = worker.handle(obs.clone()).expect("selection");
    assert_eq!(models.selected_count, 1);
    assert!(models.songs.iter().any(|row| row.selected));

    obs.like_count += 1;
    obs.track_id = tracks[0].identity.id.clone();
    obs.track_source = 0;
    let models = worker.handle(obs.clone()).expect("like");
    assert_eq!(models.liked, Some(true));

    obs.track_id = tracks[1].identity.id.clone();
    obs.duration_ms = 200_000;
    worker.handle(obs.clone());
    obs.position_ms = 120_000;
    worker.handle(obs.clone());
    obs.track_id = tracks[2].identity.id.clone();
    obs.position_ms = 0;
    worker.handle(obs.clone());
    obs.track_id = tracks[3].identity.id.clone();
    obs.position_ms = 5_000;
    worker.handle(obs.clone());
    obs.track_id = tracks[4].identity.id.clone();
    obs.position_ms = 0;
    worker.handle(obs.clone());

    obs.tab = HISTORY_TAB;
    let models = worker.handle(obs.clone()).expect("history refresh");
    let titles: Vec<String> = models
        .history
        .iter()
        .map(|row| row.title.to_string())
        .collect();
    assert_eq!(
        titles,
        vec![tracks[3].title.clone(), tracks[1].title.clone()],
        "{:?}",
        tracks.iter().map(|t| t.title.clone()).collect::<Vec<_>>()
    );
    assert_eq!(models.history[0].title.as_str(), tracks[3].title.as_str());

    obs.action_count += 1;
    obs.last_action = "history-cleared".to_owned();
    let models = worker.handle(obs.clone()).expect("history cleared");
    assert!(models.history.is_empty());

    obs.tab = STATS_TAB;
    obs.year = -1;
    let models = worker.handle(obs.clone()).expect("stats refresh");
    assert!(!models.years.is_empty());
    assert!(!models.stats_totals.is_empty());

    obs.tab = 0;
    obs.action_count += 1;
    obs.last_action = "playlist-opened".to_owned();
    obs.last_arg = playlist.id.as_str().to_owned();
    worker.handle(obs.clone());
    obs.action_count += 1;
    obs.last_action = "tag-created".to_owned();
    obs.last_arg = "rock".to_owned();
    worker.handle(obs.clone());
    obs.tab = PLAYLISTS_TAB;
    let models = worker.handle(obs.clone()).expect("tag refresh");
    assert!(models.tags.iter().any(|tag| tag == "rock"));
}
