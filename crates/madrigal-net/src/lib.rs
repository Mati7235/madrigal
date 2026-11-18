pub mod client;
pub mod config;
pub mod connectivity;
pub mod doh;
pub mod retry;

pub use client::{build_client, build_stream_client};
pub use config::{Config, DohProvider, ProxyConfig, ProxyProtocol};
pub use connectivity::{Probe, test_connectivity};
pub use doh::DohResolver;
pub use retry::{RetryError, RetryPolicy, SectionBudget, delay_for_attempt};

#[derive(Debug)]
pub enum Error {
    Config(String),
    Transport(reqwest::Error),
    Dns(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Config(detail) => write!(f, "invalid net config: {detail}"),
            Error::Transport(source) => write!(f, "http transport failed: {source}"),
            Error::Dns(source) => write!(f, "dns resolution failed: {source}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Config(_) => None,
            Error::Transport(source) => Some(source),
            Error::Dns(source) => Some(source),
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(source: reqwest::Error) -> Self {
        Error::Transport(source)
    }
}
