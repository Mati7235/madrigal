use reqwest::Client;

use crate::providers::{Provider, ProviderError, ProviderResult, Query};

pub struct YoutubeSubtitleProvider;

impl Provider for YoutubeSubtitleProvider {
    fn name(&self) -> &'static str {
        "youtube_subtitle"
    }

    async fn fetch(&self, _client: &Client, _query: &Query) -> ProviderResult {
        Err(ProviderError::Unsupported("youtube_subtitle"))
    }
}
