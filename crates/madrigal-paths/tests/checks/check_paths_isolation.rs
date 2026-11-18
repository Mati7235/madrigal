use std::env;
use std::fs;

#[test]
fn paths_stay_inside_profile_root() {
    let root = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    env::set_var("HOME", home.path());
    env::set_var("XDG_DATA_HOME", home.path().join("xdg-data"));
    env::set_var("XDG_CONFIG_HOME", home.path().join("xdg-config"));
    env::set_var("XDG_CACHE_HOME", home.path().join("xdg-cache"));

    let data = madrigal_paths::data_dir();
    let config = madrigal_paths::config_dir();
    let cache = madrigal_paths::cache_dir();
    for dir in [&data, &config, &cache] {
        assert!(
            dir.starts_with(root.path()),
            "fuera del override: {}",
            dir.display()
        );
    }
    assert!(
        madrigal_paths::profile_account_path("default")
            .unwrap()
            .starts_with(root.path())
    );
    assert!(
        madrigal_paths::profile_cookies_path("default")
            .unwrap()
            .starts_with(root.path())
    );

    madrigal_paths::ensure_dirs().expect("ensure_dirs");
    assert!(data.is_dir() && config.is_dir() && cache.is_dir());

    let leftovers: Vec<_> = fs::read_dir(home.path()).unwrap().collect();
    assert!(leftovers.is_empty(), "se escribió fuera del override");
    assert!(!madrigal_paths::valid_profile_id("../escape"));
    assert!(madrigal_paths::profile_account_path("../x").is_err());
}
