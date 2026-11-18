use std::env;
use std::fmt;
use std::fs;
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const SERVICE: &str = "madrigal";
pub const KEYRING_ENV: &str = "MADRIGAL_KEYRING";

static COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum SecretError {
    Io(io::Error),
    Keyring(String),
    InvalidAccount(String),
}

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "error de E/S: {err}"),
            Self::Keyring(err) => write!(f, "error de keyring: {err}"),
            Self::InvalidAccount(id) => write!(f, "cuenta inválida: {id:?}"),
        }
    }
}

impl std::error::Error for SecretError {}

impl From<io::Error> for SecretError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

pub fn keyring_enabled() -> bool {
    match env::var(KEYRING_ENV) {
        Ok(value) => value != "0",
        Err(_) => true,
    }
}

fn validate_account(account: &str) -> Result<(), SecretError> {
    if madrigal_paths::valid_profile_id(account) {
        Ok(())
    } else {
        Err(SecretError::InvalidAccount(account.to_string()))
    }
}

pub fn fallback_path(account: &str) -> io::Result<PathBuf> {
    if !madrigal_paths::valid_profile_id(account) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("cuenta inválida: {account:?}"),
        ));
    }
    Ok(madrigal_paths::account_profiles_dir().join(format!("secret_{account}.txt")))
}

fn reject_bad_path(path: &Path) -> io::Result<()> {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => prefix.push(component.as_os_str()),
            Component::Normal(part) => prefix.push(part),
            Component::CurDir => continue,
            Component::ParentDir => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("destino inválido: {}", path.display()),
                ));
            }
        }
        match fs::symlink_metadata(&prefix) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("destino simbólico rechazado: {}", path.display()),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn prepare_private_dir(path: &Path) -> io::Result<PathBuf> {
    reject_bad_path(path)?;
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(path.to_path_buf())
}

pub fn write_private(path: &Path, data: &str) -> io::Result<PathBuf> {
    write_private_bytes(path, data.as_bytes())
}

pub fn write_private_bytes(path: &Path, data: &[u8]) -> io::Result<PathBuf> {
    reject_bad_path(path)?;
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("destino inválido: {}", path.display()),
        )
    })?;
    prepare_private_dir(parent)?;
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("destino inválido: {}", path.display()),
        )
    })?;
    let tag = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = parent.join(format!(
        ".{}.{}.{tag}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| -> io::Result<()> {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&tmp)?;
        #[cfg(unix)]
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(data)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map(|()| path.to_path_buf())
}

pub fn remove_private(path: &Path) {
    let _ = fs::remove_file(path);
}

fn keyring_entry(account: &str) -> Result<keyring::Entry, SecretError> {
    keyring::Entry::new(SERVICE, account).map_err(|err| SecretError::Keyring(err.to_string()))
}

pub fn set_secret(account: &str, secret: &str) -> Result<(), SecretError> {
    validate_account(account)?;
    if keyring_enabled() {
        keyring_entry(account)?
            .set_password(secret)
            .map_err(|err| SecretError::Keyring(err.to_string()))
    } else {
        let path = fallback_path(account)?;
        write_private(&path, secret)?;
        Ok(())
    }
}

pub fn get_secret(account: &str) -> Result<Option<String>, SecretError> {
    validate_account(account)?;
    if keyring_enabled() {
        match keyring_entry(account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(SecretError::Keyring(err.to_string())),
        }
    } else {
        match fs::read_to_string(fallback_path(account)?) {
            Ok(secret) => Ok(Some(secret)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(SecretError::Io(err)),
        }
    }
}

pub fn delete_secret(account: &str) -> Result<(), SecretError> {
    validate_account(account)?;
    if keyring_enabled() {
        match keyring_entry(account)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(SecretError::Keyring(err.to_string())),
        }
    } else {
        remove_private(&fallback_path(account)?);
        Ok(())
    }
}
