use std::cell::RefCell;
use std::env;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};

use madrigal_core::{Event, Track, TrackIdentity};
use madrigal_library::{now_unix, Library};
use madrigal_settings::{Settings, TopRange};

static PROFILE_LOCK: Mutex<()> = Mutex::new(());

fn lock_profile() -> MutexGuard<'static, ()> {
    PROFILE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

struct Fixture {
    _root: tempfile::TempDir,
    conn: rusqlite::Connection,
    settings: Rc<RefCell<Settings>>,
}

impl Fixture {
    fn setup() -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
        madrigal_paths::ensure_dirs().expect("dirs");
        let conn = madrigal_db::open(&madrigal_paths::db_path()).expect("open");
        Self {
            _root: root,
            conn,
            settings: Rc::new(RefCell::new(Settings::default())),
        }
    }

    fn library(&self) -> Library<'_> {
        Library::with_settings_source(&self.conn, || self.settings.borrow().clone())
    }
}

fn track(id: &str, title: &str, artists: &[&str], album: Option<&str>, duration: u32) -> Track {
    Track {
        identity: TrackIdentity::youtube(id),
        title: title.to_owned(),
        artists: artists.iter().map(|name| name.to_string()).collect(),
        album: album.map(str::to_owned),
        duration_secs: Some(duration),
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let adjusted = if month <= 2 { year - 1 } else { year };
    let era = if adjusted >= 0 { adjusted } else { adjusted - 399 } / 400;
    let yoe = adjusted - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[test]
fn stats_counts_streaks_and_listen_time() {
    let _guard = lock_profile();
    let fx = Fixture::setup();
    let lib = fx.library();

    let first = track("aaa111", "Alpha", &["Alice"], Some("Hits"), 200);
    let second = track("bbb222", "Beta", &["Alice", "Bob"], Some("Hits"), 300);
    let third = track("ccc333", "Gamma", &["Carol"], Some("Deep"), 400);
    madrigal_db::upsert_album(
        &fx.conn,
        &madrigal_core::Album {
            id: madrigal_core::AlbumId::new("ALB1"),
            title: "Hits".to_owned(),
            artists: vec!["Alice".to_owned()],
            artist_id: None,
            year: Some("2024".to_owned()),
            thumbnail: None,
            track_count: None,
            audio_playlist_id: None,
        },
    )
    .expect("album");
    madrigal_db::upsert_album(
        &fx.conn,
        &madrigal_core::Album {
            id: madrigal_core::AlbumId::new("ALB2"),
            title: "Deep".to_owned(),
            artists: vec!["Carol".to_owned()],
            artist_id: None,
            year: Some("2023".to_owned()),
            thumbnail: None,
            track_count: None,
            audio_playlist_id: None,
        },
    )
    .expect("album");
    for seed in [&first, &second, &third] {
        madrigal_db::upsert_song(&fx.conn, seed).expect("seed");
    }

    let midnight = now_unix() / 86_400 * 86_400;
    let play = |identity: &TrackIdentity, day_offset: i64, slot: i64, duration: u32| Event {
        track: identity.clone(),
        played_at: midnight - day_offset * 86_400 + 3600 + slot,
        duration_secs: Some(duration),
        completed: true,
    };
    let plays = [
        play(&first.identity, 2, 1, 200),
        play(&first.identity, 1, 1, 200),
        play(&first.identity, 1, 2, 200),
        play(&second.identity, 1, 3, 300),
        play(&second.identity, 0, 1, 300),
        play(&third.identity, 0, 2, 400),
        play(&third.identity, 0, 3, 400),
        play(&third.identity, 0, 4, 400),
    ];
    for event in &plays {
        assert!(lib.record_play(event).expect("record"));
    }

    assert_eq!(lib.play_count(&first.identity).expect("count"), 3);
    assert_eq!(lib.play_count(&second.identity).expect("count"), 2);
    assert_eq!(lib.play_count(&third.identity).expect("count"), 3);
    assert_eq!(
        lib.play_count(&TrackIdentity::youtube("missing")).expect("count"),
        0
    );

    let stats = lib.stats().expect("stats");
    assert_eq!(stats.total_plays, 8);
    assert_eq!(stats.unique_tracks, 3);
    assert_eq!(stats.unique_artists, 3);
    assert_eq!(stats.total_listen_secs, 3 * 200 + 2 * 300 + 3 * 400);

    let streaks = lib.streaks().expect("streaks");
    assert_eq!(streaks.current_days, 3);
    assert_eq!(streaks.longest_days, 3);

    let tops = lib.top_tracks(TopRange::AllTime, 10).expect("tops");
    assert_eq!(tops.len(), 3);
    assert_eq!(tops[0].track.identity, third.identity);
    assert_eq!(tops[0].plays, 3);
    assert_eq!(tops[1].track.identity, first.identity);
    assert_eq!(tops[1].plays, 3);
    assert_eq!(tops[2].track.identity, second.identity);
    assert_eq!(tops[2].plays, 2);

    let artists = lib.top_artists(TopRange::AllTime, 10).expect("artists");
    assert_eq!(artists.len(), 3);
    assert_eq!(artists[0].name, "Alice");
    assert_eq!(artists[0].plays, 5);
    assert_eq!(artists[1].name, "Carol");
    assert_eq!(artists[1].plays, 3);
    assert_eq!(artists[2].name, "Bob");
    assert_eq!(artists[2].plays, 2);

    let albums = lib.top_albums(TopRange::AllTime, 10).expect("albums");
    assert_eq!(albums.len(), 2);
    assert_eq!(albums[0].name, "Hits");
    assert_eq!(albums[0].plays, 5);
    assert_eq!(albums[1].name, "Deep");
    assert_eq!(albums[1].plays, 3);

    let day = lib.top_tracks(TopRange::Day, 10).expect("day tops");
    let day_cutoff = now_unix() - 86_400;
    let expected_day: std::collections::HashSet<_> = plays
        .iter()
        .filter(|event| event.played_at >= day_cutoff)
        .map(|event| event.track.clone())
        .collect();
    assert_eq!(day.len(), expected_day.len());
    assert!(day.iter().all(|top| expected_day.contains(&top.track.identity)));

    let playlists = lib.top_playlists(10).expect("top playlists");
    assert!(playlists.is_empty());
    let mix = lib.create_playlist("Mix").expect("create");
    lib.add_tracks(
        &mix.id,
        &[first.clone(), third.clone()],
        madrigal_library::DuplicatePolicy::Skip,
    )
    .expect("add");
    let playlists = lib.top_playlists(10).expect("top playlists");
    assert_eq!(playlists.len(), 1);
    assert_eq!(playlists[0].0.id, mix.id);
    assert_eq!(playlists[0].1, 6);
}

#[test]
fn year_in_music_tops_a_seeded_year() {
    let _guard = lock_profile();
    let fx = Fixture::setup();
    let lib = fx.library();

    let first = track("aaa111", "Alpha", &["Alice"], Some("Hits"), 200);
    let second = track("bbb222", "Beta", &["Bob"], Some("Deep"), 300);
    let third = track("ccc333", "Gamma", &["Alice"], Some("Hits"), 400);
    for seed in [&first, &second, &third] {
        madrigal_db::upsert_song(&fx.conn, seed).expect("seed");
    }

    let base_2024 = days_from_civil(2024, 1, 1) * 86_400;
    let base_2023 = days_from_civil(2023, 6, 15) * 86_400;
    let dated = |identity: &TrackIdentity, at: i64| Event {
        track: identity.clone(),
        played_at: at,
        duration_secs: Some(200),
        completed: true,
    };
    for (index, event) in [
        dated(&first.identity, base_2024 + 3600),
        dated(&first.identity, base_2024 + 86_400 + 10),
        dated(&first.identity, base_2024 + 2 * 86_400 + 20),
        dated(&second.identity, base_2024 + 3 * 86_400 + 30),
        dated(&third.identity, base_2024 + 4 * 86_400 + 40),
        dated(&third.identity, base_2024 + 5 * 86_400 + 50),
        dated(&second.identity, base_2023),
    ]
    .iter()
    .enumerate()
    {
        let _ = index;
        madrigal_db::record_event(&fx.conn, event).expect("seed event");
    }

    let summary = lib.year_in_music(2024, 10).expect("2024");
    assert_eq!(summary.year, 2024);
    assert_eq!(summary.top_tracks.len(), 3);
    assert_eq!(summary.top_tracks[0].track.identity, first.identity);
    assert_eq!(summary.top_tracks[0].plays, 3);
    assert_eq!(summary.top_tracks[1].track.identity, third.identity);
    assert_eq!(summary.top_tracks[1].plays, 2);
    assert_eq!(summary.top_tracks[2].track.identity, second.identity);
    assert_eq!(summary.top_tracks[2].plays, 1);
    assert_eq!(summary.top_artists.len(), 2);
    assert_eq!(summary.top_artists[0].name, "Alice");
    assert_eq!(summary.top_artists[0].plays, 5);
    assert_eq!(summary.top_artists[1].name, "Bob");
    assert_eq!(summary.top_artists[1].plays, 1);
    assert_eq!(summary.top_albums.len(), 2);
    assert_eq!(summary.top_albums[0].name, "Hits");
    assert_eq!(summary.top_albums[0].plays, 5);

    let previous = lib.year_in_music(2023, 10).expect("2023");
    assert_eq!(previous.top_tracks.len(), 1);
    assert_eq!(previous.top_tracks[0].track.identity, second.identity);

    let empty = lib.year_in_music(2022, 10).expect("2022");
    assert!(empty.top_tracks.is_empty());
    assert!(empty.top_artists.is_empty());
    assert!(empty.top_albums.is_empty());

    assert!(lib.year_in_music(1969, 10).is_err());
}
