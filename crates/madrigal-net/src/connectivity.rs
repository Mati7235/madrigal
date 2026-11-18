use std::time::{Duration, Instant};

use crate::{Config, Error, build_client};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe {
    pub status: u16,
    pub latency: Duration,
}

pub fn test_connectivity(config: &Config) -> Result<Probe, Error> {
    let client = build_client(config)?;
    let start = Instant::now();
    let response = client.get(&config.connectivity_url).send()?;
    Ok(Probe {
        status: response.status().as_u16(),
        latency: start.elapsed(),
    })
}
