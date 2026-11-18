mod account;
mod ai;
mod appearance;
mod content;
mod de;
mod integrations;
mod internet;
mod lyrics;
mod player;
mod privacy;
mod recognition;
mod sanitize;
mod storage;
mod updates;

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

pub use account::Account;
pub use ai::{Ai, AiProvider, AiValidationStatus};
pub use appearance::{
    Appearance, ArtworkShapeStyle, DarkMode, FontPreference, GridItemSize, Language,
    LibraryViewType, LyricsAnimation, LyricsBackground, MiniPlayerStyle, OpenTab,
    PlayerBackground, PlayerButtonsStyle, PlayerDesignStyle, QuickPicksDisplay, SliderStyle,
    Theme,
};
pub use content::{
    AlbumFilter, AlbumSortType, ArtistFilter, ArtistSortType, ArtistSongSortType, AutoPlaylistSongSortType,
    Content, PlaylistSongSortType, PlaylistSortType, PlaylistSuggestionSource, QuickPicks,
    SearchSource, SongFilter, SongSortType, TopRange,
};
pub use integrations::{
    DiscordActivityType, DiscordPresenceStatus, Integrations, LastFmProvider, SponsorCategory,
    DEFAULT_SPONSOR_CATEGORIES, SPONSORBLOCK_DEFAULT_API_URL,
};
pub use internet::{Internet, ProxyType};
pub use lyrics::{
    Lyrics, LyricsMode, LyricsPosition, LyricsProvider, DEFAULT_PROVIDER_ORDER,
    TRANSLATION_DEFAULT_ENDPOINT,
};
pub use player::{
    AudioQuality, Normalization, Player, RepeatMode, TransitionMode, EQ_BANDS, sanitize_gains,
};
pub use privacy::Privacy;
pub use recognition::Recognition;
pub use storage::Storage;
pub use updates::{UpdateChannel, Updates};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(deserialize_with = "de::or_default")]
    pub appearance: Appearance,
    #[serde(deserialize_with = "de::or_default")]
    pub content: Content,
    #[serde(deserialize_with = "de::or_default")]
    pub player: Player,
    #[serde(deserialize_with = "de::or_default")]
    pub lyrics: Lyrics,
    #[serde(deserialize_with = "de::or_default")]
    pub internet: Internet,
    #[serde(deserialize_with = "de::or_default")]
    pub storage: Storage,
    #[serde(deserialize_with = "de::or_default")]
    pub privacy: Privacy,
    #[serde(deserialize_with = "de::or_default")]
    pub recognition: Recognition,
    #[serde(deserialize_with = "de::or_default")]
    pub account: Account,
    #[serde(deserialize_with = "de::or_default")]
    pub integrations: Integrations,
    #[serde(deserialize_with = "de::or_default")]
    pub ai: Ai,
    #[serde(deserialize_with = "de::or_default")]
    pub updates: Updates,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

pub fn sanitize_account_id(raw: &str) -> String {
    let id = raw.trim();
    if madrigal_paths::valid_profile_id(id) {
        id.to_owned()
    } else {
        madrigal_paths::DEFAULT_ACCOUNT_ID.to_owned()
    }
}

impl Settings {
    pub fn load() -> Self {
        Self::load_from(&madrigal_paths::settings_path())
    }

    pub fn load_from(path: &Path) -> Self {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(_) => return Self::default(),
        };
        let value: serde_json::Value = match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(_) => return Self::default(),
        };
        if !value.is_object() {
            return Self::default();
        }
        let mut settings: Self = serde_json::from_value(value).unwrap_or_default();
        settings.sanitize();
        settings
    }

    pub fn save(&self) -> io::Result<PathBuf> {
        self.save_to(&madrigal_paths::settings_path())
    }

    pub fn save_to(&self, path: &Path) -> io::Result<PathBuf> {
        let text = serde_json::to_string_pretty(self)
            .map(|mut text| {
                text.push('\n');
                text
            })
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        madrigal_files::atomic_write_text(path, &text)
    }

    fn sanitize(&mut self) {
        self.appearance.sanitize();
        self.content.sanitize();
        self.player.sanitize();
        self.lyrics.sanitize();
        self.internet.sanitize();
        self.storage.sanitize();
        self.privacy.sanitize();
        self.recognition.sanitize();
        self.account.sanitize();
        self.integrations.sanitize();
        self.ai.sanitize();
        self.updates.sanitize();
    }
}

pub fn set_active_account(id: &str) -> io::Result<()> {
    set_active_account_in(&madrigal_paths::settings_path(), id)
}

pub fn set_active_account_in(path: &Path, id: &str) -> io::Result<()> {
    if !madrigal_paths::valid_profile_id(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid account id: {id:?}"),
        ));
    }
    let mut root: serde_json::Value = match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or(serde_json::Value::Null),
        Err(err) if err.kind() == io::ErrorKind::NotFound => serde_json::Value::Null,
        Err(err) => return Err(err),
    };
    if !root.is_object() {
        root = serde_json::Value::Object(serde_json::Map::new());
    }
    let account = root
        .as_object_mut()
        .and_then(|map| map.get_mut("account"))
        .filter(|value| value.is_object());
    if account.is_none() {
        if let Some(map) = root.as_object_mut() {
            map.insert(
                "account".to_owned(),
                serde_json::Value::Object(serde_json::Map::new()),
            );
        }
    }
    if let Some(entry) = root
        .as_object_mut()
        .and_then(|map| map.get_mut("account"))
        .and_then(|value| value.as_object_mut())
    {
        entry.insert(
            "active_account_id".to_owned(),
            serde_json::Value::String(id.to_owned()),
        );
    }
    let text = serde_json::to_string_pretty(&root)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    madrigal_files::atomic_write_text(path, &text)?;
    Ok(())
}
