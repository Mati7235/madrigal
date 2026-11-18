use std::collections::VecDeque;
use std::sync::Mutex;

use crate::backend::{NotificationBackend, NotifyError, NotifyRequest};

#[derive(Debug, Clone)]
pub struct RecordedNotification {
    pub request: NotifyRequest,
    pub id: u32,
}

#[derive(Default)]
pub struct MockBackend {
    sent: Mutex<Vec<RecordedNotification>>,
    next_id: Mutex<u32>,
    capabilities: Mutex<Vec<String>>,
    pending_actions: Mutex<VecDeque<(u32, String)>>,
}

impl MockBackend {
    pub fn new(capabilities: Vec<String>) -> Self {
        Self {
            sent: Mutex::new(Vec::new()),
            next_id: Mutex::new(0),
            capabilities: Mutex::new(capabilities),
            pending_actions: Mutex::new(VecDeque::new()),
        }
    }

    pub fn push_action(&self, id: u32, key: &str) {
        self.pending_actions
            .lock()
            .expect("actions")
            .push_back((id, key.to_owned()));
    }

    pub fn sent(&self) -> Vec<RecordedNotification> {
        self.sent.lock().expect("sent").clone()
    }
}

impl NotificationBackend for MockBackend {
    async fn notify(&self, request: &NotifyRequest) -> Result<u32, NotifyError> {
        let mut next_id = self.next_id.lock().expect("id");
        *next_id += 1;
        let id = *next_id;
        self.sent.lock().expect("sent").push(RecordedNotification {
            request: request.clone(),
            id,
        });
        Ok(id)
    }

    async fn server_capabilities(&self) -> Result<Vec<String>, NotifyError> {
        Ok(self.capabilities.lock().expect("caps").clone())
    }

    async fn next_action(&self) -> Result<(u32, String), NotifyError> {
        self.pending_actions
            .lock()
            .expect("actions")
            .pop_front()
            .ok_or(NotifyError::Closed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Notifier;
    use crate::builder::NotificationBuilder;

    #[test]
    fn mock_redondea_show_y_acciones() {
        zbus::block_on(async {
            let notifier = Notifier::new(MockBackend::new(vec!["actions".to_owned()]));
            let id = notifier
                .show(&NotificationBuilder::new("t").body("b"))
                .await
                .expect("show");
            assert_eq!(id, 1);
            assert_eq!(notifier.server_capabilities().await.expect("caps"), vec!["actions"]);
            notifier.backend().push_action(id, "toggle");
            assert_eq!(notifier.next_action().await.expect("action"), (id, "toggle".to_owned()));
            assert!(notifier.backend().sent()[0].request.summary == "t");
        });
    }
}
