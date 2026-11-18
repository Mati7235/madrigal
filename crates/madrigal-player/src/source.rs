use madrigal_core::TrackIdentity;

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedStream {
    pub url: String,
    pub start_offset_secs: f64,
}

impl ResolvedStream {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            start_offset_secs: 0.0,
        }
    }

    pub fn with_offset(url: impl Into<String>, start_offset_secs: f64) -> Self {
        Self {
            url: url.into(),
            start_offset_secs: start_offset_secs.max(0.0),
        }
    }
}

pub trait StreamSource: Send + Sync {
    fn resolve(&self, track: &TrackIdentity) -> Result<ResolvedStream, String>;
}
