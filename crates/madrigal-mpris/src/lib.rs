mod mock;
mod provider;
mod service;

pub use mock::{MockProvider, MockState};
pub use provider::{
    ActionResult, BUS_NAME, DESKTOP_ENTRY, IDENTITY, LoopStatus, MAX_RATE, MIN_RATE, NowPlaying,
    NowPlayingProvider, OBJECT_PATH, PlaybackStatus, TRACK_PATH, metadata_map, rate_from_settings,
    track_object_path, volume_from_settings,
};
pub use service::MprisService;
