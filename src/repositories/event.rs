use color_eyre::eyre::Result;
use std::sync::Arc;

use nostr_sdk::database::MemoryDatabase;
use nostr_sdk::prelude::*;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Mutex;

use crate::nostr::ConnectionAction;

#[derive(Clone)]
pub struct EventRepository {
    keys: Keys,
    cache: Arc<Mutex<MemoryDatabase>>,
    tx: UnboundedSender<ConnectionAction>,
}

impl EventRepository {
    pub fn new(
        keys: Keys,
        cache: Arc<Mutex<MemoryDatabase>>,
        tx: UnboundedSender<ConnectionAction>,
    ) -> Self {
        Self { keys, cache, tx }
    }

    pub fn send(&self, ev: Event) -> Result<()> {
        self.tx.send(ConnectionAction::SendEvent(ev))?;
        Ok(())
    }

    pub async fn find(&self, id: EventId) -> Option<Event> {
        let cache = (*self.cache).lock().await;
        cache.event_by_id(id).await.ok()
    }

    pub fn send_text_note(&self, content: &String, tags: Vec<Tag>) -> Result<Event> {
        let ev = EventBuilder::text_note(content, tags).to_event(&self.keys)?;
        self.send(ev.clone())?;
        Ok(ev)
    }

    pub fn send_reaction(&self, id: EventId, pubkey: XOnlyPublicKey) -> Result<Event> {
        let ev = EventBuilder::reaction(id, pubkey, "+").to_event(&self.keys)?;
        self.send(ev.clone())?;
        Ok(ev)
    }

    pub fn send_repost(&self, id: EventId, pubkey: XOnlyPublicKey) -> Result<Event> {
        let ev = EventBuilder::repost(id, pubkey).to_event(&self.keys)?;
        self.send(ev.clone())?;
        Ok(ev)
    }
}
