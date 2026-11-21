use std::path::PathBuf;

use madrigal_streams::{Orchestrator, StreamConfig};

pub const YTDLP_ENV: &str = "MADRIGAL_YTDLP";
pub const INNERTUBE_ENV: &str = "MADRIGAL_INNERTUBE";

fn innertube_enabled() -> bool {
    let Ok(value) = std::env::var(INNERTUBE_ENV) else {
        return true;
    };
    let value = value.trim().to_ascii_lowercase();
    !matches!(value.as_str(), "0" | "off" | "false" | "no")
}

pub fn stream_config() -> StreamConfig {
    let mut config = StreamConfig::default();
    if let Some(bin) = std::env::var_os(YTDLP_ENV).filter(|value| !value.is_empty()) {
        config.ytdlp_bin = PathBuf::from(bin);
    }
    config.innertube = innertube_enabled();
    config
}

pub fn orchestrator() -> Result<Orchestrator, String> {
    Orchestrator::new(stream_config()).map_err(|err| err.to_string())
}
