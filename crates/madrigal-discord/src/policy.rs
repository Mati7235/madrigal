#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceMode {
    Playing,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    PlayingReady,
    PausedByUser,
    BufferingWhilePlayRequested,
    Inactive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HiddenReason {
    Disabled,
    NoClientId,
    NoTrack,
    PausedByPreference,
    ServiceStopping,
    NoStablePlaybackYet,
    PlaybackStalled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldReason {
    BufferingWhilePlayRequested,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Visible { track_id: String, mode: PresenceMode },
    Hidden { reason: HiddenReason },
    Hold { reason: HoldReason, track_id: Option<String> },
}

impl Decision {
    pub fn is_paused(&self) -> bool {
        matches!(
            self,
            Decision::Visible {
                mode: PresenceMode::Paused,
                ..
            }
        )
    }
}

#[derive(Debug, Clone)]
pub struct PresenceInputs {
    pub enabled: bool,
    pub has_client_id: bool,
    pub track_id: Option<String>,
    pub play_when_ready: bool,
    pub is_playing: bool,
    pub buffering: bool,
    pub show_when_paused: bool,
    pub service_stopping: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveHoldState {
    pub reason: HoldReason,
    pub track_id: Option<String>,
    pub started_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct LastAppliedVisible {
    pub track_id: String,
    pub mode: PresenceMode,
    pub applied_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct HoldContext {
    pub now_ms: u64,
    pub active_hold: Option<ActiveHoldState>,
    pub last_visible: Option<LastAppliedVisible>,
    pub hold_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldResolution {
    pub decision: Decision,
    pub next_hold: Option<ActiveHoldState>,
}

pub fn derive_semantic_state(input: &PresenceInputs) -> PlaybackState {
    if input.track_id.is_none() {
        return PlaybackState::Inactive;
    }
    if !input.play_when_ready {
        return PlaybackState::PausedByUser;
    }
    if input.buffering || !input.is_playing {
        return PlaybackState::BufferingWhilePlayRequested;
    }
    PlaybackState::PlayingReady
}

pub fn derive_raw_decision(
    input: &PresenceInputs,
    semantic: PlaybackState,
) -> Decision {
    if input.service_stopping {
        return Decision::Hidden {
            reason: HiddenReason::ServiceStopping,
        };
    }
    if !input.enabled {
        return Decision::Hidden {
            reason: HiddenReason::Disabled,
        };
    }
    if !input.has_client_id {
        return Decision::Hidden {
            reason: HiddenReason::NoClientId,
        };
    }
    let track_id = match input.track_id.clone() {
        Some(id) => id,
        None => {
            return Decision::Hidden {
                reason: HiddenReason::NoTrack,
            };
        }
    };
    match semantic {
        PlaybackState::PlayingReady => Decision::Visible {
            track_id,
            mode: PresenceMode::Playing,
        },
        PlaybackState::PausedByUser => {
            if !input.show_when_paused {
                Decision::Hidden {
                    reason: HiddenReason::PausedByPreference,
                }
            } else {
                Decision::Visible {
                    track_id,
                    mode: PresenceMode::Paused,
                }
            }
        }
        PlaybackState::BufferingWhilePlayRequested => Decision::Hold {
            reason: HoldReason::BufferingWhilePlayRequested,
            track_id: Some(track_id),
        },
        PlaybackState::Inactive => Decision::Hidden {
            reason: HiddenReason::PlaybackStalled,
        },
    }
}

pub fn resolve_decision(raw: Decision, context: &HoldContext) -> HoldResolution {
    match raw {
        Decision::Visible { .. } | Decision::Hidden { .. } => HoldResolution {
            decision: raw,
            next_hold: None,
        },
        Decision::Hold { reason, track_id } => {
            let last_visible = match context.last_visible.as_ref() {
                Some(visible) => visible,
                None => {
                    return HoldResolution {
                        decision: Decision::Hidden {
                            reason: HiddenReason::NoStablePlaybackYet,
                        },
                        next_hold: None,
                    };
                }
            };
            if let Some(active) = context.active_hold.as_ref() {
                if active.reason == reason && active.track_id == track_id {
                    if context.now_ms.saturating_sub(active.started_at_ms)
                        >= context.hold_timeout_ms
                    {
                        return HoldResolution {
                            decision: Decision::Hidden {
                                reason: HiddenReason::PlaybackStalled,
                            },
                            next_hold: None,
                        };
                    }
                    return HoldResolution {
                        decision: Decision::Hold { reason, track_id },
                        next_hold: Some(active.clone()),
                    };
                }
            }
            HoldResolution {
                decision: Decision::Hold {
                    reason,
                    track_id: track_id.clone(),
                },
                next_hold: Some(ActiveHoldState {
                    reason,
                    track_id: track_id.or(Some(last_visible.track_id.clone())),
                    started_at_ms: context.now_ms,
                }),
            }
        }
    }
}

pub fn derive_decision(input: &PresenceInputs) -> Decision {
    derive_final_decision(
        input,
        &HoldContext {
            now_ms: 0,
            active_hold: None,
            last_visible: None,
            hold_timeout_ms: 0,
        },
    )
    .decision
}

pub fn derive_final_decision(
    input: &PresenceInputs,
    context: &HoldContext,
) -> HoldResolution {
    let semantic = derive_semantic_state(input);
    let raw = derive_raw_decision(input, semantic);
    resolve_decision(raw, context)
}
