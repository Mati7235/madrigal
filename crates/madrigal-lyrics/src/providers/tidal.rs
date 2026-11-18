use reqwest::Client;

use crate::providers::{Provider, ProviderError, ProviderResult, Query};

pub struct TidalProvider;

impl Provider for TidalProvider {
    fn name(&self) -> &'static str {
        "tidal"
    }

    async fn fetch(&self, _client: &Client, _query: &Query) -> ProviderResult {
        Err(ProviderError::Unsupported("tidal"))
    }
}
