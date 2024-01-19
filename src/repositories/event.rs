use std::sync::Arc;
use std::time::Duration;

use color_eyre::eyre::{ErrReport, Result};
use nostr_sdk::prelude::*;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Mutex;

pub struct EventRepository {
    client: Arc<Mutex<Client>>,
}

impl EventRepository {
    pub fn new(client: Arc<Mutex<Client>>) -> Self {
        Self { client }
    }

    pub async fn find(&self, id: EventId) -> Option<Event> {
        let client = (*self.client).lock().await;
        if let Ok(ev) = client.database().event_by_id(id).await {
            Some(ev)
        } else {
            None
        }
    }

    pub async fn send(&self, ev: Event) -> Result<()> {
        let client = (*self.client).lock().await;
        client.send_event(ev).await?;
        Ok(())
    }

    pub async fn timeline_filters(&self) -> Result<Vec<Filter>> {
        let client = (*self.client).lock().await;
        let followings = client.get_contact_list_public_keys(None).await?;
        let timeline_filter = Filter::new()
            .authors(followings.clone())
            .kinds([
                Kind::TextNote,
                Kind::Repost,
                Kind::Reaction,
                Kind::ZapReceipt,
            ])
            .since(Timestamp::now() - Duration::new(60 * 5, 0)); // 5min

        let profile_filter = Filter::new().authors(followings).kind(Kind::Metadata);

        Ok(vec![timeline_filter, profile_filter])
    }

    pub async fn run(
        &mut self,
    ) -> (
        UnboundedReceiver<Event>,
        UnboundedSender<Vec<Filter>>,
        UnboundedSender<()>,
    ) {
        // TODO: Use broadcast instead
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (stop_tx, mut stop_rx) = tokio::sync::mpsc::unbounded_channel();
        let (filter_tx, mut filter_rx) = tokio::sync::mpsc::unbounded_channel();

        let client_ptr = self.client.clone();

        tokio::spawn(async move {
            let client = (*client_ptr).lock().await;
            let mut notifications = client.notifications();

            loop {
                while let Ok(notification) = notifications.try_recv() {
                    match notification {
                        RelayPoolNotification::Event { event, .. } => {
                            client.database().save_event(&event).await?;
                            event_tx.send(event.clone())?;
                        }
                        RelayPoolNotification::RelayStatus { relay_url, status } => {
                            log::info!("A relay status changed on {relay_url}: {status}")
                        }
                        RelayPoolNotification::Message {
                            relay_url,
                            message: RelayMessage::Notice { message },
                        } => log::info!("A notice received from {relay_url}: {message}"),
                        _ => {}
                    }
                }

                while let Ok(filters) = filter_rx.try_recv() {
                    log::info!("The filters are updated");
                    client.unsubscribe().await;
                    client.subscribe(filters).await;
                }

                if stop_rx.try_recv().is_ok() {
                    break;
                }
            }
            Ok::<(), ErrReport>(())
        });

        (event_rx, filter_tx, stop_tx)
    }
}
