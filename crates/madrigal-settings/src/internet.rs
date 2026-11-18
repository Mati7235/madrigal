use serde::{Deserialize, Serialize};

use crate::de::or_default;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProxyType {
    #[default]
    Http,
    Socks5,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Internet {
    #[serde(deserialize_with = "or_default")]
    pub pot_server_home: String,
    #[serde(deserialize_with = "or_default")]
    pub proxy_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub proxy_host: String,
    #[serde(deserialize_with = "or_default")]
    pub proxy_port: i32,
    #[serde(deserialize_with = "or_default")]
    pub proxy_username: String,
    #[serde(deserialize_with = "or_default")]
    pub proxy_password: String,
    #[serde(deserialize_with = "or_default")]
    pub proxy_type: ProxyType,
    #[serde(deserialize_with = "or_default")]
    pub dns_over_https_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub dns_over_https_provider: String,
    #[serde(deserialize_with = "or_default")]
    pub stream_bypass_proxy: bool,
    #[serde(deserialize_with = "or_default")]
    pub network_metered: bool,
    #[serde(deserialize_with = "or_default")]
    pub use_login_for_browse: bool,
    #[serde(deserialize_with = "or_default")]
    pub web_client_po_token_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub use_visitor_data: bool,
    #[serde(deserialize_with = "or_default")]
    pub po_token_source_url: String,
    #[serde(deserialize_with = "or_default")]
    pub offline_mode: bool,
}

impl Default for Internet {
    fn default() -> Self {
        Self {
            pot_server_home: String::new(),
            proxy_enabled: false,
            proxy_host: String::new(),
            proxy_port: 8080,
            proxy_username: String::new(),
            proxy_password: String::new(),
            proxy_type: ProxyType::Http,
            dns_over_https_enabled: false,
            dns_over_https_provider: String::new(),
            stream_bypass_proxy: false,
            network_metered: false,
            use_login_for_browse: false,
            web_client_po_token_enabled: true,
            use_visitor_data: true,
            po_token_source_url: String::new(),
            offline_mode: false,
        }
    }
}

impl Internet {
    pub fn sanitize(&mut self) {
        if self.proxy_port < 1 || self.proxy_port > 65535 {
            self.proxy_port = 8080;
        }
        self.pot_server_home = self.pot_server_home.trim().to_owned();
        self.proxy_host = self.proxy_host.trim().to_owned();
        self.proxy_username = self.proxy_username.trim().to_owned();
        self.proxy_password = self.proxy_password.trim().to_owned();
        self.dns_over_https_provider = self.dns_over_https_provider.trim().to_owned();
        self.po_token_source_url = self.po_token_source_url.trim().to_owned();
    }
}
