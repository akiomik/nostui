use std::time::Duration;

use color_eyre::eyre::{ErrReport, Result};
use nostr_sdk::prelude::*;
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::UnboundedSender;

use crate::repositories::NostrAction;

pub struct NewConnectionOpts {
    event_channel_size: usize,
}

impl NewConnectionOpts {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for NewConnectionOpts {
    fn default() -> Self {
        Self {
            event_channel_size: 1024,
        }
    }
}

pub struct NewConnection {
    client: Client,
    opts: NewConnectionOpts,
}

impl NewConnection {
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self::with_opts(client, NewConnectionOpts::new())
    }

    #[must_use]
    pub fn with_opts(client: Client, opts: NewConnectionOpts) -> Self {
        Self { client, opts }
    }

    pub async fn timeline_filters(&self) -> Result<Vec<Filter>> {
        let followings = self.client.get_contact_list_public_keys(None).await?;
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

    #[must_use]
    pub fn run(self) -> (UnboundedSender<NostrAction>, Receiver<Event>) {
        let (event_tx, event_rx) =
            tokio::sync::broadcast::channel::<Event>(self.opts.event_channel_size);
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();

        tokio::spawn(async move {
            self.client.connect().await;
            let mut notifications = self.client.notifications();
            let filters = self.timeline_filters().await?;
            self.client.subscribe(filters).await;

            // TODO: Read cached events from self.client.database() on bootstrap

            'main: loop {
                while let Ok(notification) = notifications.try_recv() {
                    match notification {
                        RelayPoolNotification::Event { event, .. } => {
                            self.client.database().save_event(&event).await?;
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

                while let Ok(action) = action_rx.try_recv() {
                    match action {
                        NostrAction::SendEvent(ev) => {
                            self.client.send_event(ev).await?;
                        }
                        NostrAction::Shutdown => {
                            self.client.shutdown().await?;
                            break 'main;
                        }
                    }
                }
            }

            Ok::<(), ErrReport>(())
        });

        (action_tx, event_rx)
    }
}
