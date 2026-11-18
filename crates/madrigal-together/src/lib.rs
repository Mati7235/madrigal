pub mod discovery;
pub mod guest;
pub mod host;
pub mod link;
pub mod models;
pub mod proto;

pub use discovery::{Beacon, BeaconResponder, DEFAULT_DISCOVERY_PORT, discover};
pub use guest::ConnectedGuest;
pub use host::{HostConfig, RunningHost};
pub use link::{JoinInfo, JoinLink, parse as parse_link};
pub use models::{
    ActivityEvent, ActivityKind, Participant, PlayState, Role, RoomFlags, SessionCode,
    SessionSnapshot,
};
pub use proto::{
    AddMode, ControlAction, Message, PROTOCOL_VERSION,
};

pub fn default_display_name() -> String {
    let name = madrigal_settings::Settings::load().account.account_name;
    let trimmed = name.trim();
    if trimmed.is_empty() {
        String::from("Madrigal")
    } else {
        trimmed.chars().take(64).collect()
    }
}
