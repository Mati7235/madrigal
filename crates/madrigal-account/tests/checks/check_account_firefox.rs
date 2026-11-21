use std::path::{Path, PathBuf};
use std::sync::Mutex;

use madrigal_account::browser::{records_from_browser, BrowserError};

static ENV: Mutex<()> = Mutex::new(());

const SCHEMA: &str = "CREATE TABLE moz_cookies (id INTEGER PRIMARY KEY, originAttributes TEXT NOT NULL DEFAULT '', name TEXT, value TEXT, host TEXT, path TEXT, expiry INTEGER, lastAccessed INTEGER, creationTime INTEGER, isSecure INTEGER, isHttpOnly INTEGER, inBrowserElement INTEGER DEFAULT 0, sameSite INTEGER DEFAULT 0, rawSameSite INTEGER DEFAULT 0, schemeMap INTEGER DEFAULT 0, isPartitionedAttributeSet INTEGER DEFAULT 0, CONSTRAINT moz_uniqueid UNIQUE (name, host, path, originAttributes))";

fn home() -> (std::sync::MutexGuard<'static, ()>, tempfile::TempDir, tempfile::TempDir) {
    let guard = ENV.lock().expect("env lock");
    let home = tempfile::tempdir().expect("home");
    let config = tempfile::tempdir().expect("config");
    std::env::set_var("HOME", home.path());
    std::env::set_var("XDG_CONFIG_HOME", config.path());
    (guard, home, config)
}

fn insert(conn: &rusqlite::Connection, row: (&str, &str, &str, &str, &str, i64, i64, i64)) {
    conn.execute(
        "INSERT INTO moz_cookies (originAttributes, name, value, host, path, expiry, isSecure, isHttpOnly) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7],
    )
    .expect("insert");
}

fn cookie_db(dir: &Path, wal: bool) -> rusqlite::Connection {
    let path = dir.join("cookies.sqlite");
    let conn = rusqlite::Connection::open(&path).expect("open");
    if wal {
        conn.pragma_update(None, "journal_mode", "WAL")
            .expect("wal mode");
    }
    conn.execute_batch(SCHEMA).expect("schema");
    conn
}

fn write_ini(root: &Path, default: &str) {
    let other = if default == "rel.default-release" {
        "other.default"
    } else {
        "rel.default-release"
    };
    let text = format!(
        "[General]\nStartWithLastProfile=1\nVersion=2\n\n[InstallFIREFOX123]\nDefault={default}\nLocked=1\n\n[Profile0]\nName=default-rel\nIsRelative=1\nPath=rel.default-release\nDefault=1\n\n[Profile1]\nName=other\nIsRelative=1\nPath={other}\n"
    );
    std::fs::write(root.join("profiles.ini"), text).expect("profiles.ini");
}

fn firefox_layout(home: &Path, default: &str) -> PathBuf {
    let root = home.join(".mozilla/firefox");
    let rel = root.join("rel.default-release");
    let other = root.join("other.default");
    std::fs::create_dir_all(&rel).expect("rel dir");
    std::fs::create_dir_all(&other).expect("other dir");
    write_ini(&root, default);
    let conn = cookie_db(&rel, false);
    insert(
        &conn,
        ("", "LOGIN_INFO", "login-default", ".youtube.com", "/", 1_900_000_000, 1, 0),
    );
    insert(
        &conn,
        ("", "__Secure-3PAPISID", "sapisid-ms", ".youtube.com", "/", 1_900_000_000_000, 1, 1),
    );
    insert(
        &conn,
        ("", "PREF", "hl=es", ".youtube.com", "/", 1_900_000_000, 0, 0),
    );
    insert(
        &conn,
        ("", "NID", "google-value", ".google.com", "/", 1_900_000_000, 1, 1),
    );
    insert(
        &conn,
        ("userContextId=2", "LOGIN_INFO", "container-login", ".youtube.com", "/", 1_900_000_000, 1, 0),
    );
    insert(
        &conn,
        ("", "SAPISID", "music-sapisid", ".music.youtube.com", "/", 1_900_000_000, 1, 0),
    );
    let other_conn = cookie_db(&other, false);
    insert(
        &other_conn,
        ("", "LOGIN_INFO", "login-other", ".youtube.com", "/", 1_900_000_000, 1, 0),
    );
    insert(
        &other_conn,
        ("", "SAPISID", "other-sapisid", ".youtube.com", "/", 1_900_000_000, 1, 0),
    );
    drop(conn);
    drop(other_conn);
    rel
}

fn value_of(records: &[madrigal_account::cookies::CookieRecord], name: &str) -> Option<String> {
    records
        .iter()
        .find(|record| record.name == name)
        .map(|record| record.value.clone())
}

#[test]
fn firefox_default_profile_via_install_section() {
    let (_guard, home, _config) = home();
    firefox_layout(home.path(), "rel.default-release");
    let records = records_from_browser("firefox", "").expect("default import");
    assert_eq!(value_of(&records, "LOGIN_INFO").as_deref(), Some("login-default"));
    assert_eq!(
        value_of(&records, "__Secure-3PAPISID").as_deref(),
        Some("sapisid-ms")
    );
    assert_eq!(
        value_of(&records, "SAPISID").as_deref(),
        Some("music-sapisid")
    );
    assert!(records.iter().all(|record| record.domain.contains("youtube")));
    assert!(records.iter().all(|record| record.value != "container-login"));
    let primary = records
        .iter()
        .find(|record| record.name == "__Secure-3PAPISID")
        .expect("primary sapisid");
    assert_eq!(primary.expires, 1_900_000_000);
    assert!(primary.secure && primary.http_only);
    let secondary = records
        .iter()
        .find(|record| record.name == "SAPISID")
        .expect("secondary sapisid");
    assert_eq!(secondary.expires, 1_900_000_000);
}

#[test]
fn firefox_explicit_profile_dir() {
    let (_guard, home, _config) = home();
    firefox_layout(home.path(), "rel.default-release");
    let other = home.path().join(".mozilla/firefox/other.default");
    let records = records_from_browser("firefox", other.to_str().expect("path"))
        .expect("explicit import");
    assert_eq!(value_of(&records, "LOGIN_INFO").as_deref(), Some("login-other"));
}

#[test]
fn firefox_no_login_error() {
    let (_guard, home, _config) = home();
    let root = home.path().join(".mozilla/firefox");
    let rel = root.join("rel.default-release");
    std::fs::create_dir_all(&rel).expect("rel dir");
    write_ini(&root, "rel.default-release");
    let conn = cookie_db(&rel, false);
    insert(&conn, ("", "PREF", "hl=es", ".youtube.com", "/", 1_900_000_000, 0, 0));
    insert(&conn, ("", "NID", "google-value", ".google.com", "/", 1_900_000_000, 1, 1));
    drop(conn);
    let err = records_from_browser("firefox", "").expect_err("no login should fail");
    assert!(matches!(err, BrowserError::NoLogin(_)), "{err:?}");
}

#[test]
fn firefox_not_found_error() {
    let (_guard, _home, _config) = home();
    let err = records_from_browser("firefox", "").expect_err("missing firefox should fail");
    assert!(matches!(err, BrowserError::NotFound(_)), "{err:?}");
    let err = records_from_browser("firefox", "missing-profile").expect_err("missing profile");
    assert!(matches!(err, BrowserError::ProfileNotFound(_)), "{err:?}");
}

#[test]
fn firefox_locked_database_copied() {
    let (_guard, home, _config) = home();
    let root = home.path().join(".mozilla/firefox");
    let rel = root.join("rel.default-release");
    std::fs::create_dir_all(&rel).expect("rel dir");
    write_ini(&root, "rel.default-release");
    let conn = cookie_db(&rel, true);
    insert(
        &conn,
        ("", "LOGIN_INFO", "login-wal", ".youtube.com", "/", 1_900_000_000, 1, 0),
    );
    insert(
        &conn,
        ("", "SAPISID", "wal-sapisid", ".youtube.com", "/", 1_900_000_000, 1, 0),
    );
    conn.execute_batch("BEGIN EXCLUSIVE").expect("lock");
    let records = records_from_browser("firefox", "").expect("wal import under lock");
    assert_eq!(value_of(&records, "LOGIN_INFO").as_deref(), Some("login-wal"));
    conn.execute_batch("ROLLBACK").expect("unlock");
}
