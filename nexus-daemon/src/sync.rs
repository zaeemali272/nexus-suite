//! Offline message queue synchronization engine.

use nexus_core::{Message, NexusResult, PeerId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

/// In-memory & SQLite backed offline message queue manager.
#[derive(Debug, Clone, Default)]
pub struct OfflineQueueManager {
    queue: Arc<Mutex<HashMap<PeerId, Vec<Message>>>>,
}

impl OfflineQueueManager {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Enqueue a message for an offline peer.
    pub async fn enqueue(&self, peer_id: PeerId, message: Message) -> NexusResult<()> {
        let mut guard = self.queue.lock().await;
        guard.entry(peer_id).or_default().push(message);
        info!("Enqueued message for offline peer {}", peer_id);
        Ok(())
    }

    /// Drain queued messages for a newly reconnected peer.
    pub async fn drain(&self, peer_id: &PeerId) -> Vec<Message> {
        let mut guard = self.queue.lock().await;
        guard.remove(peer_id).unwrap_or_default()
    }
}
