use std::time::Duration;

use crate::retry::RetryPolicy;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DohProvider {
    Cloudflare,
    Google,
    AdGuard,
    Quad9,
    Custom(String),
}

impl DohProvider {
    pub fn template_url(&self) -> &str {
        match self {
            DohProvider::Cloudflare => "https://cloudflare-dns.com/dns-query",
            DohProvider::Google => "https://dns.google/dns-query",
            DohProvider::AdGuard => "https://dns.adguard-dns.com/dns-query",
            DohProvider::Quad9 => "https://dns.quad9.net/dns-query",
            DohProvider::Custom(url) => url,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProxyProtocol {
    Http,
    Socks5,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxyConfig {
    pub protocol: ProxyProtocol,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl ProxyConfig {
    pub fn endpoint(&self) -> String {
        match self.protocol {
            ProxyProtocol::Http => format!("http://{}:{}", self.host, self.port),
            ProxyProtocol::Socks5 => format!("socks5h://{}:{}", self.host, self.port),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub doh: Option<DohProvider>,
    pub proxy: Option<ProxyConfig>,
    pub user_agent: String,
    pub timeout: Duration,
    pub connectivity_url: String,
    pub retry: RetryPolicy,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            doh: None,
            proxy: None,
            user_agent: "Madrigal/0.1".to_owned(),
            timeout: Duration::from_secs(15),
            connectivity_url: "https://www.google.com/generate_204".to_owned(),
            retry: RetryPolicy::default(),
        }
    }
}

impl Config {
    pub fn new() -> Self {
        Config::default()
    }

    pub fn with_doh(mut self, provider: DohProvider) -> Self {
        self.doh = Some(provider);
        self
    }

    pub fn without_doh(mut self) -> Self {
        self.doh = None;
        self
    }

    pub fn with_proxy(mut self, proxy: ProxyConfig) -> Self {
        self.proxy = Some(proxy);
        self
    }

    pub fn without_proxy(mut self) -> Self {
        self.proxy = None;
        self
    }

    pub fn with_user_agent(mut self, user_agent: &str) -> Self {
        self.user_agent = user_agent.to_owned();
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_connectivity_url(mut self, url: &str) -> Self {
        self.connectivity_url = url.to_owned();
        self
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }
}
