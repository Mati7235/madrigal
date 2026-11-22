use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn accounts_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join("config").join("madrigal").join("accounts")
}

#[test]
fn secrets_use_keyring_and_migrate_plaintext() {
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    env::set_var("MADRIGAL_KEYRING", "0");

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
    assert!(madrigal_secrets::write_private(&dir_link.join("f.txt"), "no").is_err());

    madrigal_secrets::set_secret("cuenta-1", "abc").unwrap();
    assert_eq!(
        madrigal_secrets::get_secret("cuenta-1").unwrap(),
        Some("abc".to_string())
    );
    let accounts = accounts_dir(root.path());
    assert!(!accounts.join("secret_cuenta-1.txt").exists());
    assert_eq!(madrigal_secrets::get_secret("nadie").unwrap(), None);
    madrigal_secrets::delete_secret("cuenta-1").unwrap();
    assert_eq!(madrigal_secrets::get_secret("cuenta-1").unwrap(), None);
    assert!(madrigal_secrets::set_secret("../x", "abc").is_err());

    fs::create_dir_all(&accounts).unwrap();
    let legacy = accounts.join("secret_legacy.txt");
    fs::write(&legacy, "vieja").unwrap();
    assert_eq!(
        madrigal_secrets::get_secret("legacy").unwrap(),
        Some("vieja".to_string())
    );
    assert!(!legacy.exists());
    let bulk = accounts.join("secret_bulk.txt");
    fs::write(&bulk, "masiva").unwrap();
    madrigal_secrets::migrate_legacy_secrets();
    assert!(!bulk.exists());
    assert_eq!(
        madrigal_secrets::get_secret("bulk").unwrap(),
        Some("masiva".to_string())
    );

    env::set_var("MADRIGAL_KEYRING", "fail");
    assert!(matches!(
        madrigal_secrets::set_secret("sin-llavero", "abc"),
        Err(madrigal_secrets::SecretError::NoKeyring)
    ));
    assert!(matches!(
        madrigal_secrets::get_secret("sin-llavero"),
        Err(madrigal_secrets::SecretError::NoKeyring)
    ));
    assert!(matches!(
        madrigal_secrets::delete_secret("sin-llavero"),
        Err(madrigal_secrets::SecretError::NoKeyring)
    ));
    let pending = accounts.join("secret_pendiente.txt");
    fs::write(&pending, "pendiente").unwrap();
    assert!(matches!(
        madrigal_secrets::get_secret("pendiente"),
        Err(madrigal_secrets::SecretError::NoKeyring)
    ));
    assert!(pending.exists());
    assert!(!accounts.join("secret_sin-llavero.txt").exists());

    env::set_var("MADRIGAL_KEYRING", "0");
    assert_eq!(
        madrigal_secrets::get_secret("pendiente").unwrap(),
        Some("pendiente".to_string())
    );
    assert!(!pending.exists());
    let leftovers: Vec<String> = fs::read_dir(&accounts)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("secret_"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    assert!(madrigal_secrets::write_private(&target, "abc").is_ok());
}
