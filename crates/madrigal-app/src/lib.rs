pub mod catalog;
pub mod cli;
pub mod lyrics;
pub mod library;
pub mod downloads;
pub mod desktop;
pub mod actions;
pub mod playback;
pub mod state;
pub mod store;
pub mod streams;
pub mod ui;
pub mod account;

use std::path::Path;
use std::time::Instant;

use store::Store;

pub struct Boot {
    pub store: Store,
    pub startup_ms: u64,
}

pub fn boot_headless(profile_dir: &Path) -> Result<Boot, String> {
    let started = Instant::now();
    if !profile_dir.as_os_str().is_empty() {
        std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, profile_dir);
    }
    let store = Store::open()?;
    let startup_ms = started.elapsed().as_millis() as u64;
    Ok(Boot { store, startup_ms })
}
