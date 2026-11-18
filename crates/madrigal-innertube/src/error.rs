use std::fmt;

pub enum Error {
    Api(ytmapi_rs::Error),
    Fixture(String),
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Api(e) => write!(f, "Api({e:?})"),
            Error::Fixture(msg) => write!(f, "Fixture({msg})"),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Api(e) => write!(f, "innertube request failed: {e}"),
            Error::Fixture(msg) => write!(f, "invalid fixture: {msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Api(e) => Some(e),
            Error::Fixture(_) => None,
        }
    }
}

impl From<ytmapi_rs::Error> for Error {
    fn from(e: ytmapi_rs::Error) -> Self {
        Error::Api(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Fixture(e.to_string())
    }
}
