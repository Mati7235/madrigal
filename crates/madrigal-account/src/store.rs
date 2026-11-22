use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::session::Session;
use madrigal_secrets::SecretError;

fn checked_id(profile: &str) -> io::Result<&str> {
    if madrigal_paths::valid_profile_id(profile) {
        Ok(profile)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid account id: {profile:?}"),
        ))
    }
}

#[derive(Debug, Clone, Default)]
pub struct SessionStore {
    dir: Option<PathBuf>,
}

impl SessionStore {
    pub fn with_dir(dir: PathBuf) -> Self {
        Self { dir: Some(dir) }
    }

    fn account_dir(&self) -> PathBuf {
        self.dir.clone().unwrap_or_else(madrigal_paths::account_profiles_dir)
    }

    pub fn account_path(&self, profile: &str) -> io::Result<PathBuf> {
        let id = checked_id(profile)?;
        match &self.dir {
            None => madrigal_paths::profile_account_path(id),
            Some(dir) => Ok(dir.join(format!("session_{id}.json"))),
        }
    }

    pub fn cookies_path(&self, profile: Option<&str>) -> io::Result<PathBuf> {
        let fallback;
        let id = match profile {
            Some(id) => checked_id(id)?,
            None => {
                fallback = self.active_id();
                fallback.as_str()
            }
        };
        match &self.dir {
            None => madrigal_paths::profile_cookies_path(id),
            Some(dir) => Ok(dir.join(format!("cookies_{id}.txt"))),
        }
    }

    pub fn ids(&self) -> Vec<String> {
        let mut found = Vec::new();
        if let Ok(entries) = std::fs::read_dir(self.account_dir()) {
            let mut names: Vec<String> = entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .and_then(|name| name.strip_prefix("session_")?.strip_suffix(".json"))
                        .filter(|id| madrigal_paths::valid_profile_id(id))
                        .map(str::to_owned)
                })
                .collect();
            names.sort();
            names.dedup();
            found = names;
        }
        if !found.contains(&madrigal_paths::DEFAULT_ACCOUNT_ID.to_owned()) {
            found.insert(0, madrigal_paths::DEFAULT_ACCOUNT_ID.to_owned());
        }
        found
    }

    pub fn active_id(&self) -> String {
        madrigal_settings::Settings::load().account.active_account_id
    }

    pub fn create(&self, profile: &str) -> Result<PathBuf, SecretError> {
        let path = self.account_path(profile).map_err(SecretError::Io)?;
        if !path.exists() {
            Session::default().save(&path)?;
        }
        Ok(path)
    }

    pub fn load(&self, profile: &str) -> Session {
        match self.account_path(profile) {
            Ok(path) => Session::load(&path),
            Err(_) => Session::default(),
        }
    }

    pub fn active(&self) -> Session {
        self.load(&self.active_id())
    }

    pub fn migrate(&self) {
        madrigal_secrets::migrate_legacy_secrets();
        for id in self.ids() {
            let _ = self.load(&id);
        }
    }

    pub fn save(&self, session: &Session, profile: Option<&str>) -> Result<String, SecretError> {
        let fallback = self.active_id();
        let id = checked_id(profile.unwrap_or(&fallback))
            .map_err(SecretError::Io)?
            .to_owned();
        session.save(&self.account_path(&id).map_err(SecretError::Io)?)?;
        Ok(id)
    }

    pub fn activate(&self, profile: &str) -> io::Result<Session> {
        let id = checked_id(profile)?.to_owned();
        madrigal_settings::set_active_account(&id)?;
        Ok(self.load(&id))
    }

    pub fn remove(&self, profile: &str) -> io::Result<()> {
        if profile == madrigal_paths::DEFAULT_ACCOUNT_ID || !madrigal_paths::valid_profile_id(profile)
        {
            return Ok(());
        }
        let _ = madrigal_secrets::delete_secret(profile);
        if let Ok(path) = self.account_path(profile) {
            madrigal_secrets::remove_private(&path);
        }
        if let Ok(path) = self.cookies_path(Some(profile)) {
            madrigal_secrets::remove_private(&path);
        }
        if self.active_id() == profile {
            madrigal_settings::set_active_account(madrigal_paths::DEFAULT_ACCOUNT_ID)?;
        }
        Ok(())
    }
}

impl fmt::Display for SessionStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.account_dir().display())
    }
}
