use madrigal_core::TrackIdentity;

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedStream {
    pub url: String,
    pub start_offset_secs: f64,
    pub user_agent: Option<String>,
    pub resolution_path: Option<String>,
}

impl ResolvedStream {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            start_offset_secs: 0.0,
            user_agent: None,
            resolution_path: None,
        }
    }

    pub fn with_offset(url: impl Into<String>, start_offset_secs: f64) -> Self {
        Self {
            url: url.into(),
            start_offset_secs: start_offset_secs.max(0.0),
            user_agent: None,
            resolution_path: None,
        }
    }

    pub fn with_user_agent(mut self, user_agent: Option<String>) -> Self {
        self.user_agent = user_agent;
        self
    }

    pub fn with_resolution_path(mut self, path: impl Into<String>) -> Self {
        self.resolution_path = Some(path.into());
        self
    }
}

pub trait StreamSource: Send + Sync + 'static {
    fn resolve(&self, track: &TrackIdentity) -> Result<ResolvedStream, String>;

    fn resolves_remotely(&self, _track: &TrackIdentity) -> bool {
        false
    }
}
