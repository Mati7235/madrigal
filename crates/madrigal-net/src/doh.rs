use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};

use hickory_resolver::config::{NameServerConfig, ResolverConfig};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::TokioResolver;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use crate::config::DohProvider;
use crate::Error;

fn well_known(provider: &DohProvider) -> Option<(&'static str, Vec<IpAddr>)> {
    match provider {
        DohProvider::Cloudflare => Some((
            "cloudflare-dns.com",
            vec![
                IpAddr::from([1, 1, 1, 1]),
                IpAddr::from([1, 0, 0, 1]),
            ],
        )),
        DohProvider::Google => Some((
            "dns.google",
            vec![IpAddr::from([8, 8, 8, 8]), IpAddr::from([8, 8, 4, 4])],
        )),
        DohProvider::AdGuard => Some((
            "dns.adguard-dns.com",
            vec![
                IpAddr::from([94, 140, 14, 14]),
                IpAddr::from([94, 140, 14, 15]),
            ],
        )),
        DohProvider::Quad9 => Some((
            "dns.quad9.net",
            vec![
                IpAddr::from([9, 9, 9, 9]),
                IpAddr::from([149, 112, 112, 112]),
            ],
        )),
        DohProvider::Custom(_) => None,
    }
}

fn nameserver(ip: IpAddr, server_name: &str, path: &str) -> NameServerConfig {
    NameServerConfig::https(ip, server_name.into(), Some(path.into()))
}

fn custom_parts(url: &str) -> Result<(String, String), Error> {
    let parsed: reqwest::Url = url
        .parse()
        .map_err(|source| Error::Config(format!("malformed DoH template url: {source}")))?;
    if parsed.scheme() != "https" {
        return Err(Error::Config("DoH template url must use https".to_owned()));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| Error::Config("DoH template url has no host".to_owned()))?
        .to_owned();
    let path = parsed.path().to_owned();
    Ok((host, path))
}

fn resolver_config_sync(provider: &DohProvider) -> Result<ResolverConfig, Error> {
    if let Some((server_name, ips)) = well_known(provider) {
        let path = provider.template_url();
        let path = reqwest::Url::parse(path)
            .map(|url| url.path().to_owned())
            .unwrap_or_else(|_| "/dns-query".to_owned());
        return Ok(ResolverConfig::from_name_servers(
            ips.into_iter().map(|ip| nameserver(ip, server_name, &path)).collect(),
        ));
    }
    if let DohProvider::Custom(url) = provider {
        let (host, path) = custom_parts(url)?;
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Ok(ResolverConfig::from_name_servers(vec![nameserver(ip, &host, &path)]));
        }
        return Err(Error::Config(
            "custom DoH hostname needs a bootstrap lookup at resolve time".to_owned(),
        ));
    }
    Err(Error::Config("unknown DoH provider".to_owned()))
}

async fn bootstrap_ip(host: &str) -> std::io::Result<IpAddr> {
    let mut addrs = tokio::net::lookup_host(format!("{host}:443"))
        .await
        .map_err(|_| std::io::Error::other(format!("DoH bootstrap host has no address: {host}")))?;
    addrs
        .next()
        .map(|socket| socket.ip())
        .ok_or_else(|| std::io::Error::other(format!("DoH bootstrap host has no address: {host}")))
}

async fn build_resolver(provider: &DohProvider) -> std::io::Result<TokioResolver> {
    let config = if let DohProvider::Custom(url) = provider {
        let (host, path) = custom_parts(url).map_err(std::io::Error::other)?;
        let ip = match host.parse::<IpAddr>() {
            Ok(ip) => ip,
            Err(_) => bootstrap_ip(&host).await?,
        };
        ResolverConfig::from_name_servers(vec![nameserver(ip, &host, &path)])
    } else {
        resolver_config_sync(provider).map_err(std::io::Error::other)?
    };
    TokioResolver::builder_with_config(config, TokioRuntimeProvider::default())
        .build()
        .map_err(std::io::Error::other)
}

#[derive(Clone, Debug)]
pub struct DohResolver {
    provider: DohProvider,
    cached: Arc<Mutex<Option<TokioResolver>>>,
}

impl DohResolver {
    pub fn new(provider: DohProvider) -> Self {
        DohResolver {
            provider,
            cached: Arc::new(Mutex::new(None)),
        }
    }

    pub fn provider(&self) -> &DohProvider {
        &self.provider
    }

    pub fn resolver_config(&self) -> Result<ResolverConfig, Error> {
        resolver_config_sync(&self.provider)
    }
}

impl Resolve for DohResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let provider = self.provider.clone();
        let cached = Arc::clone(&self.cached);
        Box::pin(async move {
            let hit = cached.lock().ok().and_then(|guard| guard.clone());
            let resolver = match hit {
                Some(resolver) => resolver,
                None => {
                    let fresh = build_resolver(&provider).await?;
                    if let Ok(mut guard) = cached.lock() {
                        *guard = Some(fresh.clone());
                    }
                    fresh
                }
            };
            let lookup = resolver
                .lookup_ip(name.as_str())
                .await
                .map_err(std::io::Error::other)?;
            let owned: Vec<SocketAddr> =
                lookup.iter().map(|ip| SocketAddr::new(ip, 0)).collect();
            let addrs: Addrs = Box::new(owned.into_iter());
            Ok(addrs)
        })
    }
}
