use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn secrets_use_private_files_and_reject_symlinks() {
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    env::set_var("MADRIGAL_KEYRING", "0");
    assert!(!madrigal_secrets::keyring_enabled());

    let dir = root.path().join("secrets");
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("token.txt");
    madrigal_secrets::write_private(&target, "s3cr3t").unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "s3cr3t");
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let real = dir.join("real.txt");
    fs::write(&real, "x").unwrap();
    let link = dir.join("link.txt");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert!(madrigal_secrets::write_private(&link, "no").is_err());
    let dir_link = dir.join("parent-link");
    std::os::unix::fs::symlink(&dir, &dir_link).unwrap();
    assert!(
        madrigal_secrets::write_private(&dir_link.join("f.txt"), "no").is_err()
    );

    madrigal_secrets::set_secret("cuenta-1", "abc").unwrap();
    let fallback = madrigal_secrets::fallback_path("cuenta-1").unwrap();
    assert!(fallback.starts_with(root.path()));
    assert_eq!(
        fs::metadata(&fallback).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        madrigal_secrets::get_secret("cuenta-1").unwrap(),
        Some("abc".to_string())
    );
    assert_eq!(madrigal_secrets::get_secret("nadie").unwrap(), None);
    madrigal_secrets::delete_secret("cuenta-1").unwrap();
    assert_eq!(madrigal_secrets::get_secret("cuenta-1").unwrap(), None);
    assert!(madrigal_secrets::set_secret("../x", "abc").is_err());
}
