use std::env;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use sha2::{Digest, Sha256};

use crate::browser::BrowserError;
use crate::cookies::{
    has_login_cookie, has_primary_sapisid, is_youtube_cookie, youtube_records, CookieRecord,
};
use crate::secrets;

const SALT: &[u8] = b"saltysalt";
const V10_PASSWORD: &[u8] = b"peanuts";
const IV: [u8; 16] = [b' '; 16];
const CHROME_EPOCH_OFFSET: i64 = 11_644_473_600;
const HASH_VERSION: i64 = 24;

pub(crate) struct ChromiumSpec {
    pub key: &'static str,
    pub app_names: &'static [&'static str],
    pub labels: &'static [&'static str],
    pub product: &'static str,
}

const CHROMIUM_SPECS: [ChromiumSpec; 6] = [
    ChromiumSpec {
        key: "chrome",
        app_names: &["chrome"],
        labels: &["Chrome Safe Storage"],
        product: "Google Chrome",
    },
    ChromiumSpec {
        key: "chromium",
        app_names: &["chromium"],
        labels: &["Chromium Safe Storage"],
        product: "Chromium",
    },
    ChromiumSpec {
        key: "brave",
        app_names: &["brave"],
        labels: &["Brave Safe Storage"],
        product: "Brave",
    },
    ChromiumSpec {
        key: "edge",
        app_names: &["microsoft-edge", "msedge"],
        labels: &["Microsoft Edge Safe Storage"],
        product: "Microsoft Edge",
    },
    ChromiumSpec {
        key: "vivaldi",
        app_names: &["vivaldi", "chrome"],
        labels: &["Vivaldi Safe Storage", "Chrome Safe Storage"],
        product: "Vivaldi",
    },
    ChromiumSpec {
        key: "opera",
        app_names: &["opera", "chromium"],
        labels: &["Opera Safe Storage", "Chromium Safe Storage"],
        product: "Opera",
    },
];

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

fn spec(key: &str) -> Option<&'static ChromiumSpec> {
    CHROMIUM_SPECS.iter().find(|spec| spec.key == key)
}

pub(crate) fn is_chromium(key: &str) -> bool {
    spec(key).is_some()
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|dir| !dir.as_os_str().is_empty())
}

fn config_dir() -> Option<PathBuf> {
    let home = home_dir()?;
    Some(
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|dir| !dir.as_os_str().is_empty())
            .unwrap_or_else(|| home.join(".config")),
    )
}

fn snap_dirs(home: &Path, app: &str, suffix: &str) -> Vec<PathBuf> {
    vec![
        home.join(format!("snap/{app}/common/{suffix}")),
        home.join(format!("snap/{app}/current/{suffix}")),
    ]
}

pub(crate) fn roots(key: &str) -> Vec<PathBuf> {
    let (Some(home), Some(config)) = (home_dir(), config_dir()) else {
        return Vec::new();
    };
    match key {
        "chrome" => {
            let mut roots = vec![
                config.join("google-chrome"),
                config.join("google-chrome-beta"),
                config.join("google-chrome-unstable"),
                home.join(".var/app/com.google.Chrome/config/google-chrome"),
            ];
            roots.extend(snap_dirs(&home, "google-chrome", ".config/google-chrome"));
            roots
        }
        "chromium" => {
            let mut roots = vec![
                config.join("chromium"),
                home.join(".var/app/org.chromium.Chromium/config/chromium"),
            ];
            roots.extend(snap_dirs(&home, "chromium", "chromium"));
            roots
        }
        "brave" => {
            let relative = "BraveSoftware/Brave-Browser";
            let mut roots = vec![
                config.join(relative),
                home.join(format!(".var/app/com.brave.Browser/config/{relative}")),
            ];
            roots.extend(snap_dirs(
                &home,
                "brave",
                ".config/BraveSoftware/Brave-Browser",
            ));
            roots
        }
        "edge" => {
            let mut roots = vec![
                config.join("microsoft-edge"),
                home.join(".var/app/com.microsoft.Edge/config/microsoft-edge"),
            ];
            roots.extend(snap_dirs(&home, "microsoft-edge", ".config/microsoft-edge"));
            roots
        }
        "vivaldi" => {
            let mut roots = vec![
                config.join("vivaldi"),
                config.join("vivaldi-snapshot"),
                home.join(".var/app/com.vivaldi.Vivaldi/config/vivaldi"),
            ];
            roots.extend(snap_dirs(&home, "vivaldi", ".config/vivaldi"));
            roots
        }
        "opera" => {
            let mut roots = vec![
                config.join("opera"),
                config.join("opera-gx"),
                home.join(".var/app/com.opera.Opera/config/opera"),
            ];
            roots.extend(snap_dirs(&home, "opera", ".config/opera"));
            roots
        }
        _ => Vec::new(),
    }
}

fn cookie_db(profile: &Path) -> Option<PathBuf> {
    let network = profile.join("Network/Cookies");
    if network.is_file() {
        return Some(network);
    }
    let legacy = profile.join("Cookies");
    legacy.is_file().then_some(legacy)
}

fn local_state_profiles(root: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(text) = std::fs::read_to_string(root.join("Local State")) {
        if let Ok(state) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(last) = state["profile"]["last_used"].as_str() {
                if !last.is_empty() {
                    names.push(last.to_owned());
                }
            }
        }
    }
    names.push("Default".to_owned());
    names
}

fn numbered_profiles(root: &Path) -> Vec<String> {
    let mut numbered: Vec<(u32, String)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(number) = name.strip_prefix("Profile ") else {
            continue;
        };
        let Ok(number) = number.parse::<u32>() else {
            continue;
        };
        numbered.push((number, name));
    }
    numbered.sort_by_key(|(number, _)| *number);
    numbered.into_iter().map(|(_, name)| name).collect()
}

pub(crate) fn profiles(root: &Path) -> Vec<PathBuf> {
    let mut names = local_state_profiles(root);
    names.extend(numbered_profiles(root));
    let mut dirs: Vec<PathBuf> = Vec::new();
    for name in names {
        let dir = root.join(&name);
        if cookie_db(&dir).is_some() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

pub(crate) fn default_profile(root: &Path) -> Option<PathBuf> {
    profiles(root).into_iter().next()
}

pub(crate) fn resolve_profile(key: &str, profile: &str) -> Result<PathBuf, BrowserError> {
    let direct = Path::new(profile);
    if direct.is_dir() && cookie_db(direct).is_some() {
        return Ok(direct.to_path_buf());
    }
    for root in roots(key) {
        let candidate = root.join(direct);
        if candidate.is_dir() && cookie_db(&candidate).is_some() {
            return Ok(candidate);
        }
    }
    Err(BrowserError::ProfileNotFound(profile.to_owned()))
}

fn temp_copy(db: &Path) -> Result<PathBuf, BrowserError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|now| now.as_nanos())
        .unwrap_or(0);
    let target = env::temp_dir().join(format!("madrigal-chromium-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&target).map_err(|err| BrowserError::ReadFailed(err.to_string()))?;
    let name = db
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Cookies")
        .to_owned();
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let from = db.with_file_name(format!("{name}{suffix}"));
        if from.is_file() {
            let to = target.join(format!("{name}{suffix}"));
            std::fs::copy(&from, &to).map_err(|err| BrowserError::ReadFailed(err.to_string()))?;
        }
    }
    let copied = target.join(&name);
    if !copied.is_file() {
        let _ = std::fs::remove_dir_all(&target);
        return Err(BrowserError::ProfileNotFound("Cookies missing".to_owned()));
    }
    Ok(copied)
}

pub(crate) fn derive_key(password: &[u8]) -> [u8; 16] {
    let mut key = [0u8; 16];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password, SALT, 1, &mut key);
    key
}

fn strip_host_hash<'a>(plain: &'a [u8], host: &str) -> Option<&'a [u8]> {
    if plain.len() < 32 {
        return None;
    }
    let hash = Sha256::digest(host.as_bytes());
    (plain[..32] == hash[..]).then_some(&plain[32..])
}

fn decrypt_aes(data: &[u8], password: &[u8]) -> Option<Vec<u8>> {
    let key = derive_key(password);
    let buffer = data.to_vec();
    Aes128CbcDec::new_from_slices(&key, &IV)
        .ok()?
        .decrypt_padded_vec_mut::<Pkcs7>(&buffer)
        .ok()
}

fn decrypt_cookie(
    encrypted: &[u8],
    host: &str,
    version: i64,
    password: Option<&str>,
) -> Option<String> {
    let plain = if let Some(data) = encrypted.strip_prefix(b"v10") {
        decrypt_aes(data, V10_PASSWORD)?
    } else if let Some(data) = encrypted.strip_prefix(b"v11") {
        decrypt_aes(data, password?.as_bytes())?
    } else if encrypted.starts_with(b"v") {
        return None;
    } else {
        encrypted.to_vec()
    };
    let plain = if version >= HASH_VERSION {
        strip_host_hash(&plain, host)?
    } else {
        plain.as_slice()
    };
    String::from_utf8(plain.to_vec()).ok()
}

fn is_google_cookie(domain: &str) -> bool {
    let normalized = domain.strip_prefix('.').unwrap_or(domain);
    normalized == "google.com" || normalized.ends_with(".google.com")
}

struct Row {
    host: String,
    name: String,
    value: String,
    encrypted: Vec<u8>,
    path: String,
    expires: i64,
    has_expires: bool,
    secure: bool,
    http_only: bool,
}

fn rows(db_path: &Path) -> Result<(i64, Vec<Row>), BrowserError> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|err| BrowserError::ReadFailed(err.to_string()))?;
    let version = conn
        .query_row("SELECT value FROM meta WHERE key = 'version'", [], |row| {
            row.get::<_, String>(0)
        })
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let mut stmt = conn
        .prepare(
            "SELECT host_key, name, value, encrypted_value, path, expires_utc, has_expires, is_secure, is_httponly FROM cookies",
        )
        .map_err(|err| BrowserError::ReadFailed(err.to_string()))?;
    let mapped = stmt
        .query_map([], |row| {
            Ok(Row {
                host: row.get::<_, String>(0).unwrap_or_default(),
                name: row.get::<_, String>(1).unwrap_or_default(),
                value: row.get::<_, String>(2).unwrap_or_default(),
                encrypted: row.get::<_, Vec<u8>>(3).unwrap_or_default(),
                path: row.get::<_, String>(4).unwrap_or_default(),
                expires: row.get::<_, i64>(5).unwrap_or(0),
                has_expires: row.get::<_, i64>(6).unwrap_or(0) != 0,
                secure: row.get::<_, i64>(7).unwrap_or(0) != 0,
                http_only: row.get::<_, i64>(8).unwrap_or(0) != 0,
            })
        })
        .map_err(|err| BrowserError::ReadFailed(err.to_string()))?;
    let mut collected = Vec::new();
    for row in mapped {
        collected.push(row.map_err(|err| BrowserError::ReadFailed(err.to_string()))?);
    }
    Ok((version, collected))
}

fn read_db(db_path: &Path, key: &str) -> Result<Vec<CookieRecord>, BrowserError> {
    read_db_with_password(db_path, key, None)
}

fn read_db_with_password(
    db_path: &Path,
    key: &str,
    password_override: Option<&str>,
) -> Result<Vec<CookieRecord>, BrowserError> {
    let (version, rows) = rows(db_path)?;
    let needs_password = rows
        .iter()
        .any(|row| row.encrypted.starts_with(b"v11") && row.value.is_empty());
    let password = if needs_password {
        password_override.map(str::to_owned).or_else(|| {
            spec(key).and_then(|spec| {
                secrets::safe_storage_password(spec.app_names, spec.labels, spec.product)
            })
        })
    } else {
        None
    };
    let mut records = Vec::new();
    for row in rows {
        if row.name.is_empty() || row.host.is_empty() {
            continue;
        }
        if !is_youtube_cookie(&row.host) && !is_google_cookie(&row.host) {
            continue;
        }
        let value = if row.value.is_empty() {
            let Some(value) =
                decrypt_cookie(&row.encrypted, &row.host, version, password.as_deref())
            else {
                continue;
            };
            value
        } else {
            row.value
        };
        if value.is_empty() {
            continue;
        }
        let expires = if row.has_expires && row.expires > 0 {
            row.expires / 1_000_000 - CHROME_EPOCH_OFFSET
        } else {
            0
        };
        records.push(CookieRecord {
            name: row.name,
            value,
            domain: row.host,
            path: if row.path.is_empty() {
                "/".to_owned()
            } else {
                row.path
            },
            secure: row.secure,
            http_only: row.http_only,
            expires,
        });
    }
    let youtube = youtube_records(&records);
    let pairs: Vec<(String, String)> = youtube
        .iter()
        .map(|record| (record.name.clone(), record.value.clone()))
        .collect();
    if !has_login_cookie(&pairs) && !has_primary_sapisid(&pairs) {
        return Err(BrowserError::NoLogin("no SAPISID/login cookie".to_owned()));
    }
    Ok(youtube)
}

pub(crate) fn read_profile(profile: &Path, key: &str) -> Result<Vec<CookieRecord>, BrowserError> {
    let db = cookie_db(profile)
        .ok_or_else(|| BrowserError::ProfileNotFound(profile.display().to_string()))?;
    let staged = temp_copy(&db)?;
    let result = read_db(&staged, key);
    if let Some(dir) = staged.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const V11_PASSWORD: &str = "wp28-test-password";
    const V11_LOGIN: &str = "763131841b464f146c896a846a37ac0963076d7f82cbc9834c6f81875e1807227545e0e5f9a627de556634ef7df0c0f63bd7bb";
    const V11_SAPISID: &str = "763131841b464f146c896a846a37ac0963076d7f82cbc9834c6f81875e1807227545e00f09c8f42512815da0ac619c6b05b275";
    const V11_PREF: &str = "763131841b464f146c896a846a37ac0963076d7f82cbc9834c6f81875e1807227545e0d6f2dbb3c401f22ad80f4e6a8e698e65";

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/chromium/Default/Cookies")
    }

    fn value<'a>(records: &'a [CookieRecord], name: &str) -> Option<&'a str> {
        records
            .iter()
            .find(|record| record.name == name)
            .map(|record| record.value.as_str())
    }

    #[test]
    fn decrypts_real_chromium_v10_database() {
        let records = read_db(&fixture(), "chrome").expect("fixture import");
        assert_eq!(value(&records, "LOGIN_INFO"), Some("dummy-login"));
        assert_eq!(value(&records, "__Secure-3PAPISID"), Some("dummy-sapisid"));
        assert_eq!(value(&records, "PREF"), Some("hl=es"));
        let expiry = records[0].expires;
        assert!(expiry > 1_700_000_000 && expiry < 2_000_000_000, "{expiry}");
    }

    #[test]
    fn derives_chromium_key() {
        let key = derive_key(V11_PASSWORD.as_bytes());
        assert_eq!(
            key.iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "5d5d47b9e47c77d646243f0a0af0d899"
        );
    }

    #[test]
    fn decrypts_v11_with_test_password() {
        for (hex, expected) in [
            (V11_LOGIN, "v11-login"),
            (V11_SAPISID, "v11-sapisid"),
            (V11_PREF, "hl=en"),
        ] {
            let encrypted = hex_to_bytes(hex);
            let plain = decrypt_cookie(&encrypted, ".youtube.com", 24, Some(V11_PASSWORD))
                .expect("v11 decrypt");
            assert_eq!(plain, expected);
            assert_eq!(
                decrypt_cookie(&encrypted, ".youtube.com", 24, Some("wrong")),
                None
            );
        }
    }

    #[test]
    fn v11_database_rows_import() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("Cookies");
        std::fs::copy(fixture(), &db).expect("copy fixture");
        {
            let conn = rusqlite::Connection::open(&db).expect("open");
            conn.execute("DELETE FROM cookies", []).expect("clear");
            for (name, hex) in [
                ("LOGIN_INFO", V11_LOGIN),
                ("__Secure-3PAPISID", V11_SAPISID),
                ("PREF", V11_PREF),
            ] {
                conn.execute(
                    "INSERT INTO cookies (creation_utc, host_key, top_frame_site_key, name, value, encrypted_value, path, expires_utc, is_secure, is_httponly, last_access_utc, has_expires, is_persistent, priority, samesite, source_scheme, source_port, last_update_utc, source_type, has_cross_site_ancestor) VALUES (0, '.youtube.com', '', ?1, '', ?2, '/', 13474330797552757, 1, 1, 0, 1, 1, 1, 0, 2, 443, 0, 0, 0)",
                    rusqlite::params![name, hex_to_bytes(hex)],
                )
                .expect("insert");
            }
        }
        let records = read_db_with_password(&db, "chrome", Some(V11_PASSWORD)).expect("v11 import");
        assert_eq!(value(&records, "LOGIN_INFO"), Some("v11-login"));
        assert_eq!(value(&records, "__Secure-3PAPISID"), Some("v11-sapisid"));
        let err = read_db_with_password(&db, "chrome", Some("wrong")).expect_err("wrong key");
        assert!(matches!(err, BrowserError::NoLogin(_)), "{err:?}");
    }

    #[test]
    fn strips_only_matching_host_hash() {
        let hash = Sha256::digest(b".youtube.com");
        let plain = [hash.to_vec(), b"value".to_vec()].concat();
        assert_eq!(strip_host_hash(&plain, ".youtube.com"), Some(b"value".as_slice()));
        assert_eq!(strip_host_hash(&plain, ".other.com"), None);
        assert_eq!(strip_host_hash(b"short", ".youtube.com"), None);
    }

    #[test]
    fn rejects_bitflipped_ciphertext() {
        let mut encrypted = hex_to_bytes(V11_LOGIN);
        encrypted[20] ^= 0xff;
        assert_eq!(
            decrypt_cookie(&encrypted, ".youtube.com", 24, Some(V11_PASSWORD)),
            None
        );
    }

    #[test]
    fn profile_order_prefers_last_used() {
        let root = tempfile::tempdir().expect("root");
        std::fs::write(
            root.path().join("Local State"),
            r#"{"profile":{"last_used":"Profile 2"}}"#,
        )
        .expect("state");
        for name in ["Default", "Profile 1", "Profile 2", "Profile 10"] {
            let dir = root.path().join(name);
            std::fs::create_dir_all(dir.join("Network")).expect("network dir");
            std::fs::File::create(dir.join("Network/Cookies")).expect("cookies");
        }
        let dirs = profiles(root.path());
        let names: Vec<String> = dirs
            .iter()
            .map(|dir| dir.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["Profile 2", "Default", "Profile 1", "Profile 10"]);
    }

    #[test]
    fn profile_discovery_survives_missing_state() {
        let root = tempfile::tempdir().expect("root");
        std::fs::create_dir_all(root.path().join("Default")).expect("default");
        std::fs::write(root.path().join("Default/Cookies"), b"").expect("legacy");
        std::fs::create_dir_all(root.path().join("Profile 1")).expect("profile 1");
        std::fs::write(root.path().join("Profile 1/Cookies"), b"").expect("cookies");
        let dirs = profiles(root.path());
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[0].file_name().unwrap(), "Default");
    }

    fn hex_to_bytes(hex: &str) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(hex.len() / 2);
        let mut chars = hex.bytes();
        while let (Some(high), Some(low)) = (chars.next(), chars.next()) {
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                b'A'..=b'F' => byte - b'A' + 10,
                _ => panic!("bad hex"),
            };
            bytes.push(digit(high) << 4 | digit(low));
        }
        bytes
    }
}
