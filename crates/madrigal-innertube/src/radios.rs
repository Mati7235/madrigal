use madrigal_core::Track;
use ytmapi_rs::common::{PlaylistID, YoutubeID};
use ytmapi_rs::query::GetWatchPlaylistQuery;

use crate::error::Error;
use crate::CatalogClient;

pub fn is_radio_id(id: &str) -> bool {
    id.starts_with("RD") || id.starts_with("VL")
}

impl CatalogClient {
    pub async fn playlist_radio(&self, playlist_id: &str) -> Result<Vec<Track>, Error> {
        let query =
            GetWatchPlaylistQuery::new_from_playlist_id(PlaylistID::from_raw(playlist_id));
        let value = self.raw_json(query).await?;
        Self::parse_playlist_radio(&value.to_string())
    }

    pub fn parse_playlist_radio(source: &str) -> Result<Vec<Track>, Error> {
        Self::parse_watch_playlist(source)
    }
}
