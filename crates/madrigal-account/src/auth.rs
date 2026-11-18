use crate::cookies::{YOUTUBE_MUSIC_ORIGIN, sapisidhash};
use crate::session::Session;

pub const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36";
pub const DEFAULT_AUTHUSER: &str = "0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthHeaders {
    pub cookie: String,
    pub authorization: String,
    pub authuser: String,
    pub origin: String,
    pub user_agent: String,
    pub visitor_id: Option<String>,
}

impl AuthHeaders {
    pub fn pairs(&self) -> Vec<(String, String)> {
        let mut headers = vec![
            ("cookie".to_owned(), self.cookie.clone()),
            ("authorization".to_owned(), self.authorization.clone()),
            ("x-goog-authuser".to_owned(), self.authuser.clone()),
            ("origin".to_owned(), self.origin.clone()),
            ("user-agent".to_owned(), self.user_agent.clone()),
        ];
        if let Some(visitor) = &self.visitor_id {
            headers.push(("x-goog-visitor-id".to_owned(), visitor.clone()));
        }
        headers
    }
}

pub fn browser_auth(session: &Session, now: i64) -> Option<AuthHeaders> {
    if !session.logged_in() || !session.catalog_ready() {
        return None;
    }
    let authorization = sapisidhash(&session.cookie, YOUTUBE_MUSIC_ORIGIN, now)?;
    let authuser = if session.authuser.is_empty() {
        DEFAULT_AUTHUSER.to_owned()
    } else {
        session.authuser.clone()
    };
    Some(AuthHeaders {
        cookie: session.cookie.clone(),
        authorization,
        authuser,
        origin: YOUTUBE_MUSIC_ORIGIN.to_owned(),
        user_agent: USER_AGENT.to_owned(),
        visitor_id: if session.visitor_data.is_empty() {
            None
        } else {
            Some(session.visitor_data.clone())
        },
    })
}

pub fn on_behalf_of_user(session: &Session) -> Option<&str> {
    if session.data_sync_id.is_empty() {
        None
    } else {
        Some(session.data_sync_id.as_str())
    }
}
