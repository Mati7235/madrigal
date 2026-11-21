use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use madrigal_account::browser::{
    choose_browser, detect_browsers, records_from_browser, BrowserCandidate, BrowserChoice,
    BrowserError,
};

static ENV: Mutex<()> = Mutex::new(());

fn isolate() -> (
    std::sync::MutexGuard<'static, ()>,
    tempfile::TempDir,
    tempfile::TempDir,
) {
    let guard = ENV.lock().expect("env lock");
    let home = tempfile::tempdir().expect("home");
    let config = tempfile::tempdir().expect("config");
    std::env::set_var("HOME", home.path());
    std::env::set_var("XDG_CONFIG_HOME", config.path());
    std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent/wp28");
    (guard, home, config)
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/chromium")
}

fn install_chromium(config: &Path, profile: &str) -> PathBuf {
    let root = config.join("chromium");
    let dir = root.join(profile);
    std::fs::create_dir_all(&dir).expect("profile dir");
    std::fs::copy(fixture().join("Default/Cookies"), dir.join("Cookies")).expect("cookies");
    std::fs::create_dir_all(&root).expect("chromium root");
    std::fs::write(
        root.join("Local State"),
        format!(r#"{{"profile":{{"last_used":"{profile}"}}}}"#),
    )
    .expect("local state");
    dir
}

fn value_of(records: &[madrigal_account::cookies::CookieRecord], name: &str) -> Option<String> {
    records
        .iter()
        .find(|record| record.name == name)
        .map(|record| record.value.clone())
}

#[test]
fn chromium_default_profile_import() {
    let (_guard, _home, config) = isolate();
    install_chromium(config.path(), "Default");
    let records = records_from_browser("chromium", "").expect("default import");
    assert_eq!(
        value_of(&records, "LOGIN_INFO").as_deref(),
        Some("dummy-login")
    );
    assert_eq!(
        value_of(&records, "__Secure-3PAPISID").as_deref(),
        Some("dummy-sapisid")
    );
    assert_eq!(value_of(&records, "PREF").as_deref(), Some("hl=es"));
    assert!(records.iter().all(|record| record.domain == ".youtube.com"));
}

#[test]
fn chromium_last_used_profile_wins() {
    let (_guard, _home, config) = isolate();
    let root = config.path().join("chromium");
    install_chromium(config.path(), "Profile 2");
    let default = root.join("Default");
    std::fs::create_dir_all(default.join("Network")).expect("network");
    std::fs::copy(
        fixture().join("Default/Cookies"),
        default.join("Network/Cookies"),
    )
    .expect("copy");
    let conn = rusqlite::Connection::open(default.join("Network/Cookies")).expect("open");
    conn.execute(
        "UPDATE cookies SET value = 'default-login', encrypted_value = x'' WHERE name = 'LOGIN_INFO'",
        [],
    )
    .expect("update");
    drop(conn);
    let records = records_from_browser("chromium", "").expect("last used import");
    assert_eq!(
        value_of(&records, "LOGIN_INFO").as_deref(),
        Some("dummy-login")
    );
}

#[test]
fn chromium_explicit_profile_and_missing() {
    let (_guard, _home, config) = isolate();
    let dir = install_chromium(config.path(), "Profile 1");
    let records = records_from_browser("chromium", dir.to_str().expect("path")).expect("explicit");
    assert_eq!(
        value_of(&records, "LOGIN_INFO").as_deref(),
        Some("dummy-login")
    );

    let empty = tempfile::tempdir().expect("empty");
    std::env::set_var("HOME", empty.path());
    std::env::set_var("XDG_CONFIG_HOME", empty.path().join("config"));
    let err = records_from_browser("chromium", "").expect_err("missing chromium");
    assert!(matches!(err, BrowserError::NotFound(_)), "{err:?}");
}

#[test]
fn chromium_locked_database_copied() {
    let (_guard, _home, config) = isolate();
    let dir = install_chromium(config.path(), "Default");
    let conn = rusqlite::Connection::open(dir.join("Cookies")).expect("open");
    conn.execute_batch("BEGIN EXCLUSIVE").expect("lock");
    let records = records_from_browser("chromium", "").expect("locked import");
    assert_eq!(
        value_of(&records, "LOGIN_INFO").as_deref(),
        Some("dummy-login")
    );
    conn.execute_batch("ROLLBACK").expect("unlock");
}

#[test]
fn browser_chooser_logic() {
    let candidate = |key: &str, name: &str| BrowserCandidate {
        key: key.to_owned(),
        name: name.to_owned(),
        profile: "/tmp/profile".to_owned(),
    };
    assert_eq!(choose_browser(Vec::new()), BrowserChoice::None);
    let one = candidate("chromium", "Chromium");
    assert_eq!(
        choose_browser(vec![one.clone()]),
        BrowserChoice::Single(one.clone())
    );
    let two = candidate("firefox", "Firefox");
    assert_eq!(
        choose_browser(vec![one.clone(), two.clone()]),
        BrowserChoice::Multiple(vec![one, two])
    );
}

#[test]
fn detection_lists_both_families() {
    let (_guard, _home, config) = isolate();
    install_chromium(config.path(), "Default");
    let home = tempfile::tempdir().expect("home");
    std::env::set_var("HOME", home.path());
    let root = home.path().join(".mozilla/firefox");
    let profile = root.join("rel.default-release");
    std::fs::create_dir_all(&profile).expect("profile");
    std::fs::write(
        root.join("profiles.ini"),
        "[Profile0]\nName=default\nIsRelative=1\nPath=rel.default-release\nDefault=1\n",
    )
    .expect("ini");
    let conn = rusqlite::Connection::open(profile.join("cookies.sqlite")).expect("open");
    conn.execute_batch(
        "CREATE TABLE moz_cookies (id INTEGER PRIMARY KEY, originAttributes TEXT NOT NULL DEFAULT '', name TEXT, value TEXT, host TEXT, path TEXT, expiry INTEGER, isAccessed INTEGER, isSecure INTEGER, isHttpOnly INTEGER); INSERT INTO moz_cookies (originAttributes, name, value, host, path, expiry, isSecure, isHttpOnly) VALUES ('', 'LOGIN_INFO', 'ff-login', '.youtube.com', '/', 1900000000, 1, 0), ('', 'SAPISID', 'ff-sapisid', '.youtube.com', '/', 1900000000, 1, 0);",
    )
    .expect("firefox db");
    drop(conn);
    let found = detect_browsers();
    let names: Vec<&str> = found.iter().map(|candidate| candidate.label()).collect();
    assert_eq!(names, ["Firefox", "Chromium"]);
    assert!(matches!(choose_browser(found), BrowserChoice::Multiple(_)));
}

#[test]
fn headless_chromium_database_imports() {
    let (_guard, _home, _config) = isolate();
    if Command::new("chromium").arg("--version").output().is_err()
        || Command::new("openssl").arg("version").output().is_err()
    {
        return;
    }
    let work = tempfile::tempdir().expect("work");
    let generated = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-keyout",
            "key.pem",
            "-out",
            "cert.pem",
            "-days",
            "1",
            "-nodes",
            "-subj",
            "/CN=music.youtube.com",
        ])
        .current_dir(work.path())
        .output()
        .expect("openssl");
    assert!(generated.status.success());
    std::fs::write(
        work.path().join("index.html"),
        "<html><body><script>document.cookie=\"LOGIN_INFO=headless-login; domain=.youtube.com; path=/; max-age=31536000; secure\";document.cookie=\"SAPISID=headless-sapisid; domain=.youtube.com; path=/; max-age=31536000; secure\";</script></body></html>",
    )
    .expect("page");
    let port = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("free port")
        .local_addr()
        .expect("local addr")
        .port();
    let mut server = Command::new("openssl")
        .args([
            "s_server",
            "-accept",
            &format!("127.0.0.1:{port}"),
            "-cert",
            "cert.pem",
            "-key",
            "key.pem",
            "-WWW",
            "-quiet",
        ])
        .current_dir(work.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("server");
    let started = Instant::now();
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        if started.elapsed() > Duration::from_secs(5) {
            let _ = server.kill();
            let _ = server.wait();
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let profile = work.path().join("profile");
    let mut chromium = Command::new("chromium")
        .args([
            "--headless=new".to_owned(),
            "--no-sandbox".to_owned(),
            "--disable-gpu".to_owned(),
            "--no-first-run".to_owned(),
            "--disable-dev-shm-usage".to_owned(),
            "--password-store=basic".to_owned(),
            "--ignore-certificate-errors".to_owned(),
            "--host-resolver-rules=MAP music.youtube.com 127.0.0.1".to_owned(),
            format!("--user-data-dir={}", profile.display()),
            format!("https://music.youtube.com:{port}/index.html"),
            "--dump-dom".to_owned(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("chromium");
    let started = Instant::now();
    loop {
        match chromium.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < Duration::from_secs(40) => {
                std::thread::sleep(Duration::from_millis(100));
            }
            _ => {
                let _ = chromium.kill();
                let _ = chromium.wait();
                let _ = server.kill();
                let _ = server.wait();
                return;
            }
        }
    }
    let _ = server.kill();
    let _ = server.wait();
    let dir = profile.join("Default");
    if !dir.join("Cookies").is_file() && !dir.join("Network/Cookies").is_file() {
        return;
    }
    let records = records_from_browser("chromium", dir.to_str().expect("profile path"))
        .expect("headless import");
    assert_eq!(
        value_of(&records, "LOGIN_INFO").as_deref(),
        Some("headless-login")
    );
    assert_eq!(
        value_of(&records, "SAPISID").as_deref(),
        Some("headless-sapisid")
    );
}
