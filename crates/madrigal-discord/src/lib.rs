pub mod ipc;
pub mod oauth;
pub mod policy;
pub mod presence;
pub mod refresh;

pub const APP_NAME: &str = "Madrigal";

pub use ipc::{IpcClient, IpcError};
pub use oauth::{has_credentials, valid_client_id};
pub use policy::{
    ActiveHoldState, Decision, HiddenReason, HoldContext, HoldReason, HoldResolution, PlaybackState,
    PresenceInputs, PresenceMode, derive_decision, derive_final_decision, derive_raw_decision,
    derive_semantic_state, resolve_decision,
};
pub use presence::{Preview, build_activity};
pub use refresh::{PlaybackContext, RefreshGate, DEFAULT_DEBOUNCE_MS};
