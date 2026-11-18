use serde::{Deserialize, Serialize};

use crate::de::or_default;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Account {
    #[serde(deserialize_with = "or_default")]
    pub active_account_id: String,
    #[serde(deserialize_with = "or_default")]
    pub account_name: String,
    #[serde(deserialize_with = "or_default")]
    pub account_email: String,
    #[serde(deserialize_with = "or_default")]
    pub account_channel_handle: String,
    #[serde(deserialize_with = "or_default")]
    pub saved_accounts_json: String,
    #[serde(deserialize_with = "or_default")]
    pub ytm_sync: bool,
    #[serde(deserialize_with = "or_default")]
    pub force_sync_on_account_switch: bool,
    #[serde(deserialize_with = "or_default")]
    pub last_like_song_sync: i64,
    #[serde(deserialize_with = "or_default")]
    pub last_library_song_sync: i64,
    #[serde(deserialize_with = "or_default")]
    pub last_album_sync: i64,
    #[serde(deserialize_with = "or_default")]
    pub last_artist_sync: i64,
    #[serde(deserialize_with = "or_default")]
    pub last_playlist_sync: i64,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            active_account_id: madrigal_paths::DEFAULT_ACCOUNT_ID.to_owned(),
            account_name: String::new(),
            account_email: String::new(),
            account_channel_handle: String::new(),
            saved_accounts_json: String::new(),
            ytm_sync: true,
            force_sync_on_account_switch: false,
            last_like_song_sync: 0,
            last_library_song_sync: 0,
            last_album_sync: 0,
            last_artist_sync: 0,
            last_playlist_sync: 0,
        }
    }
}

impl Account {
    pub fn sanitize(&mut self) {
        self.active_account_id = crate::sanitize_account_id(&self.active_account_id);
        self.account_name = self.account_name.trim().to_owned();
        self.account_email = self.account_email.trim().to_owned();
        self.account_channel_handle = self.account_channel_handle.trim().to_owned();
        self.saved_accounts_json = self.saved_accounts_json.trim().to_owned();
        for stamp in [
            &mut self.last_like_song_sync,
            &mut self.last_library_song_sync,
            &mut self.last_album_sync,
            &mut self.last_artist_sync,
            &mut self.last_playlist_sync,
        ] {
            if *stamp < 0 {
                *stamp = 0;
            }
        }
    }
}
