use color_eyre::eyre::Result;
use std::sync::Arc;

use nostr_sdk::database::MemoryDatabase;
use nostr_sdk::prelude::*;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Mutex;

pub enum NostrAction {
    SendEvent(Event),
    Shutdown,
}

pub struct EventRepository {
    cache: Arc<Mutex<MemoryDatabase>>,
    tx: UnboundedSender<NostrAction>,
}

impl EventRepository {
    pub fn new(cache: Arc<Mutex<MemoryDatabase>>, tx: UnboundedSender<NostrAction>) -> Self {
        Self { cache, tx }
    }

    pub fn send(&self, ev: Event) -> Result<()> {
        self.tx.send(NostrAction::SendEvent(ev))?;
        Ok(())
    }

    pub async fn find(&self, id: EventId) -> Option<Event> {
        let cache = (*self.cache).lock().await;
        cache.event_by_id(id).await.ok()
    }
}
