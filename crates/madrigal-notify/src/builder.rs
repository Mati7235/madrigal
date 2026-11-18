use madrigal_core::Track;

pub const APP_NAME: &str = "Madrigal";
pub const BUS_NAME: &str = "org.freedesktop.Notifications";
pub const OBJECT_PATH: &str = "/org/freedesktop/Notifications";
pub const INTERFACE: &str = "org.freedesktop.Notifications";
pub const DEFAULT_ACTION_KEY: &str = "default";
pub const ACTION_PREVIOUS: &str = "previous";
pub const ACTION_TOGGLE: &str = "toggle";
pub const ACTION_NEXT: &str = "next";
pub const MAX_LABELED_ACTIONS: usize = 3;
pub const DEFAULT_EXPIRE_MS: i32 = 5000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationAction {
    pub key: String,
    pub label: String,
}

impl NotificationAction {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NotificationBuilder {
    pub summary: String,
    pub body: String,
    pub icon: String,
    pub actions: Vec<NotificationAction>,
    pub expire_timeout_ms: i32,
    pub replaces_id: u32,
}

impl NotificationBuilder {
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            body: String::new(),
            icon: String::new(),
            actions: Vec::new(),
            expire_timeout_ms: DEFAULT_EXPIRE_MS,
            replaces_id: 0,
        }
    }

    pub fn playback(track: &Track) -> Self {
        Self::new(track.title.clone()).body(track.artists.join(", "))
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn timeout_ms(mut self, timeout_ms: i32) -> Self {
        self.expire_timeout_ms = timeout_ms;
        self
    }

    pub fn replaces(mut self, id: u32) -> Self {
        self.replaces_id = id;
        self
    }

    pub fn action(mut self, key: impl Into<String>, label: impl Into<String>) -> Self {
        if self.actions.len() < MAX_LABELED_ACTIONS {
            self.actions.push(NotificationAction::new(key, label));
        }
        self
    }

    pub fn playback_actions(self, previous: &str, toggle: &str, next: &str) -> Self {
        self.action(ACTION_PREVIOUS, previous)
            .action(ACTION_TOGGLE, toggle)
            .action(ACTION_NEXT, next)
    }

    pub fn flat_actions(&self) -> Vec<String> {
        let mut flat = vec![DEFAULT_ACTION_KEY.to_owned(), String::new()];
        for action in &self.actions {
            flat.push(action.key.clone());
            flat.push(action.label.clone());
        }
        flat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track {
            identity: madrigal_core::TrackIdentity::youtube("dQw4w9WgXcQ"),
            title: "Never Gonna Give You Up".to_owned(),
            artists: vec!["Rick Astley".to_owned()],
            album: None,
            duration_secs: None,
            thumbnail: None,
            track_number: None,
            local_path: None,
            set_video_id: None,
        }
    }

    #[test]
    fn playback_deriva_summary_y_body() {
        let built = NotificationBuilder::playback(&track());
        assert_eq!(built.summary, "Never Gonna Give You Up");
        assert_eq!(built.body, "Rick Astley");
        assert_eq!(built.expire_timeout_ms, DEFAULT_EXPIRE_MS);
    }

    #[test]
    fn acciones_planas_con_default_y_tope() {
        let built = NotificationBuilder::new("t")
            .playback_actions("Previous", "Play/Pause", "Next")
            .action("extra", "Extra");
        assert_eq!(
            built.flat_actions(),
            vec![
                "default",
                "",
                "previous",
                "Previous",
                "toggle",
                "Play/Pause",
                "next",
                "Next",
            ]
        );
        assert_eq!(built.actions.len(), MAX_LABELED_ACTIONS);
    }
}
