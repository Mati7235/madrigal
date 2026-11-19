use std::fs;
use std::time::Instant;

const WALL_LIMIT_MS: u128 = 2000;
const RSS_DELTA_LIMIT_KB: u64 = 150 * 1024;

fn vm_hwm_kb() -> u64 {
    let status = fs::read_to_string("/proc/self/status").expect("leer /proc/self/status");
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let value = rest.split_whitespace().next().expect("valor VmHWM");
            return value.parse().expect("VmHWM numérico");
        }
    }
    panic!("VmHWM ausente en /proc/self/status");
}

#[test]
fn app_startup() {
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, root.path());
    std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
    std::env::set_var("XDG_CACHE_HOME", root.path().join("cache"));

    let before = vm_hwm_kb();
    let started = Instant::now();
    let boot = madrigal_app::boot_headless(root.path()).expect("boot_headless");
    let wall_ms = started.elapsed().as_millis();
    let delta = vm_hwm_kb().saturating_sub(before);

    assert!(boot.startup_ms <= wall_ms as u64);
    assert!(
        wall_ms < WALL_LIMIT_MS,
        "boot_headless tardó {wall_ms} ms"
    );
    assert!(delta < RSS_DELTA_LIMIT_KB, "rss_delta {delta} kB");
    assert!(boot.store.queue().is_empty());
    assert!(madrigal_paths::db_path().is_file());
    assert!(madrigal_paths::settings_path().starts_with(root.path()));

    println!("OK boot_headless {wall_ms}");
    println!("OK rss_delta {delta}");
}
