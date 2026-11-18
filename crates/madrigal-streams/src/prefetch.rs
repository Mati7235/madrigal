use std::thread;

use madrigal_core::TrackIdentity;

use crate::resolver::Orchestrator;

#[derive(Clone)]
pub struct Prefetcher {
    client: Orchestrator,
}

impl Prefetcher {
    pub fn new(client: Orchestrator) -> Self {
        Self { client }
    }

    pub fn prefetch(&self, identity: TrackIdentity, quality: Option<String>) {
        let client = self.client.clone();
        thread::spawn(move || {
            let _ = client.resolve(&identity, quality.as_deref());
        });
    }

    pub fn prefetch_many(&self, items: Vec<(TrackIdentity, Option<String>)>) {
        for (identity, quality) in items {
            self.prefetch(identity, quality);
        }
    }
}
