use ytmapi_rs::auth::BrowserToken;
use ytmapi_rs::common::YoutubeID;
use ytmapi_rs::parse::{
    HistoryItem, HistoryPeriod as NativePeriod, LibraryPlaylist as NativePlaylist, ParseFrom,
    TableListSong,
};
use ytmapi_rs::query::{GetHistoryQuery, GetLibraryPlaylistsQuery, GetLibrarySongsQuery};
use ytmapi_rs::{ProcessedResult, YtMusic};

use madrigal_core::{Playlist, PlaylistId, PlaylistKind, Track, TrackIdentity, parse_duration};

use crate::error::Error;
use crate::mapping;
use crate::CatalogClient;

pub struct AuthCatalogClient {
    pub(crate) yt: YtMusic<BrowserToken>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryPeriod {
    pub name: String,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct HistoryFeed {
    pub periods: Vec<HistoryPeriod>,
}

impl CatalogClient {
    pub fn with_cookies(cookie_header: &str) -> Result<AuthCatalogClient, Error> {
        let pairs = madrigal_account::cookies::parse_cookie_header(cookie_header)
            .map_err(|e| Error::Fixture(e.to_string()))?;
        if !madrigal_account::cookies::has_login_cookie(&pairs) {
            return Err(Error::Fixture(
                "cookie header lacks login cookies".to_owned(),
            ));
        }
        let sapisid = madrigal_account::cookies::login_cookie_value(&pairs)
            .ok_or_else(|| Error::Fixture("cookie header lacks SAPISID".to_owned()))?;
        let token: BrowserToken = serde_json::from_value(serde_json::json!({
            "sapisid": sapisid,
            "client_version": "1.20261118.01.00",
            "cookies": cookie_header,
        }))
        // BrowserToken has no sync constructor; field names pinned to ytmapi-rs =0.3.4.
        .map_err(|_| Error::Fixture("browser token shape changed".to_owned()))?;
        Ok(AuthCatalogClient {
            yt: YtMusic::from_auth_token(token),
        })
    }
}

impl AuthCatalogClient {
    pub fn from_token(token: BrowserToken) -> Self {
        Self {
            yt: YtMusic::from_auth_token(token),
        }
    }

    pub async fn liked_songs(&self) -> Result<Vec<Track>, Error> {
        Ok(self
            .yt
            .get_library_songs()
            .await?
            .iter()
            .map(table_song_to_track)
            .collect())
    }

    pub async fn library_playlists(&self) -> Result<Vec<Playlist>, Error> {
        Ok(self
            .yt
            .get_library_playlists()
            .await?
            .iter()
            .map(library_playlist_to_playlist)
            .collect())
    }

    pub async fn history(&self) -> Result<HistoryFeed, Error> {
        Ok(history_from_native(&self.yt.get_history().await?))
    }

    pub fn parse_liked_songs(source: &str) -> Result<Vec<Track>, Error> {
        let query = GetLibrarySongsQuery::default();
        let parsed = offline::<GetLibrarySongsQuery, Vec<TableListSong>>(&query, source)?;
        Ok(parsed.iter().map(table_song_to_track).collect())
    }

    pub fn parse_library_playlists(source: &str) -> Result<Vec<Playlist>, Error> {
        let query = GetLibraryPlaylistsQuery;
        let parsed =
            offline::<GetLibraryPlaylistsQuery, Vec<NativePlaylist>>(&query, source)?;
        Ok(parsed.iter().map(library_playlist_to_playlist).collect())
    }

    pub fn parse_history(source: &str) -> Result<HistoryFeed, Error> {
        let query = GetHistoryQuery;
        let parsed = offline::<GetHistoryQuery, Vec<NativePeriod>>(&query, source)?;
        Ok(history_from_native(&parsed))
    }
}

fn history_from_native(periods: &[NativePeriod]) -> HistoryFeed {
    HistoryFeed {
        periods: periods
            .iter()
            .map(|p| HistoryPeriod {
                name: p.period_name.clone(),
                tracks: p.items.iter().filter_map(history_item_to_track).collect(),
            })
            .collect(),
    }
}

fn history_item_to_track(item: &HistoryItem) -> Option<Track> {
    match item {
        HistoryItem::Song(song) => Some(Track {
            identity: TrackIdentity::youtube(song.video_id.get_raw()),
            title: song.title.clone(),
            artists: song.artists.iter().map(|a| a.name.clone()).collect(),
            album: Some(song.album.name.clone()),
            duration_secs: parse_duration(&song.duration),
            thumbnail: mapping::best_thumbnail(&song.thumbnails),
            track_number: None,
            local_path: None,
            set_video_id: None,
        }),
        HistoryItem::Video(video) => Some(Track {
            identity: TrackIdentity::youtube(video.video_id.get_raw()),
            title: video.title.clone(),
            artists: vec![video.channel_name.clone()],
            album: None,
            duration_secs: parse_duration(&video.duration),
            thumbnail: mapping::best_thumbnail(&video.thumbnails),
            track_number: None,
            local_path: None,
            set_video_id: None,
        }),
        HistoryItem::Episode(_) | HistoryItem::UploadSong(_) => None,
    }
}

fn table_song_to_track(song: &TableListSong) -> Track {
    Track {
        identity: TrackIdentity::youtube(song.video_id.get_raw()),
        title: song.title.clone(),
        artists: song.artists.iter().map(|a| a.name.clone()).collect(),
        album: Some(song.album.name.clone()),
        duration_secs: parse_duration(&song.duration),
        thumbnail: mapping::best_thumbnail(&song.thumbnails),
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

fn library_playlist_to_playlist(playlist: &NativePlaylist) -> Playlist {
    Playlist {
        id: PlaylistId::new(playlist.playlist_id.get_raw()),
        kind: PlaylistKind::Remote,
        title: playlist.title.clone(),
        author: Some(playlist.author.clone()),
        description: None,
        thumbnail: mapping::best_thumbnail(&playlist.thumbnails),
        track_count: mapping::leading_count(&playlist.tracks),
        owned: false,
        privacy: None,
        author_id: playlist.author_id.as_ref().map(|id| id.get_raw().to_owned()),
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
