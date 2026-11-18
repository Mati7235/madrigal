#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateError {
    Offline,
}

impl std::fmt::Display for GateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Offline => write!(f, "offline mode is on: network operation blocked"),
        }
    }
}

impl std::error::Error for GateError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkGate {
    offline: bool,
}

impl NetworkGate {
    pub fn new(offline: bool) -> Self {
        Self { offline }
    }

    pub fn from_settings(settings: &madrigal_settings::Settings) -> Self {
        Self::new(settings.internet.offline_mode)
    }

    pub fn is_offline(&self) -> bool {
        self.offline
    }

    pub fn check(&self) -> Result<(), GateError> {
        if self.offline {
            Err(GateError::Offline)
        } else {
            Ok(())
        }
    }

    pub fn try_online<T>(&self, op: impl FnOnce() -> T) -> Result<T, GateError> {
        self.check()?;
        Ok(op())
    }
}
