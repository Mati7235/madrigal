use std::sync::Mutex;

use madrigal_account::browser::records_from_browser;
use madrigal_account::session::Session;
use madrigal_account::store::SessionStore;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn isolate() -> (std::sync::MutexGuard<'static, ()>, tempfile::TempDir) {
    let guard = ENV_LOCK.lock().expect("env lock");
    let root = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("home");
    std::env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    std::env::set_var("MADRIGAL_KEYRING", "0");
    std::env::set_var("HOME", home.path());
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("XDG_DATA_HOME");
    std::env::remove_var("XDG_CACHE_HOME");
    for id in [
        "default",
        "acc2",
        "work",
        "legacy",
        "pendiente",
        "sin-llavero",
        "spotify",
        "tidal",
    ] {
        let _ = madrigal_secrets::delete_secret(id);
    }
    (guard, root)
}

fn named_session(cookie: &str, name: &str) -> Session {
    Session {
        cookie: cookie.to_owned(),
        account_name: name.to_owned(),
        ..Session::default()
    }
}

fn accounts_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join("config").join("madrigal").join("accounts")
}

#[test]
fn profiles_coexist_isolated() {
    let (_guard, root) = isolate();
    let store = SessionStore::default();
    store.create("default").expect("create default");
    store.create("acc2").expect("create acc2");

    let first = named_session("LOGIN_INFO=abc:def; __Secure-3PAPISID=primero", "Primera");
    let second = named_session("LOGIN_INFO=z; __Secure-3PAPISID=segundo", "Segunda");
    store.save(&first, Some("default")).expect("save default");
    store.save(&second, Some("acc2")).expect("save acc2");

    let mut ids = store.ids();
    ids.sort();
    assert_eq!(ids, vec!["acc2".to_owned(), "default".to_owned()]);
    assert_eq!(store.active_id(), "default");

    let back_first = store.load("default");
    let back_second = store.load("acc2");
    assert_eq!(back_first.account_name, "Primera");
    assert_eq!(back_second.account_name, "Segunda");
    assert_eq!(back_first.cookie, first.cookie);
    assert_eq!(back_second.cookie, second.cookie);

    let stored = std::fs::read_to_string(store.account_path("default").expect("account path"))
        .expect("read account");
    assert!(!stored.contains("primero"));
    assert!(!stored.contains("\"cookie\""));
    assert!(stored.contains("\"secret_backend\": \"secret\""));
    assert_eq!(
        madrigal_secrets::get_secret("default").expect("read secret"),
        Some(first.cookie.clone())
    );
    assert_eq!(
        madrigal_secrets::get_secret("acc2").expect("read secret"),
        Some(second.cookie.clone())
    );
    assert!(!accounts_dir(root.path())
        .join("secret_default.txt")
        .exists());
    assert!(!accounts_dir(root.path()).join("secret_acc2.txt").exists());
    assert!(back_first.logged_in());
    assert!(back_first.catalog_ready());
    assert_eq!(back_first.cookie_count(), 2);
}

#[test]
fn channel_identity_survives_reload() {
    let (_guard, _root) = isolate();
    let store = SessionStore::default();
    let session = Session {
        cookie: "LOGIN_INFO=a; __Secure-3PAPISID=b".to_owned(),
        account_name: "Cuenta".to_owned(),
        account_email: "cuenta@example.com".to_owned(),
        channel_handle: "@cuenta".to_owned(),
        channel_id: "UC00000000000000000000".to_owned(),
        avatar_url: "https://example.com/avatar.png".to_owned(),
        ..Session::default()
    };
    store.save(&session, Some("default")).expect("save");
    let back = store.load("default");
    assert_eq!(back.account_name, "Cuenta");
    assert_eq!(back.account_email, "cuenta@example.com");
    assert_eq!(back.channel_handle, "@cuenta");
    assert_eq!(back.channel_id, "UC00000000000000000000");
    assert_eq!(back.avatar_url, "https://example.com/avatar.png");
}

#[test]
fn active_pointer_flips_settings_only() {
    let (_guard, _root) = isolate();
    let store = SessionStore::default();
    store.create("default").expect("create default");
    store.create("work").expect("create work");
    store
        .save(
            &named_session("LOGIN_INFO=a; __Secure-3PAPISID=w", "Trabajo"),
            Some("work"),
        )
        .expect("save work");

    let mut settings = madrigal_settings::Settings::load();
    settings.player.volume = 42;
    settings.save().expect("save settings");
    let before = madrigal_settings::Settings::load();

    let active = store.activate("work").expect("activate");
    assert_eq!(store.active_id(), "work");
    assert_eq!(active.account_name, "Trabajo");
    assert_eq!(store.active().account_name, "Trabajo");

    let after = madrigal_settings::Settings::load();
    assert_eq!(after.account.active_account_id, "work");
    assert_eq!(after.player.volume, before.player.volume);
    let mut normalized = after.clone();
    normalized.account.active_account_id = before.account.active_account_id.clone();
    assert_eq!(normalized, before);

    store.activate("default").expect("back to default");
    assert_eq!(store.active_id(), "default");
}

#[test]
fn delete_removes_files() {
    let (_guard, _root) = isolate();
    let store = SessionStore::default();
    store.create("acc2").expect("create acc2");
    store
        .save(
            &named_session("LOGIN_INFO=z; __Secure-3PAPISID=segundo", "Segunda"),
            Some("acc2"),
        )
        .expect("save acc2");
    let account_path = store.account_path("acc2").expect("account path");
    assert!(account_path.exists());

    store.activate("acc2").expect("activate acc2");
    store.remove("acc2").expect("remove acc2");
    assert!(!store.ids().contains(&"acc2".to_owned()));
    assert!(!account_path.exists());
    assert!(madrigal_secrets::get_secret("acc2")
        .expect("read secret")
        .is_none());
    assert_eq!(store.active_id(), "default");

    store.remove("default").expect("default survives");
    assert!(store.ids().contains(&"default".to_owned()));
    assert!(store.remove("../escape").is_ok());
}

#[test]
fn invalid_ids_rejected() {
    let (_guard, _root) = isolate();
    let store = SessionStore::default();
    assert!(store.create("../escape").is_err());
    assert!(store.account_path("a/b").is_err());
    assert!(store.activate("").is_err());
    assert!(!store.load("../escape").logged_in());
}

#[test]
fn session_metadata_private() {
    let (_guard, _root) = isolate();
    let store = SessionStore::default();
    store.create("default").expect("create default");
    let path = store
        .save(
            &named_session("LOGIN_INFO=s; __Secure-3PAPISID=s", "Sesión"),
            Some("default"),
        )
        .expect("save");
    let account = store.account_path(&path).expect("account path");
    #[cfg(unix)]
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(&account).expect("meta").permissions()
        ) & 0o777,
        0o600
    );
}

#[test]
fn symlink_session_rejected() {
    let (_guard, _root) = isolate();
    let dir = tempfile::tempdir().expect("tempdir");
    let store = SessionStore::with_dir(dir.path().to_path_buf());
    let victim = dir.path().join("victim.json");
    std::fs::write(&victim, "intacto").expect("victim");
    let link = store.account_path("default").expect("account path");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&victim, &link).expect("symlink");
    #[cfg(unix)]
    assert!(store
        .save(&named_session("LOGIN_INFO=a", "A"), Some("default"))
        .is_err());
    assert_eq!(std::fs::read_to_string(&victim).expect("read"), "intacto");
}

#[test]
fn legacy_cookie_migrates_from_session_file() {
    let (_guard, root) = isolate();
    let store = SessionStore::default();
    let path = store.account_path("default").expect("account path");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let cookie = "LOGIN_INFO=legacy:1; __Secure-3PAPISID=legacy";
    let text = format!(
        "{{\n  \"cookie\": \"{}\",\n  \"account_name\": \"Vieja\",\n  \"source_browser\": \"firefox\"\n}}\n",
        cookie
    );
    std::fs::write(&path, text).unwrap();
    store.migrate();

    let session = store.load("default");
    assert_eq!(session.cookie, cookie);
    assert_eq!(session.account_name, "Vieja");
    let stored = std::fs::read_to_string(&path).expect("read");
    assert!(!stored.contains("legacy"));
    assert!(!stored.contains("\"cookie\""));
    assert!(stored.contains("\"secret_backend\": \"secret\""));
    assert_eq!(
        madrigal_secrets::get_secret("default").expect("read secret"),
        Some(cookie.to_owned())
    );
    assert!(!accounts_dir(root.path())
        .join("secret_default.txt")
        .exists());
}

#[test]
fn legacy_secret_file_migrates_on_load() {
    let (_guard, root) = isolate();
    let store = SessionStore::default();
    let path = store.account_path("default").expect("account path");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "{\n  \"account_name\": \"Vieja\",\n  \"secret_backend\": \"secret\"\n}\n",
    )
    .unwrap();
    let legacy = accounts_dir(root.path()).join("secret_default.txt");
    let cookie = "LOGIN_INFO=fallback:1; __Secure-3PAPISID=fallback";
    std::fs::write(&legacy, cookie).unwrap();
    let service = accounts_dir(root.path()).join("secret_spotify.txt");
    std::fs::write(&service, "spotify-token").unwrap();
    store.migrate();

    let session = store.load("default");
    assert_eq!(session.cookie, cookie);
    assert!(!legacy.exists());
    assert!(!service.exists());
    assert_eq!(
        madrigal_secrets::get_secret("default").expect("read secret"),
        Some(cookie.to_owned())
    );
    assert_eq!(
        madrigal_secrets::get_secret("spotify").expect("read secret"),
        Some("spotify-token".to_owned())
    );
}

#[test]
fn missing_keyring_keeps_plaintext_until_migration() {
    let (_guard, root) = isolate();
    let store = SessionStore::default();
    let path = store.account_path("default").expect("account path");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let cookie = "LOGIN_INFO=blocked:1; __Secure-3PAPISID=blocked";
    let text = format!(
        "{{\n  \"cookie\": \"{}\",\n  \"account_name\": \"Vieja\"\n}}\n",
        cookie
    );
    std::fs::write(&path, &text).unwrap();

    std::env::set_var("MADRIGAL_KEYRING", "fail");
    store.migrate();
    let session = store.load("default");
    assert!(!session.logged_in());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    assert!(madrigal_account::SecretError::NoKeyring.no_keyring());

    std::env::set_var("MADRIGAL_KEYRING", "0");
    store.migrate();
    let migrated = store.load("default");
    assert_eq!(migrated.cookie, cookie);
    let stored = std::fs::read_to_string(&path).expect("read");
    assert!(!stored.contains("blocked"));
    assert!(!accounts_dir(root.path())
        .join("secret_default.txt")
        .exists());
}

#[test]
fn browser_seam_empty() {
    let (_guard, _root) = isolate();
    for browser in [
        "firefox",
        "librewolf",
        "zen",
        "floorp",
        "waterfox",
        "chrome",
        "chromium",
        "brave",
        "edge",
        "vivaldi",
        "opera",
    ] {
        assert!(records_from_browser(browser, "").is_err(), "{browser} seam");
    }
    assert!(records_from_browser("inventado", "").is_err());
}
