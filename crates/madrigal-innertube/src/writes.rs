use ytmapi_rs::common::{ApiOutcome, LikeStatus, PlaylistID, SetVideoID, VideoID, YoutubeID};
use ytmapi_rs::parse::{AddPlaylistItem, ParseFrom};
use ytmapi_rs::query::playlist::{
    AddPlaylistItemsQuery, AddVideosToPlaylist, CreatePlaylistQuery, DuplicateHandlingMode,
    EditPlaylistQuery, PrivacyStatus,
};
use ytmapi_rs::query::{RateSongQuery, RemoveHistoryItemsQuery};
use ytmapi_rs::ProcessedResult;

use crate::error::Error;
use crate::library::AuthCatalogClient;

#[derive(Debug, Clone, PartialEq)]
pub struct AddedEntry {
    pub video_id: String,
    pub set_video_id: String,
}

impl AuthCatalogClient {
    pub async fn create_playlist(
        &self,
        title: &str,
        description: Option<&str>,
        privacy: PrivacyStatus,
    ) -> Result<String, Error> {
        let query = CreatePlaylistQuery::new(title, description, privacy);
        Ok(self.yt.create_playlist(query).await?.get_raw().to_owned())
    }

    pub async fn rename_playlist(
        &self,
        playlist_id: &str,
        title: &str,
    ) -> Result<ApiOutcome, Error> {
        let query = EditPlaylistQuery::new_title(PlaylistID::from_raw(playlist_id), title);
        Ok(self.yt.edit_playlist(query).await?)
    }

    pub async fn describe_playlist(
        &self,
        playlist_id: &str,
        description: &str,
    ) -> Result<ApiOutcome, Error> {
        let query =
            EditPlaylistQuery::new_description(PlaylistID::from_raw(playlist_id), description);
        Ok(self.yt.edit_playlist(query).await?)
    }

    pub async fn privatise_playlist(
        &self,
        playlist_id: &str,
        privacy: PrivacyStatus,
    ) -> Result<ApiOutcome, Error> {
        let query =
            EditPlaylistQuery::new_privacy_status(PlaylistID::from_raw(playlist_id), privacy);
        Ok(self.yt.edit_playlist(query).await?)
    }

    pub async fn add_tracks(
        &self,
        playlist_id: &str,
        video_ids: &[String],
    ) -> Result<Vec<AddedEntry>, Error> {
        let query = AddPlaylistItemsQuery::new_from_videos(
            PlaylistID::from_raw(playlist_id),
            video_ids.iter().map(|id| VideoID::from_raw(id.as_str())),
            DuplicateHandlingMode::default(),
        );
        Ok(self
            .yt
            .query(query)
            .await?
            .iter()
            .map(added_entry)
            .collect())
    }

    pub async fn remove_entries(
        &self,
        playlist_id: &str,
        set_video_ids: &[String],
    ) -> Result<(), Error> {
        self.yt
            .remove_playlist_items(
                PlaylistID::from_raw(playlist_id),
                set_video_ids.iter().map(|id| SetVideoID::from_raw(id.as_str())),
            )
            .await?;
        Ok(())
    }

    pub async fn move_entry(
        &self,
        playlist_id: &str,
        first: &str,
        second: &str,
    ) -> Result<ApiOutcome, Error> {
        let query = EditPlaylistQuery::swap_videos_order(
            PlaylistID::from_raw(playlist_id),
            SetVideoID::from_raw(first),
            SetVideoID::from_raw(second),
        );
        Ok(self.yt.edit_playlist(query).await?)
    }

    pub async fn delete_playlist(&self, playlist_id: &str) -> Result<(), Error> {
        self.yt
            .delete_playlist(PlaylistID::from_raw(playlist_id))
            .await?;
        Ok(())
    }

    pub async fn rate_song(&self, video_id: &str, liked: bool) -> Result<(), Error> {
        let rating = if liked {
            LikeStatus::Liked
        } else {
            LikeStatus::Indifferent
        };
        self.yt.rate_song(VideoID::from_raw(video_id), rating).await?;
        Ok(())
    }

    pub fn parse_rate_song(source: &str) -> Result<(), Error> {
        let query = RateSongQuery::new(VideoID::from_raw(""), LikeStatus::Liked);
        offline::<RateSongQuery, ()>(&query, source)
    }

    pub fn parse_edit_playlist(source: &str) -> Result<ApiOutcome, Error> {
        let query = EditPlaylistQuery::new_title(PlaylistID::from_raw(""), "");
        offline::<EditPlaylistQuery, ApiOutcome>(&query, source)
    }

    pub fn parse_add_playlist_items(source: &str) -> Result<Vec<AddedEntry>, Error> {
        let query = AddPlaylistItemsQuery::new_from_videos(
            PlaylistID::from_raw(""),
            Vec::new(),
            DuplicateHandlingMode::default(),
        );
        let parsed =
            offline::<AddPlaylistItemsQuery<AddVideosToPlaylist>, Vec<AddPlaylistItem>>(
                &query, source,
            )?;
        Ok(parsed.iter().map(added_entry).collect())
    }

    pub fn parse_remove_history_items(source: &str) -> Result<Vec<ApiOutcome>, Error> {
        let query = RemoveHistoryItemsQuery::new(Vec::new());
        offline::<RemoveHistoryItemsQuery, Vec<ApiOutcome>>(&query, source)
    }
}

fn added_entry(item: &AddPlaylistItem) -> AddedEntry {
    AddedEntry {
        video_id: item.video_id.get_raw().to_owned(),
        set_video_id: item.set_video_id.get_raw().to_owned(),
    }
}

fn offline<Q, O>(query: &Q, source: &str) -> Result<O, Error>
where
    O: ParseFrom<Q>,
{
    let json: ytmapi_rs::json::Json = serde_json::from_str(source)?;
    let processed = ProcessedResult {
        query,
        source: source.to_owned(),
        json,
    };
    Ok(O::parse_from(processed)?)
}
