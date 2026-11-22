use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use madrigal_core::{Album, Artist, Episode, Playlist, Podcast, Track};
use madrigal_i18n::Lang;
use madrigal_innertube::{
    AlbumPage, ArtistPage, BrowseItem, BrowsePage, CatalogClient, ChartsPage, HomeFeed, HomeItem,
    MoodListing, NewReleases, PlaylistPage,
};

use crate::{artwork, CatalogItem, CatalogSection, DetailKind, DetailRef, MainShell, MoodEntry};

pub type CatalogResult<T> = Result<T, madrigal_innertube::Error>;

fn item(
    kind: DetailKind,
    id: &str,
    title: &str,
    subtitle: &str,
    duration: &str,
    playable: bool,
) -> CatalogItem {
    CatalogItem {
        kind,
        id: id.into(),
        title: title.into(),
        subtitle: subtitle.into(),
        duration: duration.into(),
        playable,
        artist: SharedString::new(),
        album: SharedString::new(),
        thumbnail: SharedString::new(),
        cover: Default::default(),
        duration_secs: 0,
    }
}

pub fn track_item(track: &Track) -> CatalogItem {
    let mut subtitle = track.artist_text();
    if let Some(album) = track.album.as_deref() {
        if !subtitle.is_empty() {
            subtitle.push_str(" · ");
        }
        subtitle.push_str(album);
    }
    CatalogItem {
        artist: track.artist_text().into(),
        album: track.album.as_deref().unwrap_or_default().into(),
        thumbnail: track.thumbnail.as_deref().unwrap_or_default().into(),
        cover: Default::default(),
        duration_secs: track.duration_secs.unwrap_or_default() as i32,
        ..item(
            DetailKind::Album,
            track.identity.id.as_str(),
            track.title.as_str(),
            subtitle.as_str(),
            madrigal_core::format_duration(track.duration_secs).as_str(),
            true,
        )
    }
}

pub fn album_item(album: &Album) -> CatalogItem {
    CatalogItem {
        thumbnail: album.thumbnail.as_deref().unwrap_or_default().into(),
        ..item(
            DetailKind::Album,
            album.id.as_str(),
            album.title.as_str(),
            album.artist_text().as_str(),
            album.year.as_deref().unwrap_or_default(),
            false,
        )
    }
}

pub fn artist_item(artist: &Artist) -> CatalogItem {
    CatalogItem {
        thumbnail: artist.thumbnail.as_deref().unwrap_or_default().into(),
        ..item(
            DetailKind::Artist,
            artist.id.as_str(),
            artist.name.as_str(),
            artist.subscribers.as_deref().unwrap_or_default(),
            "",
            false,
        )
    }
}

pub fn playlist_item(playlist: &Playlist) -> CatalogItem {
    let mut subtitle = playlist.author.clone().unwrap_or_default();
    if let Some(count) = playlist.track_count {
        if !subtitle.is_empty() {
            subtitle.push_str(" · ");
        }
        subtitle.push_str(&count.to_string());
    }
    CatalogItem {
        thumbnail: playlist.thumbnail.as_deref().unwrap_or_default().into(),
        ..item(
            DetailKind::Playlist,
            playlist.id.as_str(),
            playlist.title.as_str(),
            subtitle.as_str(),
            "",
            false,
        )
    }
}

pub fn episode_item(episode: &Episode) -> CatalogItem {
    item(
        DetailKind::Podcast,
        episode.id.as_str(),
        episode.title.as_str(),
        episode.description.as_deref().unwrap_or_default(),
        madrigal_core::format_duration(episode.duration_secs).as_str(),
        true,
    )
}

fn browse_item(entry: &BrowseItem) -> CatalogItem {
    match entry {
        BrowseItem::Track(track) => track_item(track),
        BrowseItem::Album(album) => album_item(album),
        BrowseItem::Artist(artist) => artist_item(artist),
        BrowseItem::Playlist(playlist) => playlist_item(playlist),
    }
}

fn home_item(entry: &HomeItem) -> CatalogItem {
    match entry {
        HomeItem::Track(track) => track_item(track),
        HomeItem::Album(album) => album_item(album),
        HomeItem::Artist(artist) => artist_item(artist),
        HomeItem::Playlist(playlist) => playlist_item(playlist),
    }
}

fn section_of(title: &str, items: Vec<CatalogItem>) -> CatalogSection {
    CatalogSection {
        title: title.into(),
        items: ModelRc::new(VecModel::from_slice(&items)),
    }
}

pub fn home_sections(feed: &HomeFeed) -> Vec<CatalogSection> {
    feed.sections
        .iter()
        .map(|section| {
            section_of(
                section.title.as_str(),
                section.items.iter().map(home_item).collect(),
            )
        })
        .collect()
}

pub fn quick_pick_items(feed: &HomeFeed) -> Vec<CatalogItem> {
    feed.sections
        .iter()
        .find(|section| section.title.to_lowercase().contains("quick picks"))
        .map(|section| {
            section
                .items
                .iter()
                .filter_map(|entry| match entry {
                    HomeItem::Track(track) => Some(track_item(track)),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn home_chips(feed: &HomeFeed, show: bool, lang: madrigal_i18n::Lang) -> Vec<SharedString> {
    if !show {
        return Vec::new();
    }
    let mut chips = vec![SharedString::from(madrigal_i18n::tr(
        lang,
        "catalog.filter.all",
    ))];
    chips.extend(
        feed.sections
            .iter()
            .map(|s| SharedString::from(s.title.as_str())),
    );
    chips
}

pub fn apply_home(shell: &MainShell, feed: &HomeFeed) {
    shell.set_home_sections(ModelRc::new(VecModel::from_slice(&home_sections(feed))));
    shell.set_home_quick_picks(ModelRc::new(VecModel::from_slice(&quick_pick_items(
        feed,
    ))));
    shell.set_home_current_chip(0);
    shell.set_home_selected_item(-1);
    artwork::resolve_home(shell);
}

pub fn apply_home_chips(
    shell: &MainShell,
    feed: &HomeFeed,
    content: &madrigal_settings::Content,
    lang: madrigal_i18n::Lang,
) {
    shell.set_home_chips(ModelRc::new(VecModel::from_slice(&home_chips(
        feed,
        content.show_home_category_chips,
        lang,
    ))));
}

pub fn apply_home_chip(shell: &MainShell, chip: i32, feed: &HomeFeed) {
    shell.set_home_current_chip(chip);
    let mut sections = home_sections(feed);
    if chip > 0 {
        let index = (chip - 1) as usize;
        sections = sections.into_iter().skip(index).take(1).collect();
    }
    shell.set_home_sections(ModelRc::new(VecModel::from_slice(&sections)));
    artwork::resolve_home(shell);
}

pub fn apply_pins(shell: &MainShell, ids: &[String], tracks: &[Track]) {
    let mut items = Vec::new();
    for id in ids {
        if let Some(track) = tracks
            .iter()
            .find(|t| t.identity.id.as_str() == id.as_str())
        {
            items.push(track_item(track));
        }
    }
    shell.set_home_pins(ModelRc::new(VecModel::from_slice(&items)));
}

pub fn load_home_fixture(shell: &MainShell, source: &str) -> CatalogResult<HomeFeed> {
    let feed = CatalogClient::parse_home(source)?;
    apply_home(shell, &feed);
    Ok(feed)
}

pub fn song_results(tracks: &[Track]) -> Vec<CatalogItem> {
    tracks.iter().map(track_item).collect()
}

fn artist_search_item(artist: &Artist, lang: Lang) -> CatalogItem {
    let subtitle = artist
        .subscribers
        .as_deref()
        .and_then(|raw| subscriber_label(raw, lang))
        .unwrap_or_default();
    CatalogItem {
        thumbnail: artist.thumbnail.as_deref().unwrap_or_default().into(),
        ..item(
            DetailKind::Artist,
            artist.id.as_str(),
            artist.name.as_str(),
            subtitle.as_str(),
            "",
            false,
        )
    }
}

fn is_channel(artist: &Artist) -> bool {
    artist
        .subscribers
        .as_deref()
        .is_some_and(|raw| raw.ends_with("subscribers"))
}

pub fn artist_results(artists: &[Artist], lang: Lang) -> Vec<CatalogItem> {
    let mut ordered: Vec<&Artist> = artists.iter().collect();
    ordered.sort_by_key(|artist| is_channel(artist));
    ordered
        .into_iter()
        .map(|artist| artist_search_item(artist, lang))
        .collect()
}

pub const MIXED_PER_KIND: usize = 8;

pub fn mixed_results(
    songs: &[Track],
    artists: &[Artist],
    albums: &[Album],
    lang: Lang,
) -> Vec<CatalogItem> {
    let mut items = Vec::new();
    items.extend(
        artists
            .iter()
            .take(MIXED_PER_KIND)
            .map(|artist| artist_search_item(artist, lang)),
    );
    items.extend(albums.iter().take(MIXED_PER_KIND).map(album_item));
    items.extend(songs.iter().take(MIXED_PER_KIND).map(track_item));
    items
}

pub fn album_results(albums: &[Album]) -> Vec<CatalogItem> {
    albums.iter().map(album_item).collect()
}

pub fn apply_search_results(shell: &MainShell, items: Vec<CatalogItem>) {
    apply_search_partial(shell, items);
    shell.set_search_done(true);
}

pub fn apply_search_partial(shell: &MainShell, items: Vec<CatalogItem>) {
    shell.set_search_results(ModelRc::new(VecModel::from_slice(&items)));
    shell.set_search_error(false);
    artwork::resolve_search(shell);
}

pub fn apply_suggestions(shell: &MainShell, query: &str, all: &[String]) {
    let lowered = query.to_lowercase();
    let hits: Vec<SharedString> = all
        .iter()
        .filter(|s| s.to_lowercase().starts_with(&lowered))
        .take(8)
        .map(SharedString::from)
        .collect();
    shell.set_search_suggestions(ModelRc::new(VecModel::from_slice(&hits)));
    shell.set_search_suggestion_index(-1);
}

pub fn load_search_fixture(
    shell: &MainShell,
    songs_source: &str,
    artists_source: &str,
    albums_source: &str,
) -> CatalogResult<(Vec<Track>, Vec<Artist>, Vec<Album>)> {
    let songs = CatalogClient::parse_search_songs(songs_source)?;
    let artists = CatalogClient::parse_search_artists(artists_source)?;
    let albums = CatalogClient::parse_search_albums(albums_source)?;
    apply_search_results(shell, song_results(&songs));
    Ok((songs, artists, albums))
}

pub fn load_suggestions_fixture(shell: &MainShell, source: &str) -> CatalogResult<Vec<String>> {
    let suggestions = CatalogClient::parse_suggestions(source)?;
    apply_suggestions(shell, shell.get_search_query().as_str(), &suggestions);
    Ok(suggestions)
}

pub fn apply_moods(shell: &MainShell, listing: &MoodListing) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for (index, entry) in listing
        .sections
        .iter()
        .flat_map(|section| section.entries.iter())
        .enumerate()
    {
        let item = MoodEntry {
            title: entry.title.as_str().into(),
            params: entry.params.as_str().into(),
        };
        if index % 2 == 0 {
            left.push(item);
        } else {
            right.push(item);
        }
    }
    shell.set_explore_moods_left(ModelRc::new(VecModel::from_slice(&left)));
    shell.set_explore_moods_right(ModelRc::new(VecModel::from_slice(&right)));
}

pub fn apply_releases(shell: &MainShell, releases: &NewReleases) {
    shell.set_explore_releases_albums(ModelRc::new(VecModel::from_slice(&album_results(
        &releases.albums,
    ))));
    shell.set_explore_releases_videos(ModelRc::new(VecModel::from_slice(&song_results(
        &releases.videos,
    ))));
    artwork::resolve_releases(shell);
}

pub fn apply_charts_entry(shell: &MainShell, charts: &ChartsPage) {
    shell.set_explore_charts_id(format!("charts:{}", charts.country).into());
}

pub fn apply_browse(shell: &MainShell, browse: &BrowsePage) {
    let sections: Vec<CatalogSection> = browse
        .sections
        .iter()
        .map(|section| {
            section_of(
                section.title.as_str(),
                section.items.iter().map(browse_item).collect(),
            )
        })
        .collect();
    shell.set_explore_browse_sections(ModelRc::new(VecModel::from_slice(&sections)));
    artwork::resolve_browse(shell);
}

pub fn load_explore_fixtures(
    shell: &MainShell,
    moods_source: &str,
    explore_source: &str,
    charts_source: &str,
) -> CatalogResult<()> {
    apply_moods(shell, &CatalogClient::parse_mood_listing(moods_source)?);
    apply_releases(
        shell,
        &CatalogClient::parse_new_releases(explore_source)?,
    );
    apply_browse(shell, &CatalogClient::parse_browse(explore_source)?);
    apply_charts_entry(shell, &CatalogClient::parse_charts(charts_source)?);
    Ok(())
}

fn clear_detail(shell: &MainShell, kind: DetailKind, id: &str, title: &str) {
    shell.set_detail_ref(DetailRef {
        kind,
        id: id.into(),
        title: title.into(),
    });
    shell.set_detail_title(title.into());
    shell.set_detail_subtitle("".into());
    shell.set_detail_description("".into());
    shell.set_detail_desc_long(false);
    shell.set_detail_cover(slint::Image::default());
    shell.set_detail_loading(false);
    shell.set_detail_error("".into());
    shell.set_detail_tracks(ModelRc::new(VecModel::from_slice(&[])));
    shell.set_detail_albums(ModelRc::new(VecModel::from_slice(&[])));
    shell.set_detail_sections(ModelRc::new(VecModel::from_slice(&[])));
    shell.set_detail_tab(0);
}

pub fn begin_detail_load(shell: &MainShell, reference: &DetailRef) {
    shell.set_detail_ref(reference.clone());
    shell.set_detail_title(reference.title.clone());
    shell.set_detail_subtitle("".into());
    shell.set_detail_description("".into());
    shell.set_detail_desc_long(false);
    shell.set_detail_cover(slint::Image::default());
    shell.set_detail_loading(true);
    shell.set_detail_error("".into());
    shell.set_detail_tracks(ModelRc::new(VecModel::from_slice(&[])));
    shell.set_detail_albums(ModelRc::new(VecModel::from_slice(&[])));
    shell.set_detail_sections(ModelRc::new(VecModel::from_slice(&[])));
    shell.set_detail_tab(0);
}

pub fn apply_detail_error(shell: &MainShell, message: &str) {
    shell.set_detail_loading(false);
    shell.set_detail_error(message.into());
}

pub fn apply_album_detail(shell: &MainShell, detail: &AlbumPage) {
    clear_detail(
        shell,
        DetailKind::Album,
        detail.album.id.as_str(),
        detail.album.title.as_str(),
    );
    shell.set_detail_subtitle(detail.album.artist_text().as_str().into());
    shell.set_detail_description(
        detail
            .album
            .year
            .as_deref()
            .unwrap_or_default()
            .into(),
    );
    shell.set_detail_tracks(ModelRc::new(VecModel::from_slice(&song_results(
        &detail.tracks,
    ))));
    artwork::resolve_detail(shell);
}

fn localized_number(number: &str, lang: Lang) -> String {
    let (integer, fraction) = match number.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (number, None),
    };
    let (group, decimal) = match lang {
        Lang::Es => ('.', ','),
        Lang::En => (',', '.'),
    };
    let mut grouped = String::new();
    for (index, ch) in integer.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(group);
        }
        grouped.push(ch);
    }
    let integer: String = grouped.chars().rev().collect();
    match fraction {
        Some(fraction) => format!("{integer}{decimal}{fraction}"),
        None => integer,
    }
}

fn audience_key(raw: &str) -> &'static str {
    let raw = raw.to_lowercase();
    if raw.ends_with("monthly audience")
        || raw.ends_with("monthly listeners")
        || raw.ends_with("oyentes mensuales")
        || raw.ends_with("usuarios mensuales")
    {
        "artist.monthly_audience"
    } else {
        "artist.subscribers"
    }
}

fn split_count(raw: &str) -> Option<(&str, &str)> {
    let start = raw.find(|c: char| c.is_ascii_digit())?;
    let rest = &raw[start..];
    let number_len = rest
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit() || *ch == '.' || *ch == ',')
        .map(|(index, ch)| index + ch.len_utf8())
        .last()
        .unwrap_or(0);
    if number_len == 0 {
        return None;
    }
    let number = &rest[..number_len];
    let after = rest[number_len..].trim_start();
    let unit = match after.chars().next() {
        Some(ch @ ('K' | 'M' | 'B')) => &after[..ch.len_utf8()],
        _ => "",
    };
    Some((number, unit))
}

fn normalize_count(number: &str) -> String {
    let Some((index, separator)) = number
        .char_indices()
        .filter(|(_, ch)| *ch == '.' || *ch == ',')
        .last()
    else {
        return number.to_owned();
    };
    let decimals = number[index + separator.len_utf8()..].chars().count();
    let decimal = decimals == 1 || decimals == 2;
    let mut normalized = String::new();
    for (offset, ch) in number.char_indices() {
        if ch == '.' || ch == ',' {
            if decimal && offset == index {
                normalized.push('.');
            }
        } else {
            normalized.push(ch);
        }
    }
    normalized
}

pub fn subscriber_label(raw: &str, lang: Lang) -> Option<String> {
    let (number, unit) = split_count(raw)?;
    let key = audience_key(raw);
    let number = localized_number(&normalize_count(number), lang);
    let value = match (lang, unit) {
        (Lang::Es, "") => number,
        (Lang::Es, unit) => format!("{number} {unit}"),
        (Lang::En, unit) => format!("{number}{unit}"),
    };
    Some(madrigal_i18n::tr(lang, key).replace("%1", &value))
}

pub fn apply_artist_detail(shell: &MainShell, detail: &ArtistPage) {
    clear_detail(
        shell,
        DetailKind::Artist,
        detail.artist.id.as_str(),
        detail.artist.name.as_str(),
    );
    let lang = crate::lang_from_code(shell.global::<crate::I18n>().get_lang().as_str());
    shell.set_detail_subtitle(
        detail
            .artist
            .subscribers
            .as_deref()
            .and_then(|raw| subscriber_label(raw, lang))
            .unwrap_or_default()
            .into(),
    );
    let description = detail
        .artist
        .description
        .as_deref()
        .unwrap_or_default()
        .to_owned();
    shell.set_detail_desc_long(description.lines().count() > 3 || description.chars().count() > 220);
    shell.set_detail_description(description.into());
    shell.set_detail_tracks(ModelRc::new(VecModel::from_slice(&song_results(
        &detail.songs,
    ))));
    shell.set_detail_albums(ModelRc::new(VecModel::from_slice(&album_results(
        &detail.albums,
    ))));
    artwork::resolve_detail(shell);
}

pub fn apply_playlist_detail(shell: &MainShell, detail: &PlaylistPage) {
    clear_detail(
        shell,
        DetailKind::Playlist,
        detail.playlist.id.as_str(),
        detail.playlist.title.as_str(),
    );
    let mut subtitle = detail.playlist.author.clone().unwrap_or_default();
    if let Some(count) = detail.playlist.track_count {
        if !subtitle.is_empty() {
            subtitle.push_str(" · ");
        }
        subtitle.push_str(&count.to_string());
    }
    shell.set_detail_subtitle(subtitle.into());
    shell.set_detail_description(
        detail
            .playlist
            .description
            .as_deref()
            .unwrap_or_default()
            .into(),
    );
    shell.set_detail_tracks(ModelRc::new(VecModel::from_slice(&song_results(
        &detail.tracks,
    ))));
    artwork::resolve_detail(shell);
}

pub fn apply_podcast_detail(shell: &MainShell, podcast: &Podcast, episodes: &[Episode]) {
    clear_detail(
        shell,
        DetailKind::Podcast,
        podcast.id.as_str(),
        podcast.title.as_str(),
    );
    shell.set_detail_subtitle(podcast.author.as_deref().unwrap_or_default().into());
    shell.set_detail_description(
        podcast.description.as_deref().unwrap_or_default().into(),
    );
    let items: Vec<CatalogItem> = episodes.iter().map(episode_item).collect();
    shell.set_detail_tracks(ModelRc::new(VecModel::from_slice(&items)));
    artwork::resolve_detail(shell);
}

pub fn apply_browse_detail(shell: &MainShell, browse: &BrowsePage, title: &str) {
    clear_detail(shell, DetailKind::Browse, "browse", title);
    let sections: Vec<CatalogSection> = browse
        .sections
        .iter()
        .map(|section| {
            section_of(
                section.title.as_str(),
                section.items.iter().map(browse_item).collect(),
            )
        })
        .collect();
    shell.set_detail_sections(ModelRc::new(VecModel::from_slice(&sections)));
    artwork::resolve_detail(shell);
}

pub fn load_album_fixture(shell: &MainShell, id: &str, source: &str) -> CatalogResult<()> {
    apply_album_detail(shell, &CatalogClient::parse_album_page(id, source)?);
    Ok(())
}

pub fn load_artist_fixture(shell: &MainShell, id: &str, source: &str) -> CatalogResult<()> {
    apply_artist_detail(shell, &CatalogClient::parse_artist_page(id, source)?);
    Ok(())
}

pub fn load_playlist_fixture(
    shell: &MainShell,
    id: &str,
    details_source: &str,
    tracks_source: &str,
) -> CatalogResult<()> {
    apply_playlist_detail(
        shell,
        &CatalogClient::parse_playlist_page(id, details_source, tracks_source)?,
    );
    Ok(())
}

pub fn load_browse_fixture(shell: &MainShell, source: &str, title: &str) -> CatalogResult<()> {
    apply_browse_detail(shell, &CatalogClient::parse_browse(source)?, title);
    Ok(())
}
