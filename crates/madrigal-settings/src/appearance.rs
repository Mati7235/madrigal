use serde::{Deserialize, Serialize};

use crate::de::{bool_or_true, default_true, or_default};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
    Matugen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DarkMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FontPreference {
    #[default]
    Default,
    System,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OpenTab {
    #[default]
    Home,
    Explore,
    Library,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GridItemSize {
    #[default]
    Big,
    Small,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SliderStyle {
    #[default]
    Standard,
    Wavy,
    Thick,
    Circular,
    Simple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlayerDesignStyle {
    #[default]
    V1,
    V2,
    V3,
    V4,
    V5,
    V6,
    V7,
    V8,
    V9,
    V10,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MiniPlayerStyle {
    #[default]
    Theme,
    Gradient,
    Glow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlayerBackground {
    #[default]
    Default,
    Gradient,
    Custom,
    Blur,
    Coloring,
    BlurGradient,
    Glow,
    GlowAnimated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LyricsBackground {
    #[default]
    Default,
    FollowTheme,
    Coloring,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LyricsAnimation {
    #[default]
    Fade,
    None,
    Glow,
    Slide,
    Karaoke,
    Apple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlayerButtonsStyle {
    #[default]
    Default,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Es,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ArtworkShapeStyle {
    #[default]
    Background,
    Cover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LibraryViewType {
    #[default]
    List,
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QuickPicksDisplay {
    #[default]
    Card,
    List,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    #[serde(deserialize_with = "or_default")]
    pub theme: Theme,
    #[serde(deserialize_with = "or_default")]
    pub dynamic_theme: bool,
    #[serde(deserialize_with = "or_default")]
    pub dark_mode: DarkMode,
    #[serde(deserialize_with = "or_default")]
    pub pure_black: bool,
    #[serde(deserialize_with = "or_default")]
    pub custom_theme_color: String,
    #[serde(deserialize_with = "or_default")]
    pub random_theme_on_startup: bool,
    #[serde(deserialize_with = "or_default")]
    pub disable_animations: bool,
    #[serde(deserialize_with = "or_default")]
    pub use_system_font: bool,
    #[serde(deserialize_with = "or_default")]
    pub font_preference: FontPreference,
    #[serde(deserialize_with = "or_default")]
    pub custom_font_name: String,
    #[serde(deserialize_with = "or_default")]
    pub language: Language,
    #[serde(deserialize_with = "or_default")]
    pub use_system_language: bool,
    #[serde(deserialize_with = "or_default")]
    pub default_open_tab: OpenTab,
    #[serde(deserialize_with = "or_default")]
    pub grid_item_size: GridItemSize,
    #[serde(deserialize_with = "or_default")]
    pub slider_style: SliderStyle,
    #[serde(deserialize_with = "or_default")]
    pub player_design_style: PlayerDesignStyle,
    #[serde(deserialize_with = "or_default")]
    pub show_player_volume_bar: bool,
    #[serde(deserialize_with = "or_default")]
    pub hide_player_thumbnail: bool,
    #[serde(deserialize_with = "or_default")]
    pub thumbnail_corner_radius: f32,
    #[serde(deserialize_with = "or_default")]
    pub crop_thumbnail_to_square: bool,
    #[serde(deserialize_with = "or_default")]
    pub disable_blur: bool,
    #[serde(deserialize_with = "or_default")]
    pub blur_radius: f32,
    #[serde(deserialize_with = "or_default")]
    pub backdrop_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub backdrop_blur_amount: i32,
    #[serde(deserialize_with = "or_default")]
    pub mini_player_style: MiniPlayerStyle,
    #[serde(deserialize_with = "or_default")]
    pub player_background: PlayerBackground,
    #[serde(deserialize_with = "or_default")]
    pub lyrics_background: LyricsBackground,
    #[serde(deserialize_with = "or_default")]
    pub lyrics_animation: LyricsAnimation,
    #[serde(deserialize_with = "or_default")]
    pub player_buttons_style: PlayerButtonsStyle,
    #[serde(default = "default_true", deserialize_with = "bool_or_true")]
    pub artwork_shape_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub artwork_shape_style: ArtworkShapeStyle,
    #[serde(deserialize_with = "or_default")]
    pub notify_track_change: bool,
    #[serde(deserialize_with = "or_default")]
    pub quick_picks_display: QuickPicksDisplay,
    #[serde(deserialize_with = "or_default")]
    pub artist_view_type: LibraryViewType,
    #[serde(deserialize_with = "or_default")]
    pub album_view_type: LibraryViewType,
    #[serde(deserialize_with = "or_default")]
    pub canvas_enabled: bool,
    #[serde(deserialize_with = "or_default")]
    pub canvas_source: String,
    #[serde(deserialize_with = "or_default")]
    pub canvas_wifi_only: bool,
    #[serde(deserialize_with = "or_default")]
    pub swipe_thumbnail: bool,
    #[serde(deserialize_with = "or_default")]
    pub swipe_sensitivity: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            dynamic_theme: true,
            dark_mode: DarkMode::System,
            pure_black: false,
            custom_theme_color: String::new(),
            random_theme_on_startup: false,
            disable_animations: false,
            use_system_font: false,
            font_preference: FontPreference::Default,
            custom_font_name: String::new(),
            language: Language::Es,
            use_system_language: false,
            default_open_tab: OpenTab::Home,
            grid_item_size: GridItemSize::Big,
            slider_style: SliderStyle::Standard,
            player_design_style: PlayerDesignStyle::V1,
            show_player_volume_bar: true,
            hide_player_thumbnail: false,
            thumbnail_corner_radius: 12.0,
            crop_thumbnail_to_square: false,
            disable_blur: false,
            blur_radius: 25.0,
            backdrop_enabled: true,
            backdrop_blur_amount: 40,
            mini_player_style: MiniPlayerStyle::Theme,
            player_background: PlayerBackground::Default,
            lyrics_background: LyricsBackground::Default,
            lyrics_animation: LyricsAnimation::Fade,
            player_buttons_style: PlayerButtonsStyle::Default,
            artwork_shape_enabled: true,
            artwork_shape_style: ArtworkShapeStyle::Background,
            notify_track_change: true,
            quick_picks_display: QuickPicksDisplay::Card,
            artist_view_type: LibraryViewType::List,
            album_view_type: LibraryViewType::Grid,
            canvas_enabled: false,
            canvas_source: String::new(),
            canvas_wifi_only: true,
            swipe_thumbnail: true,
            swipe_sensitivity: 1.0,
        }
    }
}

impl Appearance {
    pub fn sanitize(&mut self) {
        self.thumbnail_corner_radius =
            crate::sanitize::clamp_f32(self.thumbnail_corner_radius, 0.0, 32.0, 12.0);
        self.blur_radius = crate::sanitize::clamp_f32(self.blur_radius, 0.0, 100.0, 25.0);
        self.backdrop_blur_amount =
            crate::sanitize::clamp_i32(self.backdrop_blur_amount, 0, 100);
        self.swipe_sensitivity =
            crate::sanitize::clamp_f32(self.swipe_sensitivity, 0.1, 3.0, 1.0);
        self.custom_theme_color = self.custom_theme_color.trim().to_owned();
        self.custom_font_name = self.custom_font_name.trim().to_owned();
        self.canvas_source = self.canvas_source.trim().to_owned();
    }
}
