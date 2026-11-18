use madrigal_core::{
    Album, AlbumId, Artist, ArtistId, Playlist, PlaylistId, PlaylistKind, Track, TrackIdentity,
    parse_duration,
};
use ytmapi_rs::common::{Thumbnail, YoutubeID};
use ytmapi_rs::parse::{
    AlbumResult, AlbumSong, ArtistSong, GetAlbum, GetArtist, GetPlaylistDetails, PlaylistItem,
    SearchResultAlbum, SearchResultArtist, SearchResultSong, WatchPlaylistTrack,
};

use crate::pages::{AlbumPage, ArtistPage, PlaylistPage};

pub fn best_thumbnail(thumbnails: &[Thumbnail]) -> Option<String> {
    thumbnails
        .iter()
        .max_by_key(|t| t.width.saturating_mul(t.height.max(1)))
        .map(|t| t.url.clone())
}

pub fn leading_count(text: &str) -> Option<u32> {
    text.split_whitespace()
        .next()?
        .replace(',', "")
        .parse()
        .ok()
}

pub fn song_to_track(song: &SearchResultSong) -> Track {
    Track {
        identity: TrackIdentity::youtube(song.video_id.get_raw()),
        title: song.title.clone(),
        artists: vec![song.artist.clone()],
        album: song.album.as_ref().map(|a| a.name.clone()),
        duration_secs: parse_duration(&song.duration),
        thumbnail: best_thumbnail(&song.thumbnails),
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

pub fn artist_to_artist(artist: &SearchResultArtist) -> Artist {
    Artist {
        id: ArtistId::new(artist.browse_id.get_raw()),
        name: artist.artist.clone(),
        subscribers: artist.subscribers.clone(),
        thumbnail: best_thumbnail(&artist.thumbnails),
        description: None,
    }
}

pub fn album_to_album(album: &SearchResultAlbum) -> Album {
    Album {
        id: AlbumId::new(album.album_id.get_raw()),
        title: album.title.clone(),
        artists: vec![album.artist.clone()],
        artist_id: None,
        year: Some(album.year.clone()),
        thumbnail: best_thumbnail(&album.thumbnails),
        track_count: None,
        audio_playlist_id: None,
    }
}

pub fn artist_song_to_track(song: &ArtistSong) -> Track {
    Track {
        identity: TrackIdentity::youtube(song.video_id.get_raw()),
        title: song.title.clone(),
        artists: song.artists.iter().map(|a| a.name.clone()).collect(),
        album: Some(song.album.name.clone()),
        duration_secs: None,
        thumbnail: None,
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}

pub fn album_result_to_album(album: &AlbumResult) -> Album {
    Album {
        id: AlbumId::new(album.album_id.get_raw()),
        title: album.title.clone(),
        artists: Vec::new(),
        artist_id: None,
        year: Some(album.year.clone()),
        thumbnail: best_thumbnail(&album.thumbnails),
        track_count: None,
        audio_playlist_id: None,
    }
}

pub fn artist_page(page: &GetArtist) -> ArtistPage {
    let songs = page
        .top_releases
        .songs
        .as_ref()
        .map(|s| s.results.iter().map(artist_song_to_track).collect())
        .unwrap_or_default();
    let mut albums: Vec<Album> = page
        .top_releases
        .albums
        .as_ref()
        .map(|a| a.results.iter().map(album_result_to_album).collect())
        .unwrap_or_default();
    if let Some(singles) = page.top_releases.singles.as_ref() {
        albums.extend(singles.results.iter().map(album_result_to_album));
    }
    ArtistPage {
        artist: Artist {
            id: ArtistId::new(page.channel_id.get_raw()),
            name: page.name.clone(),
            subscribers: page.subscribers.clone(),
            thumbnail: best_thumbnail(&page.thumbnails),
            description: page.description.clone(),
        },
        songs,
        albums,
    }
}

pub fn album_page(browse_id: &str, album: &GetAlbum) -> AlbumPage {
    let artists: Vec<String> = album.artists.iter().map(|a| a.name.clone()).collect();
    let thumbnail = best_thumbnail(&album.thumbnails);
    let tracks = album
        .tracks
        .iter()
        .map(|t| album_song_to_track(browse_id, &album.title, &artists, thumbnail.clone(), t))
        .collect();
    AlbumPage {
        album: Album {
            id: AlbumId::new(browse_id),
            title: album.title.clone(),
            artists,
            artist_id: None,
            year: Some(album.year.clone()),
            thumbnail,
            track_count: album.track_count_text.as_deref().and_then(leading_count),
            audio_playlist_id: album.audio_playlist_id.as_ref().map(|id| id.get_raw().to_owned()),
        },
        tracks,
    }
}

fn album_song_to_track(
    browse_id: &str,
    album_title: &str,
    artists: &[String],
    thumbnail: Option<String>,
    song: &AlbumSong,
) -> Track {
    let _ = browse_id;
    Track {
        identity: TrackIdentity::youtube(song.video_id.get_raw()),
        title: song.title.clone(),
        artists: artists.to_vec(),
        album: Some(album_title.to_owned()),
        duration_secs: parse_duration(&song.duration),
        thumbnail,
        track_number: u32::try_from(song.track_no).ok(),
        local_path: None,
        set_video_id: None,
    }
}

pub fn playlist_page(details: &GetPlaylistDetails, items: &[PlaylistItem]) -> PlaylistPage {
    PlaylistPage {
        playlist: Playlist {
            id: PlaylistId::new(details.id.get_raw()),
            kind: PlaylistKind::Remote,
            title: details.title.clone(),
            author: Some(details.author.clone()),
            description: details.description.clone(),
            thumbnail: best_thumbnail(&details.thumbnails),
            track_count: Some(leading_count(&details.track_count_text).unwrap_or(0)),
            owned: false,
            privacy: details.privacy.as_ref().map(|p| p.to_string()),
            author_id: None,
        },
        tracks: items.iter().filter_map(playlist_item_to_track).collect(),
    }
}

fn playlist_item_to_track(item: &PlaylistItem) -> Option<Track> {
    match item {
        PlaylistItem::Song(song) => Some(Track {
            identity: TrackIdentity::youtube(song.video_id.get_raw()),
            title: song.title.clone(),
            artists: song.artists.iter().map(|a| a.name.clone()).collect(),
            album: Some(song.album.name.clone()),
            duration_secs: parse_duration(&song.duration),
            thumbnail: best_thumbnail(&song.thumbnails),
            track_number: u32::try_from(song.track_no).ok(),
            local_path: None,
            set_video_id: None,
        }),
        PlaylistItem::Video(video) => Some(Track {
            identity: TrackIdentity::youtube(video.video_id.get_raw()),
            title: video.title.clone(),
            artists: vec![video.channel_name.clone()],
            album: None,
            duration_secs: parse_duration(&video.duration),
            thumbnail: best_thumbnail(&video.thumbnails),
            track_number: u32::try_from(video.track_no).ok(),
            local_path: None,
            set_video_id: None,
        }),
        PlaylistItem::Episode(_) | PlaylistItem::UploadSong(_) => None,
    }
}

pub fn watch_track_to_track(track: &WatchPlaylistTrack) -> Track {
    Track {
        identity: TrackIdentity::youtube(track.video_id.get_raw()),
        title: track.title.clone(),
        artists: vec![track.author.clone()],
        album: None,
        duration_secs: parse_duration(&track.duration),
        thumbnail: best_thumbnail(&track.thumbnails),
        track_number: None,
        local_path: None,
        set_video_id: None,
    }
}
