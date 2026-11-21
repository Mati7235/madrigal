use std::cell::RefCell;
use std::env;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};

use madrigal_core::{Event, SearchHistoryItem, TrackIdentity};
use madrigal_library::{now_unix, Library};
use madrigal_settings::Settings;

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

#[test]
fn listen_history_pause_clear_and_window() {
    let _guard = lock_profile();
    let fx = Fixture::setup();
    let lib = fx.library();

    let now = now_unix();
    let first = Event {
        track: TrackIdentity::youtube("aaa111"),
        played_at: now - 60,
        duration_secs: Some(200),
        completed: true,
    };
    let second = Event {
        track: TrackIdentity::youtube("bbb222"),
        played_at: now,
        duration_secs: Some(300),
        completed: false,
    };
    assert!(lib.record_play(&first).expect("record"));
    assert!(lib.record_play(&second).expect("record"));
    assert!(lib.record_play(&Event { track: TrackIdentity::youtube(""), played_at: now, duration_secs: None, completed: false }).is_err());

    let history = lib.listen_history(10).expect("history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].track, second.track);
    assert_eq!(history[1].track, first.track);

    fx.settings.borrow_mut().privacy.pause_listen_history = true;
    let paused = Event {
        track: TrackIdentity::youtube("ccc333"),
        played_at: now,
        duration_secs: Some(100),
        completed: true,
    };
    assert!(!lib.record_play(&paused).expect("paused"));
    assert_eq!(lib.listen_history(10).expect("history").len(), 2);
    fx.settings.borrow_mut().privacy.pause_listen_history = false;

    fx.settings.borrow_mut().privacy.history_duration_days = 60;
    let old = Event {
        track: TrackIdentity::youtube("ddd444"),
        played_at: now - 10 * 86_400,
        duration_secs: Some(150),
        completed: true,
    };
    assert!(lib.record_play(&old).expect("old"));
    assert_eq!(lib.listen_history(10).expect("history").len(), 3);

    fx.settings.borrow_mut().privacy.history_duration_days = 5;
    let windowed = lib.listen_history(10).expect("history");
    assert_eq!(windowed.len(), 2);
    assert!(windowed.iter().all(|event| event.track != old.track));

    lib.clear_listen_history().expect("clear");
    assert!(lib.listen_history(10).expect("history").is_empty());
    assert_eq!(lib.stats().expect("stats").total_plays, 0);
}

#[test]
fn listen_history_keeps_title_and_artist() {
    let _guard = lock_profile();
    let fx = Fixture::setup();
    let lib = fx.library();

    let event = Event {
        track: TrackIdentity::youtube("metadata1"),
        played_at: now_unix(),
        duration_secs: Some(200),
        completed: false,
    };
    assert!(lib
        .record_play_meta(&event, Some("The Winter of Our Passing"), Some("Katatonia"))
        .expect("record"));
    let entries = lib.listen_history_entries(10).expect("entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].title.as_deref(),
        Some("The Winter of Our Passing")
    );
    assert_eq!(entries[0].artist.as_deref(), Some("Katatonia"));
    assert_eq!(entries[0].event, event);
    assert_eq!(lib.listen_history(10).expect("events"), vec![event]);
}

#[test]
fn search_history_dedupes_orders_caps_and_persists() {
    let _guard = lock_profile();
    let fx = Fixture::setup();
    let lib = fx.library();

    assert!(lib.record_search("one").expect("one"));
    assert!(lib.record_search("two").expect("two"));
    assert!(lib.record_search("one").expect("one again"));
    let history = lib.search_history(10).expect("history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].query, "one");
    assert_eq!(history[1].query, "two");

    for index in 0..12 {
        assert!(lib.record_search(&format!("q{index}")).expect("record"));
    }
    let history = lib.search_history(10).expect("capped");
    assert_eq!(history.len(), 10);
    assert_eq!(history[0].query, "q11");
    assert!(history.iter().all(|item| item.query != "one"));

    drop(lib);
    let conn = madrigal_db::open(&madrigal_paths::db_path()).expect("reopen");
    let reopened = Library::new(&conn, Settings::default());
    let persisted = reopened.search_history(10).expect("persisted");
    assert_eq!(persisted.len(), 10);
    assert_eq!(persisted[0].query, "q11");

    reopened.remove_search("q11").expect("remove");
    let history = reopened.search_history(10).expect("after remove");
    assert_eq!(history.len(), 9);
    assert!(history.iter().all(|item| item.query != "q11"));
}

#[test]
fn search_history_pause_clear_and_window() {
    let _guard = lock_profile();
    let fx = Fixture::setup();
    let lib = fx.library();

    assert!(lib.record_search("never gonna give you up").expect("record"));
    assert!(!lib.record_search("  ").expect("blank"));
    assert!(lib.record_search("rick astley").expect("record"));

    let history = lib.search_history(10).expect("history");
    assert_eq!(history.len(), 2);
    assert!(history.iter().any(|item| item.query == "never gonna give you up"));
    assert!(history.iter().any(|item| item.query == "rick astley"));

    fx.settings.borrow_mut().privacy.pause_search_history = true;
    assert!(!lib.record_search("paused query").expect("paused"));
    assert_eq!(lib.search_history(10).expect("history").len(), 2);
    fx.settings.borrow_mut().privacy.pause_search_history = false;

    let now = now_unix();
    madrigal_db::record_search(
        &fx.conn,
        &SearchHistoryItem {
            query: "old query".to_owned(),
            searched_at: now - 10 * 86_400,
        },
    )
    .expect("old search");

    fx.settings.borrow_mut().privacy.history_duration_days = 60;
    assert_eq!(lib.search_history(10).expect("history").len(), 3);

    fx.settings.borrow_mut().privacy.history_duration_days = 5;
    let windowed = lib.search_history(10).expect("history");
    assert_eq!(windowed.len(), 2);
    assert!(windowed.iter().all(|item| item.query != "old query"));

    lib.clear_search_history().expect("clear");
    assert!(lib.search_history(10).expect("history").is_empty());
}
