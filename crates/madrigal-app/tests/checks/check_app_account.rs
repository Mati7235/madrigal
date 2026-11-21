use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use madrigal_account::store::SessionStore;
use madrigal_app::account::{
    run_login_with, wire_account_with, LoginOutcome, ERR_AUTH, ERR_NO_FIREFOX,
};
use madrigal_app::catalog::{
    wire_catalog_with, CatalogEvent, CatalogSource, ExplorePages,
};
use madrigal_innertube::{AccountInfo, CatalogClient, HomeFeed, SearchFilter, SearchOutcome};
use madrigal_settings::Content;
use madrigal_testkit as kit;
use madrigal_ui::{wire_shell, wire_shell_i18n, MainShell, SettingsPresenter};
use slint::ComponentHandle;

static ENV: Mutex<()> = Mutex::new(());
static HEADLESS: std::sync::Once = std::sync::Once::new();

fn headless_once() {
    HEADLESS.call_once(kit::init_headless);
}

const SCHEMA: &str = "CREATE TABLE moz_cookies (id INTEGER PRIMARY KEY, originAttributes TEXT NOT NULL DEFAULT '', name TEXT, value TEXT, host TEXT, path TEXT, expiry INTEGER, lastAccessed INTEGER, creationTime INTEGER, isSecure INTEGER, isHttpOnly INTEGER, inBrowserElement INTEGER DEFAULT 0, sameSite INTEGER DEFAULT 0, rawSameSite INTEGER DEFAULT 0, schemeMap INTEGER DEFAULT 0, isPartitionedAttributeSet INTEGER DEFAULT 0, CONSTRAINT moz_uniqueid UNIQUE (name, host, path, originAttributes))";

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../madrigal-innertube/tests/fixtures")
            .join(name),
    )
    .expect("read fixture")
}

struct FixtureSource;

impl CatalogSource for FixtureSource {
    fn home(&mut self) -> Result<HomeFeed, madrigal_innertube::Error> {
        CatalogClient::parse_home(&fixture("home.json"))
    }

    fn suggestions(&mut self, _query: &str) -> Result<Vec<String>, madrigal_innertube::Error> {
        CatalogClient::parse_suggestions(&fixture("suggestions_beat.json"))
    }

    fn search(
        &mut self,
        _query: &str,
        filter: SearchFilter,
    ) -> Result<SearchOutcome, madrigal_innertube::Error> {
        match filter {
            SearchFilter::Songs => Ok(SearchOutcome::Songs(CatalogClient::parse_search_songs(
                &fixture("search_songs_beatles.json"),
            )?)),
            SearchFilter::Artists => Ok(SearchOutcome::Artists(
                CatalogClient::parse_search_artists(&fixture("search_artists_beatles.json"))?,
            )),
            SearchFilter::Albums => Ok(SearchOutcome::Albums(CatalogClient::parse_search_albums(
                &fixture("search_albums_beatles.json"),
            )?)),
        }
    }

    fn explore(&mut self, country: &str) -> Result<ExplorePages, madrigal_innertube::Error> {
        let mut charts = CatalogClient::parse_charts(&fixture("charts_ZZ.json"))?;
        charts.country = country.to_owned();
        Ok(ExplorePages {
            moods: CatalogClient::parse_mood_listing(&fixture("moods.json"))?,
            releases: CatalogClient::parse_new_releases(&fixture("explore.json"))?,
            browse: CatalogClient::parse_browse(&fixture("explore.json"))?,
            charts,
        })
    }
}

fn isolate() -> (std::sync::MutexGuard<'static, ()>, tempfile::TempDir, tempfile::TempDir) {
    let guard = ENV.lock().expect("env lock");
    let root = tempfile::tempdir().expect("root");
    let home = tempfile::tempdir().expect("home");
    std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, root.path());
    std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", root.path().join("cache"));
    std::env::set_var("XDG_STATE_HOME", root.path().join("state"));
    std::env::set_var("HOME", home.path());
    std::env::set_var("MADRIGAL_KEYRING", "0");
    (guard, root, home)
}

fn firefox_layout(home: &Path) {
    let root = home.join(".mozilla/firefox");
    let rel = root.join("rel.default-release");
    std::fs::create_dir_all(&rel).expect("rel dir");
    std::fs::write(
        root.join("profiles.ini"),
        "[General]\nStartWithLastProfile=1\nVersion=2\n\n[InstallTEST123]\nDefault=rel.default-release\nLocked=1\n\n[Profile0]\nName=default-rel\nIsRelative=1\nPath=rel.default-release\nDefault=1\n",
    )
    .expect("profiles.ini");
    let path = rel.join("cookies.sqlite");
    let conn = rusqlite::Connection::open(&path).expect("open");
    conn.execute_batch(SCHEMA).expect("schema");
    for (name, value, host) in [
        ("LOGIN_INFO", "login-test", ".youtube.com"),
        ("SAPISID", "sapisid-test", ".youtube.com"),
        ("__Secure-3PAPISID", "sapisid3-test", ".youtube.com"),
        ("PREF", "hl=es", ".youtube.com"),
        ("NID", "google-test", ".google.com"),
    ] {
        conn.execute(
            "INSERT INTO moz_cookies (originAttributes, name, value, host, path, expiry, isSecure, isHttpOnly) VALUES ('', ?1, ?2, ?3, '/', 1900000000, 1, 0)",
            rusqlite::params![name, value, host],
        )
        .expect("insert");
    }
    drop(conn);
}


fn loop_until<F: Fn() -> bool>(done: F) {
    let started = Instant::now();
    loop {
        if done() {
            return;
        }
        kit::mock_advance(Duration::from_millis(50));
        std::thread::sleep(Duration::from_millis(20));
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "login did not settle"
        );
    }
}

fn slow_ok(_cookie: &str) -> Result<AccountInfo, String> {
    std::thread::sleep(Duration::from_secs(1));
    Ok(account_info())
}

fn account_info() -> AccountInfo {
    AccountInfo {
        name: "Sample User".to_owned(),
        handle: Some("@sampleuser".to_owned()),
        email: Some("sample@example.com".to_owned()),
        channel_id: Some("UC00000000000000000000".to_owned()),
        avatar: Some("https://example.com/avatar.png".to_owned()),
    }
}

fn validator_ok(_cookie: &str) -> Result<AccountInfo, String> {
    Ok(account_info())
}

fn validator_fail(_cookie: &str) -> Result<AccountInfo, String> {
    Err("rejected".to_owned())
}

fn validator_not_signed_in(_cookie: &str) -> Result<AccountInfo, String> {
    Err("account menu shows no signed-in account".to_owned())
}

#[test]
fn app_account() {
    let (_guard, root, home) = isolate();

    assert_eq!(
        run_login_with(&validator_ok),
        LoginOutcome::Failed { error: ERR_NO_FIREFOX.to_owned() },
        "missing firefox maps to errNoFirefox"
    );

    firefox_layout(home.path());

    let outcome = run_login_with(&validator_fail);
    assert_eq!(
        outcome,
        LoginOutcome::Failed { error: ERR_AUTH.to_owned() },
        "rejected cookies map to errAuth"
    );
    let outcome = run_login_with(&validator_not_signed_in);
    assert_eq!(
        outcome,
        LoginOutcome::Failed { error: ERR_AUTH.to_owned() },
        "logged-out account menu maps to errAuth"
    );
    assert!(
        !SessionStore::default().active().catalog_ready(),
        "failed login must not store a session"
    );

    let outcome = run_login_with(&validator_ok);
    assert_eq!(
        outcome,
        LoginOutcome::Connected {
            name: "Sample User".to_owned(),
            email: "sample@example.com".to_owned(),
        },
        "login reports the account from the menu"
    );
    let stored = SessionStore::default().active();
    assert!(stored.logged_in() && stored.catalog_ready(), "session stored");
    assert_eq!(stored.account_name, "Sample User");
    assert_eq!(stored.account_email, "sample@example.com");
    assert_eq!(stored.channel_handle, "@sampleuser");
    assert_eq!(stored.channel_id, "UC00000000000000000000");
    assert_eq!(stored.avatar_url, "https://example.com/avatar.png");
    assert!(stored.cookie.contains("LOGIN_INFO=login-test"), "login cookie stored");
    assert!(stored.cookie.contains("SAPISID=sapisid-test"), "sapisid stored");
    assert!(stored.cookie.contains("__Secure-3PAPISID=sapisid3-test"), "primary sapisid stored");
    assert!(!stored.cookie.contains("google-test"), "google cookie dropped");
    assert_eq!(stored.source_browser, "firefox");
    let mut unnamed = stored.clone();
    unnamed.account_name.clear();
    SessionStore::default()
        .save(&unnamed, None)
        .expect("save unnamed");
    assert!(
        madrigal_app::account::restore_session(&SessionStore::default()).is_none(),
        "a session without a name is not shown as connected"
    );
    madrigal_app::account::sign_out(&SessionStore::default());
    assert!(
        !SessionStore::default().active().catalog_ready(),
        "cleared before shell wiring"
    );

    headless_once();
    let shell = MainShell::new().expect("shell");
    let lang = wire_shell_i18n(&shell, madrigal_i18n::Lang::Es);
    let settings_path = root.path().join("settings.json");
    madrigal_settings::Settings::default()
        .save_to(&settings_path)
        .expect("seed settings");
    let presenter = Rc::new(RefCell::new(SettingsPresenter::load(
        &settings_path,
        SessionStore::default().ids(),
        &root.path().join("diag"),
    )));
    wire_shell(&shell, presenter.clone(), &lang);
    let (sink, _rx) = std::sync::mpsc::channel::<CatalogEvent>();
    let sink = Arc::new(Mutex::new(sink));
    let delivery: madrigal_app::catalog::Delivery = Arc::new(move |_event| {
        let _guard = sink.lock();
    });
    let wiring = Arc::new(
        wire_catalog_with(&shell, &Content::default(), Box::new(FixtureSource), delivery)
            .expect("wire catalog"),
    );
    let _timer = wire_account_with(&shell, presenter.clone(), Arc::clone(&wiring), slow_ok);
    shell.show().expect("show");

    assert!(!shell.get_settings_account_connected());
    let started = Instant::now();
    shell.invoke_settings_login("firefox".into());
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_millis(500),
        "login handler blocked the UI thread for {elapsed:?}"
    );
    assert!(shell.get_settings_account_busy(), "busy while background works");

    loop_until(|| shell.get_settings_account_connected());
    assert!(!shell.get_settings_account_busy());
    assert!(shell.get_settings_account_error().is_empty());
    assert_eq!(shell.get_settings_account_name().as_str(), "Sample User");
    assert_eq!(
        shell.get_settings_account_email().as_str(),
        "sample@example.com"
    );
    assert!(SessionStore::default().active().catalog_ready());

    shell.invoke_settings_login("signout".into());
    assert!(!shell.get_settings_account_connected());
    assert!(
        !SessionStore::default().active().catalog_ready(),
        "sign out clears the stored session"
    );
    let jar = SessionStore::default()
        .cookies_path(None)
        .map(|path| std::fs::read_to_string(path).unwrap_or_default())
        .unwrap_or_default();
    assert!(!jar.contains("LOGIN_INFO"), "sign out removes the cookie jar");

    let empty = tempfile::tempdir().expect("empty home");
    std::env::set_var("HOME", empty.path());
    shell.invoke_settings_login("google".into());
    assert!(shell.get_settings_account_busy());
    loop_until(|| !shell.get_settings_account_busy());
    assert!(!shell.get_settings_account_connected());
    assert_eq!(
        shell.get_settings_account_error().as_str(),
        ERR_NO_FIREFOX,
        "missing firefox sets errNoFirefox state"
    );

    std::env::set_var("HOME", home.path());
    let outcome = run_login_with(&validator_ok);
    assert!(
        matches!(outcome, LoginOutcome::Connected { .. }),
        "seed a stored login for the boot restore"
    );

    let restored = MainShell::new().expect("restore shell");
    let restored_lang = wire_shell_i18n(&restored, madrigal_i18n::Lang::Es);
    let restored_presenter = Rc::new(RefCell::new(SettingsPresenter::load(
        &settings_path,
        SessionStore::default().ids(),
        &root.path().join("diag-restore"),
    )));
    wire_shell(&restored, restored_presenter.clone(), &restored_lang);
    let (restore_sink, _restore_rx) = std::sync::mpsc::channel::<CatalogEvent>();
    let restore_sink = Arc::new(Mutex::new(restore_sink));
    let restore_delivery: madrigal_app::catalog::Delivery = Arc::new(move |_event| {
        let _guard = restore_sink.lock();
    });
    let restore_wiring = Arc::new(
        wire_catalog_with(
            &restored,
            &Content::default(),
            Box::new(FixtureSource),
            restore_delivery,
        )
        .expect("wire restore catalog"),
    );
    let started = Instant::now();
    let _restore_timer = wire_account_with(
        &restored,
        restored_presenter.clone(),
        Arc::clone(&restore_wiring),
        validator_fail,
    );
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "restore blocked the boot thread"
    );
    restored.show().expect("show restore");

    loop_until(|| restored.get_settings_account_connected());
    assert_eq!(restored.get_settings_account_name().as_str(), "Sample User");
    assert_eq!(
        restored.get_settings_account_email().as_str(),
        "sample@example.com"
    );
    assert!(restored.get_settings_account_error().is_empty());
    assert!(!restored.get_settings_account_busy());
    assert_eq!(
        SessionStore::default().active().channel_id,
        "UC00000000000000000000"
    );
}
