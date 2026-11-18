use std::time::Duration;

use crate::auth::Token;
use crate::models::{
    SpotifyAlbumRef, SpotifyArtistRef, SpotifyImage, SpotifyPlaylist, SpotifyPlaylistItem,
    SpotifyTrack,
};
use crate::Error;

pub const HASH_LIBRARY_V3: &str =
    "973e511ca44261fda7eebac8b653155e7caee3675abb4fb110cc1b8c78b091c3";
pub const HASH_FETCH_PLAYLIST: &str =
    "346811f856fb0b7e4f6c59f8ebea78dd081c6e2fb01b77c954b26259f5fc6763";
pub const HASH_PROFILE: &str =
    "53bcb064f6cd18c23f752bc324a791194d20df612d8e1239c735144ab0399ced";

const PAGE_LIMIT: i64 = 50;

#[derive(Debug, Clone)]
pub struct GqlEndpoints {
    pub query_url: String,
    pub rest_base: String,
}

impl Default for GqlEndpoints {
    fn default() -> GqlEndpoints {
        GqlEndpoints {
            query_url: "https://api-partner.spotify.com/pathfinder/v2/query".to_owned(),
            rest_base: "https://api.spotify.com/v1/".to_owned(),
        }
    }
}

pub struct GqlClient {
    http: reqwest::blocking::Client,
    endpoints: GqlEndpoints,
}

impl GqlClient {
    pub fn new() -> Result<GqlClient, Error> {
        GqlClient::with_endpoints(GqlEndpoints::default())
    }

    pub fn with_endpoints(endpoints: GqlEndpoints) -> Result<GqlClient, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(5))
            .no_proxy()
            .build()
            .map_err(|err| Error::Transport(err.to_string()))?;
        Ok(GqlClient { http, endpoints })
    }

    pub fn endpoints(&self) -> &GqlEndpoints {
        &self.endpoints
    }

    fn query(
        &self,
        token: &Token,
        operation: &str,
        hash: &str,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value, Error> {
        let body = serde_json::json!({
            "variables": variables,
            "operationName": operation,
            "extensions": {"persistedQuery": {"version": 1, "sha256Hash": hash}},
        });
        let response = self
            .http
            .post(&self.endpoints.query_url)
            .header("Authorization", format!("Bearer {}", token.access_token))
            .header("app-platform", "WebPlayer")
            .header("Origin", "https://open.spotify.com")
            .header("Referer", "https://open.spotify.com/")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .map_err(|err| Error::Transport(err.to_string()))?;
        let status = response.status().as_u16();
        if status == 401 {
            return Err(Error::Auth("token expired or invalid".to_owned()));
        }
        if status == 429 {
            return Err(Error::RateLimited(2));
        }
        if status == 412 {
            return Err(Error::HashRotated(operation.to_owned()));
        }
        if !(200..300).contains(&status) {
            return Err(Error::Status(status));
        }
        let json: serde_json::Value = response
            .json()
            .map_err(|err| Error::Protocol(err.to_string()))?;
        if let Some(errors) = json.get("errors").and_then(|entry| entry.as_array()) {
            if let Some(message) = errors.first().and_then(|entry| entry.get("message")).and_then(|entry| entry.as_str()) {
                if message.contains("PersistedQueryNotFound") {
                    return Err(Error::HashRotated(operation.to_owned()));
                }
                return Err(Error::Protocol(format!("{operation}: {message}")));
            }
        }
        Ok(json)
    }

    fn rest_me(&self, token: &Token) -> Result<(String, Option<String>), Error> {
        let url = format!("{}me", self.endpoints.rest_base);
        let response = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", token.access_token))
            .header("app-platform", "WebPlayer")
            .header("Origin", "https://open.spotify.com")
            .header("Referer", "https://open.spotify.com/")
            .send()
            .map_err(|err| Error::Transport(err.to_string()))?;
        if response.status().as_u16() == 401 {
            return Err(Error::Auth("token expired or invalid".to_owned()));
        }
        if !response.status().is_success() {
            return Err(Error::Status(response.status().as_u16()));
        }
        let json: serde_json::Value = response
            .json()
            .map_err(|err| Error::Protocol(err.to_string()))?;
        let name = json
            .get("display_name")
            .and_then(|entry| entry.as_str())
            .unwrap_or_default()
            .to_owned();
        let avatar = json
            .get("images")
            .and_then(|entry| entry.as_array())
            .and_then(|images| images.first())
            .and_then(|image| image.get("url"))
            .and_then(|entry| entry.as_str())
            .map(str::to_owned);
        Ok((name, avatar))
    }

    pub fn profile(&self, token: &Token) -> Result<(String, Option<String>), Error> {
        match self.query(token, "profileAttributes", HASH_PROFILE, serde_json::json!({})) {
            Ok(json) => parse_profile(&json).ok_or_else(|| Error::Protocol(
                "invalid profileAttributes response".to_owned(),
            )),
            Err(_) => self.rest_me(token),
        }
    }

    pub fn my_playlists(&self, token: &Token) -> Result<Vec<SpotifyPlaylist>, Error> {
        let mut playlists = Vec::new();
        let mut offset = 0;
        loop {
            let variables = serde_json::json!({
                "filters": ["Playlists"],
                "order": null,
                "textFilter": "",
                "features": ["LIKED_SONGS", "YOUR_EPISODES_V2", "PRERELEASES", "EVENTS"],
                "limit": PAGE_LIMIT,
                "offset": offset,
                "flatten": true,
                "expandedFolders": [],
                "folderUri": null,
                "includeFoldersWhenFlattening": false,
            });
            let json = self.query(token, "libraryV3", HASH_LIBRARY_V3, variables)?;
            let (mut page, total) = parse_library_page(&json)?;
            if page.is_empty() {
                break;
            }
            for playlist in &mut page {
                if playlist.track_total.is_none() {
                    playlist.track_total = self.playlist_track_total(token, &playlist.id).ok();
                }
            }
            let received = page.len() as i64;
            playlists.append(&mut page);
            offset += received;
            if offset >= total || received < PAGE_LIMIT {
                break;
            }
        }
        Ok(playlists)
    }

    fn playlist_track_total(&self, token: &Token, playlist_id: &str) -> Result<u32, Error> {
        let (_, page) = self.fetch_playlist_page(token, playlist_id, 0, 1)?;
        u32::try_from(page.max(0)).map_err(|_| Error::Protocol("track total overflow".to_owned()))
    }

    pub fn playlist_detail(&self, token: &Token, playlist_id: &str) -> Result<SpotifyPlaylist, Error> {
        let variables = serde_json::json!({
            "uri": format!("spotify:playlist:{playlist_id}"),
            "offset": 0,
            "limit": 25,
            "enableWatchFeedEntrypoint": true,
        });
        let json = self.query(token, "fetchPlaylist", HASH_FETCH_PLAYLIST, variables)?;
        parse_playlist_detail(playlist_id, &json)
    }

    pub fn playlist_tracks(
        &self,
        token: &Token,
        playlist_id: &str,
    ) -> Result<Vec<SpotifyTrack>, Error> {
        let mut tracks = Vec::new();
        let mut offset = 0;
        loop {
            let (page, total) = self.fetch_playlist_page(token, playlist_id, offset, PAGE_LIMIT)?;
            if page.is_empty() {
                break;
            }
            let received = page.len() as i64;
            tracks.extend(page.into_iter().filter_map(|item| item.track));
            offset += received;
            if offset >= total || received < PAGE_LIMIT {
                break;
            }
        }
        Ok(tracks)
    }

    fn fetch_playlist_page(
        &self,
        token: &Token,
        playlist_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<SpotifyPlaylistItem>, i64), Error> {
        let variables = serde_json::json!({
            "uri": format!("spotify:playlist:{playlist_id}"),
            "offset": offset,
            "limit": limit,
            "enableWatchFeedEntrypoint": false,
        });
        let json = self.query(token, "fetchPlaylist", HASH_FETCH_PLAYLIST, variables)?;
        parse_playlist_tracks(&json)
    }
}

fn text(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|entry| entry.as_str())
        .map(str::to_owned)
}

fn integer(value: &serde_json::Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|entry| entry.as_i64())
}

fn object<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    value.get(key).filter(|entry| entry.is_object())
}

fn images_from_sources(value: Option<&serde_json::Value>) -> Vec<SpotifyImage> {
    value
        .and_then(|entry| entry.as_array())
        .map(|sources| {
            sources
                .iter()
                .filter_map(|source| {
                    Some(SpotifyImage {
                        url: text(source, "url")?,
                        height: source
                            .get("height")
                            .and_then(|entry| entry.as_u64())
                            .map(|height| height as u32),
                        width: source
                            .get("width")
                            .and_then(|entry| entry.as_u64())
                            .map(|width| width as u32),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn playlist_images(value: Option<&serde_json::Value>) -> Vec<SpotifyImage> {
    value
        .and_then(|entry| entry.get("items"))
        .and_then(|entry| entry.as_array())
        .map(|groups| {
            groups
                .iter()
                .flat_map(|group| images_from_sources(group.get("sources")))
                .collect()
        })
        .unwrap_or_default()
}

fn playlist_track_total(data: &serde_json::Value) -> Option<u32> {
    for key in ["content", "contents", "tracks", "tracksV2"] {
        if let Some(total) = object(data, key)
            .and_then(|section| integer(section, "totalCount"))
            .filter(|total| *total >= 0)
        {
            return Some(total as u32);
        }
    }
    for key in ["totalCount", "trackCount", "numTracks"] {
        if let Some(total) = integer(data, key).filter(|total| *total >= 0) {
            return Some(total as u32);
        }
    }
    None
}

fn simple_artist(value: &serde_json::Value) -> Option<SpotifyArtistRef> {
    let uri = text(value, "uri")?;
    Some(SpotifyArtistRef {
        id: uri.rsplit(':').next().unwrap_or_default().to_owned(),
        name: object(value, "profile")
            .and_then(|profile| text(profile, "name"))
            .unwrap_or_default(),
        })
}

fn track_duration_ms(track: &serde_json::Value) -> i64 {
    if let Some(total) = object(track, "duration")
        .and_then(|duration| integer(duration, "totalMilliseconds"))
        .filter(|total| *total > 0)
    {
        return total;
    }
    for key in ["durationMs", "duration_ms"] {
        if let Some(total) = integer(track, key).filter(|total| *total > 0) {
            return total;
        }
    }
    if let Some(seconds) = integer(track, "duration").filter(|total| *total > 0) {
        return seconds * 1000;
    }
    0
}

fn parse_track(track: &serde_json::Value, uri_override: Option<String>) -> SpotifyTrack {
    let uri = uri_override
        .or_else(|| text(track, "uri"))
        .or_else(|| text(track, "_uri"))
        .unwrap_or_default();
    let artists = object(track, "artists")
        .and_then(|entry| entry.get("items"))
        .and_then(|entry| entry.as_array())
        .map(|items| items.iter().filter_map(simple_artist).collect())
        .unwrap_or_default();
    let album = object(track, "albumOfTrack").map(|data| {
        let uri = text(data, "uri").unwrap_or_default();
        SpotifyAlbumRef {
            id: uri.rsplit(':').next().unwrap_or_default().to_owned(),
            name: text(data, "name").unwrap_or_default(),
            images: images_from_sources(object(data, "coverArt").and_then(|cover| cover.get("sources"))),
        }
    });
    SpotifyTrack {
        id: uri.rsplit(':').next().unwrap_or_default().to_owned(),
        name: text(track, "name").unwrap_or_default(),
        artists,
        album,
        duration_ms: track_duration_ms(track),
        is_local: track
            .get("is_local")
            .and_then(|entry| entry.as_bool())
            .unwrap_or(false),
        explicit: track
            .get("explicit")
            .and_then(|entry| entry.as_bool())
            .unwrap_or(false),
        uri: if uri.is_empty() { None } else { Some(uri) },
    }
}

fn parse_playlist_wrapper(wrapper: &serde_json::Value) -> Option<SpotifyPlaylist> {
    if text(wrapper, "__typename")?.as_str() != "PlaylistResponseWrapper" {
        return None;
    }
    let data = object(wrapper, "data")?;
    if text(data, "__typename")?.as_str() != "Playlist" {
        return None;
    }
    let uri = text(wrapper, "_uri")?;
    let owner = object(data, "ownerV2").and_then(|entry| object(entry, "data"));
    let owner_uri = owner.and_then(|entry| text(entry, "uri")).unwrap_or_default();
    Some(SpotifyPlaylist {
        id: uri.rsplit(':').next().unwrap_or_default().to_owned(),
        name: text(data, "name").unwrap_or_default(),
        description: text(data, "description"),
        images: playlist_images(data.get("images")),
        owner_id: owner
            .and_then(|entry| text(entry, "id"))
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| {
                owner_uri.rsplit(':').next().unwrap_or_default().to_owned()
            }),
        owner_name: owner.and_then(|entry| text(entry, "name")),
        track_total: playlist_track_total(data),
        uri: Some(uri),
    })
}

pub fn parse_library_page(json: &serde_json::Value) -> Result<(Vec<SpotifyPlaylist>, i64), Error> {
    let library = json
        .get("data")
        .and_then(|entry| entry.get("me"))
        .and_then(|entry| entry.get("libraryV3"))
        .ok_or_else(|| Error::Protocol("invalid libraryV3 response".to_owned()))?;
    let total = integer(library, "totalCount").unwrap_or(0);
    let items = library
        .get("items")
        .and_then(|entry| entry.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("item"))
                .filter_map(parse_playlist_wrapper)
                .collect()
        })
        .unwrap_or_default();
    Ok((items, total))
}

pub fn parse_playlist_detail(
    playlist_id: &str,
    json: &serde_json::Value,
) -> Result<SpotifyPlaylist, Error> {
    let playlist = json
        .get("data")
        .and_then(|entry| entry.get("playlistV2"))
        .ok_or_else(|| Error::Protocol("invalid fetchPlaylist response".to_owned()))?;
    let owner = object(playlist, "ownerV2").and_then(|entry| object(entry, "data"));
    let owner_uri = owner.and_then(|entry| text(entry, "uri")).unwrap_or_default();
    let images = playlist
        .get("images")
        .and_then(|entry| entry.get("items"))
        .and_then(|entry| entry.as_array())
        .and_then(|items| items.first())
        .map(|group| images_from_sources(group.get("sources")))
        .unwrap_or_default();
    Ok(SpotifyPlaylist {
        id: playlist_id.to_owned(),
        name: text(playlist, "name").unwrap_or_default(),
        description: text(playlist, "description"),
        images,
        owner_id: owner
            .and_then(|entry| text(entry, "id"))
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| {
                owner_uri.rsplit(':').next().unwrap_or_default().to_owned()
            }),
        owner_name: owner.and_then(|entry| text(entry, "name")),
        track_total: playlist_track_total(playlist),
        uri: Some(format!("spotify:playlist:{playlist_id}")),
    })
}

pub fn parse_playlist_tracks(
    json: &serde_json::Value,
) -> Result<(Vec<SpotifyPlaylistItem>, i64), Error> {
    let content = json
        .get("data")
        .and_then(|entry| entry.get("playlistV2"))
        .and_then(|entry| entry.get("content"))
        .ok_or_else(|| Error::Protocol("no content in fetchPlaylist response".to_owned()))?;
    let total = integer(content, "totalCount").unwrap_or(0);
    let items = content
        .get("items")
        .and_then(|entry| entry.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let wrapper = object(entry, "itemV2")?;
                    let data = object(wrapper, "data")?;
                    let uri = text(wrapper, "_uri").or_else(|| text(wrapper, "uri"));
                    Some(SpotifyPlaylistItem {
                        uid: text(entry, "uid").or_else(|| text(wrapper, "uid")),
                        track: Some(parse_track(data, uri)),
                        is_local: entry
                            .get("is_local")
                            .and_then(|flag| flag.as_bool())
                            .unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok((items, total))
}

fn parse_profile(json: &serde_json::Value) -> Option<(String, Option<String>)> {
    let profile = json
        .get("data")?
        .get("me")?
        .get("profile")?;
    Some((
        text(profile, "name").unwrap_or_default(),
        images_from_sources(object(profile, "avatar").and_then(|avatar| avatar.get("sources")))
            .into_iter()
            .next()
            .map(|image| image.url),
    ))
}
