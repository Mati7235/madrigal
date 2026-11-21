use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cookies::{
    has_login_cookie, has_primary_sapisid, youtube_records, CookieRecord,
};

pub const BROWSERS: [(&str, &str); 7] = [
    ("firefox", "Firefox"),
    ("brave", "Brave"),
    ("chrome", "Google Chrome"),
    ("chromium", "Chromium"),
    ("edge", "Microsoft Edge"),
    ("opera", "Opera"),
    ("vivaldi", "Vivaldi"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserError {
    Unknown(String),
    Unsupported(String),
    NotFound(String),
    ProfileNotFound(String),
    NoLogin(String),
    ReadFailed(String),
}

impl fmt::Display for BrowserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(browser) => write!(f, "unknown browser: {browser:?}"),
            Self::Unsupported(browser) => {
                write!(f, "browser cookie import not implemented: {browser:?}")
            }
            Self::NotFound(detail) => write!(f, "firefox not found: {detail}"),
            Self::ProfileNotFound(detail) => write!(f, "firefox profile not found: {detail}"),
            Self::NoLogin(detail) => write!(f, "no youtube login in profile: {detail}"),
            Self::ReadFailed(detail) => write!(f, "could not read cookies: {detail}"),
        }
    }
}

impl std::error::Error for BrowserError {}

const MS_THRESHOLD: i64 = 100_000_000_000;

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from).filter(|dir| !dir.as_os_str().is_empty())
}

fn firefox_roots() -> Vec<PathBuf> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| home.join(".config"));
    vec![
        home.join(".mozilla/firefox"),
        config.join("mozilla/firefox"),
        home.join("snap/firefox/common/.mozilla/firefox"),
        home.join(".var/app/org.mozilla.firefox/.mozilla/firefox"),
    ]
}

#[derive(Default)]
struct ProfileEntry {
    path_raw: String,
    is_relative: bool,
    is_default: bool,
}

#[derive(Default)]
struct ProfilesIni {
    installs: Vec<String>,
    profiles: Vec<ProfileEntry>,
}

fn parse_profiles_ini(text: &str) -> ProfilesIni {
    let mut parsed = ProfilesIni::default();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_owned();
            if section.starts_with("Profile") {
                parsed.profiles.push(ProfileEntry::default());
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().to_owned();
        if section.starts_with("Install") {
            if key == "Default" && !value.is_empty() {
                parsed.installs.push(value);
            }
        } else if section.starts_with("Profile") {
            let Some(entry) = parsed.profiles.last_mut() else {
                continue;
            };
            match key {
                "Path" => entry.path_raw = value,
                "IsRelative" => entry.is_relative = value != "0",
                "Default" => entry.is_default = value == "1",
                _ => {}
            }
        }
    }
    parsed
}

fn resolve_ini_path(root: &Path, raw: &str, is_relative: bool) -> PathBuf {
    let candidate = Path::new(raw);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        let _ = is_relative;
        root.join(candidate)
    }
}

fn first_install_default(root: &Path, ini: &ProfilesIni) -> Option<PathBuf> {
    ini.installs
        .iter()
        .map(|raw| resolve_ini_path(root, raw, true))
        .find(|dir| dir.join("cookies.sqlite").is_file())
}

fn default_profile_dir(root: &Path, ini: &ProfilesIni) -> Option<PathBuf> {
    if let Some(dir) = first_install_default(root, ini) {
        return Some(dir);
    }
    if let Some(entry) = ini.profiles.iter().find(|entry| entry.is_default) {
        let dir = resolve_ini_path(root, &entry.path_raw, entry.is_relative);
        if dir.join("cookies.sqlite").is_file() {
            return Some(dir);
        }
    }
    ini.profiles
        .iter()
        .map(|entry| resolve_ini_path(root, &entry.path_raw, entry.is_relative))
        .find(|dir| dir.join("cookies.sqlite").is_file())
        .or_else(|| {
            ini.profiles
                .first()
                .map(|entry| resolve_ini_path(root, &entry.path_raw, entry.is_relative))
        })
}

fn find_default_profile() -> Result<PathBuf, BrowserError> {
    let roots = firefox_roots();
    if roots.is_empty() || !roots.iter().any(|root| root.is_dir()) {
        return Err(BrowserError::NotFound("no firefox directory".to_owned()));
    }
    let mut saw_ini = false;
    for root in &roots {
        if !root.is_dir() {
            continue;
        }
        let ini_path = root.join("profiles.ini");
        let Ok(text) = std::fs::read_to_string(&ini_path) else {
            continue;
        };
        saw_ini = true;
        let ini = parse_profiles_ini(&text);
        if let Some(dir) = default_profile_dir(root, &ini) {
            return Ok(dir);
        }
    }
    if saw_ini {
        Err(BrowserError::ProfileNotFound("no usable profile".to_owned()))
    } else {
        Err(BrowserError::NotFound("no profiles.ini".to_owned()))
    }
}

fn profile_arg_dir(profile: &str) -> Result<PathBuf, BrowserError> {
    let direct = Path::new(profile);
    if direct.is_dir() && direct.join("cookies.sqlite").is_file() {
        return Ok(direct.to_path_buf());
    }
    for root in firefox_roots() {
        let candidate = root.join(direct);
        if candidate.is_dir() && candidate.join("cookies.sqlite").is_file() {
            return Ok(candidate);
        }
        let ini_path = root.join("profiles.ini");
        if let Ok(text) = std::fs::read_to_string(&ini_path) {
            let ini = parse_profiles_ini(&text);
            for entry in &ini.profiles {
                let dir = resolve_ini_path(&root, &entry.path_raw, entry.is_relative);
                if dir.file_name().is_some_and(|name| name == direct)
                    && dir.join("cookies.sqlite").is_file()
                {
                    return Ok(dir);
                }
            }
        }
    }
    Err(BrowserError::ProfileNotFound(profile.to_owned()))
}

fn temp_copy(dir: &Path) -> Result<PathBuf, BrowserError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|now| now.as_nanos())
        .unwrap_or(0);
    let target = env::temp_dir().join(format!(
        "madrigal-firefox-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir_all(&target)
        .map_err(|err| BrowserError::ReadFailed(err.to_string()))?;
    for name in ["cookies.sqlite", "cookies.sqlite-wal"] {
        let from = dir.join(name);
        if from.is_file() {
            let to = target.join(name);
            std::fs::copy(&from, &to)
                .map_err(|err| BrowserError::ReadFailed(err.to_string()))?;
        }
    }
    if !target.join("cookies.sqlite").is_file() {
        let _ = std::fs::remove_dir_all(&target);
        return Err(BrowserError::ProfileNotFound("cookies.sqlite missing".to_owned()));
    }
    Ok(target)
}

fn read_records(db_path: &Path) -> Result<Vec<CookieRecord>, BrowserError> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|err| BrowserError::ReadFailed(err.to_string()))?;
    let mut stmt = conn
        .prepare(
            "SELECT host, name, value, path, expiry, isSecure, isHttpOnly, originAttributes FROM moz_cookies",
        )
        .map_err(|err| BrowserError::ReadFailed(err.to_string()))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4).unwrap_or(0),
                row.get::<_, i64>(5).unwrap_or(0),
                row.get::<_, i64>(6).unwrap_or(0),
                row.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(|err| BrowserError::ReadFailed(err.to_string()))?;
    let mut records = Vec::new();
    for row in rows {
        let (host, name, value, path, expiry, secure, http_only, origin) =
            row.map_err(|err| BrowserError::ReadFailed(err.to_string()))?;
        if origin.as_deref().is_some_and(|origin| !origin.is_empty()) {
            continue;
        }
        let (Some(host), Some(name), Some(value)) = (host, name, value) else {
            continue;
        };
        if name.is_empty() || host.is_empty() {
            continue;
        }
        let expires = if expiry > MS_THRESHOLD { expiry / 1000 } else { expiry };
        let path = match path {
            Some(path) if !path.is_empty() => path,
            _ => "/".to_owned(),
        };
        records.push(CookieRecord {
            name,
            value,
            domain: host,
            path,
            secure: secure != 0,
            http_only: http_only != 0,
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

pub fn records_from_browser(browser: &str, profile: &str) -> Result<Vec<CookieRecord>, BrowserError> {
    if !BROWSERS.iter().any(|(key, _)| *key == browser) {
        return Err(BrowserError::Unknown(browser.to_owned()));
    }
    if browser != "firefox" {
        return Err(BrowserError::Unsupported(browser.to_owned()));
    }
    let dir = if profile.trim().is_empty() {
        find_default_profile()?
    } else {
        profile_arg_dir(profile)?
    };
    let staged = temp_copy(&dir)?;
    let result = read_records(&staged.join("cookies.sqlite"));
    let _ = std::fs::remove_dir_all(&staged);
    result
}
