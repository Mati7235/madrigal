use std::env;
use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

pub const PROJECT_DIR: &str = "madrigal";
pub const PROFILE_ROOT_ENV: &str = "MADRIGAL_PROFILE_ROOT";
pub const DEFAULT_ACCOUNT_ID: &str = "default";

pub fn profile_root() -> Option<PathBuf> {
    let root = env::var_os(PROFILE_ROOT_ENV)?;
    if root.is_empty() {
        None
    } else {
        Some(PathBuf::from(root))
    }
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn xdg(var: &str, default: &str, leaf: &str) -> PathBuf {
    if let Some(root) = profile_root() {
        return root.join(leaf).join(PROJECT_DIR);
    }
    match env::var(var) {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value.trim()).join(PROJECT_DIR),
        _ => home_dir().join(default).join(PROJECT_DIR),
    }
}

pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share", "data")
}

pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config", "config")
}

pub fn cache_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache", "cache")
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn db_path() -> PathBuf {
    data_dir().join("library.sqlite3")
}

pub fn account_profiles_dir() -> PathBuf {
    config_dir().join("accounts")
}

pub fn runtime_dir() -> PathBuf {
    data_dir().join("runtime")
}

fn config_home() -> PathBuf {
    match env::var("XDG_CONFIG_HOME") {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value.trim()),
        _ => home_dir().join(".config"),
    }
}

fn user_dirs_music() -> Option<PathBuf> {
    let text = fs::read_to_string(config_home().join("user-dirs.dirs")).ok()?;
    for line in text.lines() {
        let line = line.trim();
        let Some(value) = line.strip_prefix("XDG_MUSIC_DIR=") else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        if value.is_empty() {
            continue;
        }
        let value = value
            .replace("${HOME}", &home_dir().to_string_lossy())
            .replace("$HOME", &home_dir().to_string_lossy());
        return Some(PathBuf::from(value));
    }
    None
}

pub fn music_dir() -> PathBuf {
    if let Some(root) = profile_root() {
        if let Ok(value) = env::var("XDG_MUSIC_DIR") {
            let path = PathBuf::from(value.trim());
            if !value.trim().is_empty() && path.starts_with(&root) {
                return path;
            }
        }
        return root.join("music");
    }
    if let Ok(value) = env::var("XDG_MUSIC_DIR") {
        let path = PathBuf::from(value.trim());
        if !value.trim().is_empty() {
            return path;
        }
    }
    user_dirs_music().unwrap_or_else(|| home_dir().join("Music"))
}

pub fn valid_profile_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 {
        return false;
    }
    if !bytes[0].is_ascii_alphanumeric() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
}

fn profile_component(id: &str) -> io::Result<&str> {
    if valid_profile_id(id) {
        Ok(id)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("identificador de cuenta inválido: {id:?}"),
        ))
    }
}

pub fn profile_account_path(profile: &str) -> io::Result<PathBuf> {
    let id = profile_component(profile)?;
    Ok(account_profiles_dir().join(format!("session_{id}.json")))
}

pub fn profile_cookies_path(profile: &str) -> io::Result<PathBuf> {
    let id = profile_component(profile)?;
    Ok(runtime_dir().join(format!("cookies_{id}.txt")))
}

fn reject_symlink_prefixes(path: &Path) -> io::Result<()> {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => prefix.push(component.as_os_str()),
            Component::Normal(part) => prefix.push(part),
            Component::CurDir => continue,
            Component::ParentDir => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("ruta inválida: {}", path.display()),
                ));
            }
        }
        match fs::symlink_metadata(&prefix) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("ruta simbólica rechazada: {}", path.display()),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn secure_dir(path: &Path) -> io::Result<PathBuf> {
    reject_symlink_prefixes(path)?;
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(path.to_path_buf())
}

pub fn ensure_dirs() -> io::Result<()> {
    for dir in [
        data_dir(),
        config_dir(),
        cache_dir(),
        account_profiles_dir(),
        runtime_dir(),
    ] {
        secure_dir(&dir)?;
    }
    Ok(())
}
