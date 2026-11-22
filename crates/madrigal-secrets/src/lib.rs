use std::any::Any;
use std::collections::HashMap;
use std::env;
use std::fmt;
use std::fs;
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use keyring::credential::{
    CredentialApi, CredentialBuilder, CredentialBuilderApi, CredentialPersistence,
};

pub const SERVICE: &str = "madrigal";
pub const KEYRING_ENV: &str = "MADRIGAL_KEYRING";

static COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum SecretError {
    Io(io::Error),
    Keyring(String),
    NoKeyring,
    InvalidAccount(String),
}

impl SecretError {
    pub fn no_keyring(&self) -> bool {
        matches!(self, Self::NoKeyring)
    }
}

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "error de E/S: {err}"),
            Self::Keyring(err) => write!(f, "error de keyring: {err}"),
            Self::NoKeyring => write!(f, "no hay un llavero del sistema disponible"),
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum StoreMode {
    System,
    Memory,
    Failing,
}

fn store_mode() -> StoreMode {
    match env::var(KEYRING_ENV).as_deref() {
        Ok("0") | Ok("mock") => StoreMode::Memory,
        Ok("fail") => StoreMode::Failing,
        _ => StoreMode::System,
    }
}

fn ensure_store() {
    static INSTALLED: Mutex<Option<StoreMode>> = Mutex::new(None);
    let mode = store_mode();
    let mut installed = INSTALLED.lock().unwrap_or_else(|err| err.into_inner());
    if *installed == Some(mode) {
        return;
    }
    if mode == StoreMode::System {
        if installed.is_none() {
            *installed = Some(StoreMode::System);
        }
        return;
    }
    let builder: Box<CredentialBuilder> = match mode {
        StoreMode::Memory => Box::new(MemoryBuilder),
        StoreMode::Failing => Box::new(FailingBuilder),
        StoreMode::System => return,
    };
    keyring::set_default_credential_builder(builder);
    *installed = Some(mode);
}

static MEMORY: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn memory() -> &'static Mutex<HashMap<String, String>> {
    MEMORY.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug)]
struct MemoryCredential {
    key: String,
}

impl CredentialApi for MemoryCredential {
    fn set_secret(&self, secret: &[u8]) -> keyring::Result<()> {
        let secret = String::from_utf8(secret.to_vec())
            .map_err(|err| keyring::Error::BadEncoding(err.into_bytes()))?;
        let mut store = memory().lock().unwrap_or_else(|err| err.into_inner());
        store.insert(self.key.clone(), secret);
        Ok(())
    }

    fn get_secret(&self) -> keyring::Result<Vec<u8>> {
        let store = memory().lock().unwrap_or_else(|err| err.into_inner());
        store
            .get(&self.key)
            .map(|secret| secret.as_bytes().to_vec())
            .ok_or(keyring::Error::NoEntry)
    }

    fn delete_credential(&self) -> keyring::Result<()> {
        let mut store = memory().lock().unwrap_or_else(|err| err.into_inner());
        store
            .remove(&self.key)
            .map(|_| ())
            .ok_or(keyring::Error::NoEntry)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug)]
struct MemoryBuilder;

impl CredentialBuilderApi for MemoryBuilder {
    fn build(
        &self,
        target: Option<&str>,
        service: &str,
        user: &str,
    ) -> keyring::Result<Box<keyring::Credential>> {
        Ok(Box::new(MemoryCredential {
            key: format!("{service}\u{1f}{user}\u{1f}{}", target.unwrap_or("")),
        }))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn persistence(&self) -> CredentialPersistence {
        CredentialPersistence::ProcessOnly
    }
}

fn no_access() -> keyring::Error {
    keyring::Error::NoStorageAccess(Box::new(io::Error::other("Secret Service no disponible")))
}

#[derive(Debug)]
struct FailingCredential;

impl CredentialApi for FailingCredential {
    fn set_secret(&self, _: &[u8]) -> keyring::Result<()> {
        Err(no_access())
    }

    fn get_secret(&self) -> keyring::Result<Vec<u8>> {
        Err(no_access())
    }

    fn delete_credential(&self) -> keyring::Result<()> {
        Err(no_access())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug)]
struct FailingBuilder;

impl CredentialBuilderApi for FailingBuilder {
    fn build(
        &self,
        _: Option<&str>,
        _: &str,
        _: &str,
    ) -> keyring::Result<Box<keyring::Credential>> {
        Ok(Box::new(FailingCredential))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn persistence(&self) -> CredentialPersistence {
        CredentialPersistence::ProcessOnly
    }
}

fn validate_account(account: &str) -> Result<(), SecretError> {
    if madrigal_paths::valid_profile_id(account) {
        Ok(())
    } else {
        Err(SecretError::InvalidAccount(account.to_string()))
    }
}

fn legacy_path(account: &str) -> io::Result<PathBuf> {
    if !madrigal_paths::valid_profile_id(account) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("cuenta inválida: {account:?}"),
        ));
    }
    Ok(madrigal_paths::account_profiles_dir().join(format!("secret_{account}.txt")))
}

fn read_legacy(account: &str) -> io::Result<Option<String>> {
    match fs::read_to_string(legacy_path(account)?) {
        Ok(secret) => Ok(Some(secret)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn drop_legacy(account: &str) {
    if let Ok(path) = legacy_path(account) {
        let _ = fs::remove_file(path);
    }
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
        if let Ok(meta) = fs::symlink_metadata(&prefix) {
            if meta.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("destino simbólico rechazado: {}", path.display()),
                ));
            }
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

fn map_keyring(err: keyring::Error) -> SecretError {
    match err {
        keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_) => {
            SecretError::NoKeyring
        }
        other => SecretError::Keyring(other.to_string()),
    }
}

fn keyring_entry(account: &str) -> Result<keyring::Entry, SecretError> {
    ensure_store();
    keyring::Entry::new(SERVICE, account).map_err(map_keyring)
}

pub fn set_secret(account: &str, secret: &str) -> Result<(), SecretError> {
    validate_account(account)?;
    keyring_entry(account)?
        .set_password(secret)
        .map_err(map_keyring)?;
    drop_legacy(account);
    Ok(())
}

pub fn migrate_legacy_secrets() {
    let Ok(entries) = fs::read_dir(madrigal_paths::account_profiles_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(account) = name
            .strip_prefix("secret_")
            .and_then(|name| name.strip_suffix(".txt"))
        else {
            continue;
        };
        let _ = get_secret(account);
    }
}

pub fn get_secret(account: &str) -> Result<Option<String>, SecretError> {
    validate_account(account)?;
    let entry = keyring_entry(account)?;
    match entry.get_password() {
        Ok(secret) => {
            drop_legacy(account);
            Ok(Some(secret))
        }
        Err(keyring::Error::NoEntry) => {
            let Some(secret) = read_legacy(account)? else {
                return Ok(None);
            };
            entry.set_password(&secret).map_err(map_keyring)?;
            drop_legacy(account);
            Ok(Some(secret))
        }
        Err(err) => Err(map_keyring(err)),
    }
}

pub fn delete_secret(account: &str) -> Result<(), SecretError> {
    validate_account(account)?;
    let result = match keyring_entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(map_keyring(err)),
    };
    drop_legacy(account);
    result
}
