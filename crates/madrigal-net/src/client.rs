use crate::config::{ProxyConfig, ProxyProtocol};
use crate::{Config, DohResolver, Error};

fn with_auth(proxy: reqwest::Proxy, config: &ProxyConfig) -> reqwest::Proxy {
    match (&config.username, &config.password) {
        (Some(user), Some(pass)) => proxy.basic_auth(user, pass),
        (Some(user), None) => proxy.basic_auth(user, ""),
        _ => proxy,
    }
}

fn apply_proxy(
    builder: reqwest::blocking::ClientBuilder,
    config: &ProxyConfig,
) -> Result<reqwest::blocking::ClientBuilder, Error> {
    match config.protocol {
        ProxyProtocol::Http => {
            let endpoint = config.endpoint();
            let http = with_auth(
                reqwest::Proxy::http(&endpoint).map_err(Error::from)?,
                config,
            );
            let https = with_auth(
                reqwest::Proxy::https(&endpoint).map_err(Error::from)?,
                config,
            );
            Ok(builder.proxy(http).proxy(https))
        }
        ProxyProtocol::Socks5 => {
            let socks = with_auth(
                reqwest::Proxy::all(config.endpoint()).map_err(Error::from)?,
                config,
            );
            Ok(builder.proxy(socks))
        }
    }
}

fn client_builder(
    config: &Config,
    use_proxy: bool,
) -> Result<reqwest::blocking::ClientBuilder, Error> {
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent(&config.user_agent)
        .timeout(config.timeout);
    if let Some(provider) = &config.doh {
        builder = builder.dns_resolver(std::sync::Arc::new(DohResolver::new(provider.clone())));
    }
    if let Some(proxy) = use_proxy.then(|| config.proxy.clone()).flatten() {
        builder = apply_proxy(builder, &proxy)?;
    }
    Ok(builder)
}

pub fn build_client(config: &Config) -> Result<reqwest::blocking::Client, Error> {
    client_builder(config, true)?.build().map_err(Error::from)
}

pub fn build_stream_client(config: &Config) -> Result<reqwest::blocking::Client, Error> {
    client_builder(config, false)?.build().map_err(Error::from)
}
