use std::env;
use std::path::Path;
use std::process::{Command, Stdio};

const CHILD_ENV: &str = "MADRIGAL_KEYRING_LIVE_CHILD";
const ACCOUNT_ENV: &str = "MADRIGAL_KEYRING_LIVE_ACCOUNT";
const VALUE: &str = "valor-persistente";

fn account() -> String {
    env::var(ACCOUNT_ENV).expect("live account")
}

#[cfg(unix)]
fn private_dir(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).expect("mode");
}

fn available(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[test]
fn secret_service_roundtrip() {
    if env::var_os(CHILD_ENV).is_some() {
        return;
    }
    if !available("dbus-run-session") || !available("gnome-keyring-daemon") {
        eprintln!("keyring_live: dbus-run-session or gnome-keyring-daemon missing");
        return;
    }
    let exe = env::current_exe().expect("test binary");
    let root = tempfile::tempdir().expect("tempdir");
    let home = root.path().join("home");
    let runtime = root.path().join("run");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::create_dir_all(&runtime).expect("runtime");
    private_dir(&runtime);
    let account = format!("live-{}", std::process::id());
    let script = "printf %s test | gnome-keyring-daemon --unlock --components=secrets >/dev/null 2>&1 || exit 3\n\
                  sleep 0.2\n\
                  \"$MADRIGAL_TEST_EXE\" --exact secret_service_store --nocapture || exit 4\n\
                  \"$MADRIGAL_TEST_EXE\" --exact secret_service_read_delete --nocapture || exit 5\n";
    let status = Command::new("dbus-run-session")
        .args(["--", "sh", "-c", script])
        .env("HOME", &home)
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("MADRIGAL_PROFILE_ROOT", root.path().join("profile"))
        .env("MADRIGAL_KEYRING", "1")
        .env(CHILD_ENV, "1")
        .env(ACCOUNT_ENV, &account)
        .env("MADRIGAL_TEST_EXE", &exe)
        .status()
        .expect("dbus-run-session");
    assert!(
        status.success(),
        "secret service roundtrip failed: {status}"
    );
}

#[test]
fn secret_service_store() {
    if env::var_os(CHILD_ENV).is_none() {
        return;
    }
    madrigal_secrets::set_secret(&account(), VALUE).expect("store");
    assert_eq!(
        madrigal_secrets::get_secret(&account()).expect("read"),
        Some(VALUE.to_owned())
    );
}

#[test]
fn secret_service_read_delete() {
    if env::var_os(CHILD_ENV).is_none() {
        return;
    }
    assert_eq!(
        madrigal_secrets::get_secret(&account()).expect("read"),
        Some(VALUE.to_owned())
    );
    madrigal_secrets::delete_secret(&account()).expect("delete");
    assert_eq!(
        madrigal_secrets::get_secret(&account()).expect("read"),
        None
    );
}
