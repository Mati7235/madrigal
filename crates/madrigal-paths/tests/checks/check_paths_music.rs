use std::env;
use std::fs;

#[test]
fn music_dir_follows_xdg_user_dirs() {
    let home = tempfile::tempdir().expect("tempdir");
    let config = home.path().join("xdg-config");
    env::remove_var("MADRIGAL_PROFILE_ROOT");
    env::remove_var("XDG_MUSIC_DIR");
    env::set_var("HOME", home.path());
    env::set_var("XDG_CONFIG_HOME", &config);

    assert_eq!(madrigal_paths::music_dir(), home.path().join("Music"));

    fs::create_dir_all(&config).expect("config dir");
    fs::write(
        config.join("user-dirs.dirs"),
        "XDG_MUSIC_DIR=\"$HOME/Música\"\n",
    )
    .expect("user dirs");
    assert_eq!(madrigal_paths::music_dir(), home.path().join("Música"));

    let explicit = home.path().join("Música elegida");
    env::set_var("XDG_MUSIC_DIR", &explicit);
    assert_eq!(madrigal_paths::music_dir(), explicit);

    fs::remove_file(config.join("user-dirs.dirs")).expect("remove user dirs");
    env::remove_var("XDG_MUSIC_DIR");
    assert_eq!(madrigal_paths::music_dir(), home.path().join("Music"));
}
