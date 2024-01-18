use color_eyre::eyre::{Result, ErrReport};
use nostr_sdk::prelude::*;
use nostr_sdk::database::DatabaseOptions;
use nostr_sdk::database::memory::MemoryDatabase;

use crate::nostr::Connection;

pub struct EventRepository {
    cache: MemoryDatabase,
    conn: Connection,
    req_rx: tokio::sync::mpsc::UnboundedReceiver<Event>,
    req_tx: tokio::sync::mpsc::UnboundedSender<Event>,
    event_rx: tokio::sync::mpsc::UnboundedReceiver<Event>,
    event_tx: tokio::sync::mpsc::UnboundedSender<Event>,
    terminate_rx: tokio::sync::mpsc::UnboundedReceiver<()>,
    terminate_tx: tokio::sync::mpsc::UnboundedSender<()>,
}

impl EventRepository {
    pub fn new(conn: Connection) -> Self {
        let mut opts = DatabaseOptions::new();
        opts.events = true;
        let cache = MemoryDatabase::new(opts);

        let (req_tx, req_rx) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (terminate_tx, terminate_rx) = tokio::sync::mpsc::unbounded_channel();

        Self {
            cache,
            conn,
            req_tx,
            req_rx,
            event_rx,
            event_tx,
            terminate_rx,
            terminate_tx,
        }
    }

    pub fn run(mut self) {
        tokio::spawn(async move {
            let mut timeline = self.conn.subscribe_timeline().await?;

            loop {
                while let Ok(notification) = timeline.try_recv() {
                    if let RelayPoolNotification::Event { event, relay_url } = notification {
                        self.conn.database().save_event(&event).await?;
                        self.cache.save_event(&event).await?;
                        self.cache.event_id_seen(event.id, relay_url).await?;
                        self.req_tx.send(event)?;
                    };
                }

                while let Ok(event) = self.event_rx.try_recv() {
                    self.conn.send(event).await?;
                }

                if self.terminate_rx.try_recv().is_ok() {
                    self.conn.close().await?;
                    break;
                }
            }

            Ok::<(), ErrReport>(())
        });
    }

    pub async fn find_event(&mut self, id: EventId) -> Result<Event> {
        if let Ok(ev) = self.cache.event_by_id(id).await {
            return Ok(ev);
        }

        let ev = self.conn.database().event_by_id(id).await?;
        self.cache.save_event(&ev).await?;
        Ok(ev)
    }

    pub fn close(&self) -> Result<()> {
        self.terminate_tx.send(())?;
        Ok(())
    }

    pub fn send(&self, event: Event) -> Result<()> {
        self.event_tx.send(event)?;
        Ok(())
    }

    // TODO: subscribe arbitrary filters
    pub fn try_recv_req(&mut self) -> Result<Event> {
        let event = self.req_rx.try_recv()?;
        Ok(event)
    }
}
