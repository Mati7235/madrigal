use madrigal_core::{
    Album, AlbumId, Artist, ArtistId, Episode, EpisodeId, Event, Playlist, PlaylistId,
    PlaylistKind, Podcast, PodcastId, Recognition, SearchHistoryItem, Track, TrackIdentity,
    TrackSource, format_duration, parse_duration,
};

fn isolate_xdg() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for (key, sub) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let path = dir.path().join(sub);
        std::fs::create_dir_all(&path).expect("xdg subdir");
        std::env::set_var(key, &path);
    }
    dir
}

fn roundtrip<T>(value: &T) -> T
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(value).expect("serializa");
    let back: T = serde_json::from_str(&json).expect("deserializa");
    assert_eq!(&back, value);
    back
}

fn sample_track() -> Track {
    Track {
        identity: TrackIdentity::youtube("dQw4w9WgXcQ"),
        title: "Never Gonna Give You Up".to_owned(),
        artists: vec!["Rick Astley".to_owned()],
        album: Some("Whenever You Need Somebody".to_owned()),
        duration_secs: Some(212),
        thumbnail: Some("https://img/rick.jpg".to_owned()),
        track_number: Some(1),
        local_path: None,
        set_video_id: None,
    }
}

#[test]
fn identidad_unificada_no_vacia() {
    let _xdg = isolate_xdg();
    for identity in [
        TrackIdentity::youtube("dQw4w9WgXcQ"),
        TrackIdentity::tidal("12345"),
        TrackIdentity::local("/musica/tema.flac"),
    ] {
        assert!(!identity.is_empty());
        assert_eq!(
            TrackIdentity::parse_prefixed(&identity.to_storage_string()),
            identity
        );
        roundtrip(&identity);
    }
    assert!(TrackIdentity::new(TrackSource::Youtube, "").is_empty());
    assert!(!TrackIdentity::new(TrackSource::Youtube, "").is_remote());
    assert!(!TrackIdentity::local("/a.flac").is_remote());
    assert!(TrackIdentity::tidal("1").is_remote());
}

#[test]
fn modelos_principales_roundtrip() {
    let _xdg = isolate_xdg();
    let track = sample_track();
    assert!(track.is_valid());
    assert_eq!(track.source(), TrackSource::Youtube);
    assert!(track.is_remote());
    assert_eq!(track.artist_text(), "Rick Astley");
    roundtrip(&track);

    roundtrip(&Album {
        id: AlbumId::new("MPRE_test"),
        title: "Whenever You Need Somebody".to_owned(),
        artists: vec!["Rick Astley".to_owned()],
        artist_id: Some(ArtistId::new("UCtest")),
        year: Some("1987".to_owned()),
        thumbnail: None,
        track_count: Some(10),
        audio_playlist_id: Some("OLAKtest".to_owned()),
    });

    roundtrip(&Artist {
        id: ArtistId::new("UCtest"),
        name: "Rick Astley".to_owned(),
        subscribers: Some("1M".to_owned()),
        thumbnail: None,
        description: None,
    });

    for kind in [
        PlaylistKind::Local,
        PlaylistKind::Remote,
        PlaylistKind::Auto,
        PlaylistKind::Spotify,
    ] {
        roundtrip(&kind);
    }
    roundtrip(&Playlist {
        id: PlaylistId::new("PLtest"),
        kind: PlaylistKind::Spotify,
        title: "Importada".to_owned(),
        author: Some("yo".to_owned()),
        description: None,
        thumbnail: None,
        track_count: Some(25),
        owned: false,
        privacy: None,
        author_id: None,
    });

    roundtrip(&Podcast {
        id: PodcastId::new("pc1"),
        title: "Charlas".to_owned(),
        author: Some("Ana".to_owned()),
        description: None,
        thumbnail: None,
    });
    roundtrip(&Episode {
        id: EpisodeId::new("ep7"),
        podcast_id: PodcastId::new("pc1"),
        title: "Episodio 7".to_owned(),
        description: None,
        duration_secs: Some(1800),
        thumbnail: None,
        published_at: Some(1_700_000_000),
        position_secs: Some(300),
    });
    roundtrip(&Event {
        track: TrackIdentity::youtube("dQw4w9WgXcQ"),
        played_at: 1_700_000_000,
        duration_secs: Some(200),
        completed: true,
    });
    roundtrip(&SearchHistoryItem {
        query: "rick astley".to_owned(),
        searched_at: 1_700_000_000,
    });
    roundtrip(&Recognition {
        title: "Never Gonna Give You Up".to_owned(),
        artists: vec!["Rick Astley".to_owned()],
        album: None,
        score: 0.93,
        track: Some(TrackIdentity::youtube("dQw4w9WgXcQ")),
    });
}

#[test]
fn invariantes_rechazan_datos_vacios() {
    let _xdg = isolate_xdg();
    assert!(!sample_track().identity.is_empty());
    assert!(!SearchHistoryItem {
        query: "   ".to_owned(),
        searched_at: 0,
    }
    .is_valid());
    assert!(!Event {
        track: TrackIdentity::youtube("x"),
        played_at: 0,
        duration_secs: None,
        completed: false,
    }
    .is_valid());
    assert!(!Episode {
        id: EpisodeId::new("ep"),
        podcast_id: PodcastId::new("pc"),
        title: "t".to_owned(),
        description: None,
        duration_secs: Some(100),
        thumbnail: None,
        published_at: None,
        position_secs: Some(101),
    }
    .is_valid());
    assert!(!Recognition {
        title: "t".to_owned(),
        artists: vec![],
        album: None,
        score: 2.0,
        track: None,
    }
    .score_in_range());
    assert_eq!(format_duration(Some(212)), "3:32");
    assert_eq!(parse_duration("3:32"), Some(212));
    assert_eq!(parse_duration("nope"), None);
}

#[test]
fn snapshot_json_vive_en_tempdir() {
    let xdg = isolate_xdg();
    let path = xdg.path().join("data").join("track.json");
    let track = sample_track();
    std::fs::write(&path, serde_json::to_string(&track).expect("serializa"))
        .expect("escribe snapshot");
    let raw = std::fs::read_to_string(&path).expect("lee snapshot");
    let back: Track = serde_json::from_str(&raw).expect("deserializa");
    assert_eq!(back, track);
}
