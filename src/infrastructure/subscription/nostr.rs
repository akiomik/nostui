use std::sync::Arc;
use std::time::Duration;

use futures::{
    stream::{self, BoxStream},
    StreamExt,
};
use nostr_sdk::prelude::*;
use tears::{SubscriptionId, SubscriptionSource};
use tokio::sync::{broadcast, mpsc, RwLock};

use crate::domain::nostr::feed_filter::{
    home_feed_filters, home_load_more_filter, mention_feed_filters, mention_load_more_filter,
    user_feed_filters, user_load_more_filter, with_own_pubkey,
};
use crate::domain::nostr::FeedKind;
use crate::model::nostr_gateway::{CommandError, Message, NostrCommand};

const DEFAULT_CONTACT_LIST_TIMEOUT_SECS: u64 = 10;

#[derive(Debug, Clone)]
pub struct NostrEvents {
    client: Arc<Client>,
    /// Cached contact list (following) fetched during initialization
    /// Shared across all instances via `Arc<RwLock<>>`
    contact_list: Arc<RwLock<Option<Vec<PublicKey>>>>,
}

impl NostrEvents {
    /// Create a new NostrEvents subscription from an `Arc<Client>`.
    ///
    /// The same Arc should be reused across subscriptions to maintain subscription identity.
    /// This ensures that the subscription ID remains constant and the subscription is not
    /// recreated every frame.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use nostr_sdk::Client;
    /// use nostui::infrastructure::subscription::nostr::NostrEvents;
    ///
    /// let client = Arc::new(Client::default());
    /// let nostr_events = NostrEvents::new(Arc::clone(&client));
    /// ```
    #[must_use]
    pub fn new(client: Arc<Client>) -> Self {
        Self {
            client,
            contact_list: Arc::new(RwLock::new(None)),
        }
    }

    /// Initialize the home feed subscription by fetching the contact list and subscribing to filters
    /// Also caches the contact list for future use (e.g., loading more events)
    /// Sends SubscriptionCreated messages for NostrState to track
    async fn initialize_home_feed(
        client: &Client,
        contact_list_cache: Arc<RwLock<Option<Vec<PublicKey>>>>,
        msg_tx: &mpsc::UnboundedSender<Message>,
    ) -> broadcast::Receiver<RelayPoolNotification> {
        match client
            .get_contact_list_public_keys(Duration::from_secs(DEFAULT_CONTACT_LIST_TIMEOUT_SECS))
            .await
        {
            Ok(mut followings) => {
                // Always include the user's own posts in the home feed,
                // even if they don't follow themselves.
                if let Ok(signer) = client.signer().await {
                    if let Ok(own_pubkey) = signer.get_public_key().await {
                        followings = with_own_pubkey(followings, own_pubkey);
                    }
                }

                // Cache the contact list (including own pubkey) for future use
                {
                    let mut cache = contact_list_cache.write().await;
                    *cache = Some(followings.clone());
                }

                let [feed_backward_filter, feed_forward_filter, profile_filter] =
                    home_feed_filters(followings, Timestamp::now());

                // Subscribe to both feed and profile data concurrently
                let result = tokio::try_join!(
                    client.subscribe(feed_backward_filter, None),
                    client.subscribe(feed_forward_filter, None),
                    client.subscribe(profile_filter, None)
                );

                if let Ok((sub_id1, sub_id2, sub_id3)) = result {
                    // Send SubscriptionCreated messages for NostrState to track
                    let feed = FeedKind::Home;
                    let _ = msg_tx.send(Message::SubscriptionCreated {
                        feed: feed.clone(),
                        subscription_id: sub_id1.val,
                    });
                    let _ = msg_tx.send(Message::SubscriptionCreated {
                        feed: feed.clone(),
                        subscription_id: sub_id2.val,
                    });
                    let _ = msg_tx.send(Message::SubscriptionCreated {
                        feed,
                        subscription_id: sub_id3.val,
                    });
                }

                client.notifications()
            }
            Err(_) => {
                // If initialization fails, still create notifications channel for commands
                client.notifications()
            }
        }
    }

    /// Handle a single command and send error messages if needed
    async fn handle_command(
        cmd: NostrCommand,
        client: &Client,
        contact_list_cache: Arc<RwLock<Option<Vec<PublicKey>>>>,
        msg_tx: &mpsc::UnboundedSender<Message>,
    ) {
        match cmd {
            NostrCommand::SendEventBuilder { event_builder } => {
                if let Err(e) = client.send_event_builder(event_builder).await {
                    let _ = msg_tx.send(Message::Error {
                        error: CommandError::SendEventFailed {
                            error: e.to_string(),
                        },
                    });
                }
            }
            NostrCommand::AddRelay { url } => {
                if let Err(e) = client.add_relay(&url).await {
                    let _ = msg_tx.send(Message::Error {
                        error: CommandError::AddRelayFailed {
                            url: url.clone(),
                            error: e.to_string(),
                        },
                    });
                } else if let Err(e) = client.connect_relay(&url).await {
                    let _ = msg_tx.send(Message::Error {
                        error: CommandError::ConnectRelayFailed {
                            url,
                            error: e.to_string(),
                        },
                    });
                }
            }
            NostrCommand::RemoveRelay { url } => {
                if let Err(e) = client.remove_relay(&url).await {
                    let _ = msg_tx.send(Message::Error {
                        error: CommandError::RemoveRelayFailed {
                            url,
                            error: e.to_string(),
                        },
                    });
                }
            }
            NostrCommand::LoadMore { feed, since } => {
                // Load more feed events before the specified timestamp.
                // Map the feed to the appropriate domain filter builder; the home
                // feed reuses the contact list cached at init time.
                let filter = match &feed {
                    FeedKind::Home => match contact_list_cache.read().await.clone() {
                        Some(authors) => home_load_more_filter(authors, since),
                        None => {
                            log::warn!("Contact list not cached, cannot load more events");
                            return;
                        }
                    },
                    FeedKind::Mention => {
                        let Ok(signer) = client.signer().await else {
                            log::warn!("No signer available, cannot load more mention events");
                            return;
                        };
                        let Ok(own_pubkey) = signer.get_public_key().await else {
                            log::warn!("Failed to get public key, cannot load more mention events");
                            return;
                        };
                        mention_load_more_filter(own_pubkey, since)
                    }
                    FeedKind::Author(pubkey) => user_load_more_filter(*pubkey, since),
                };

                match client.subscribe(filter, None).await {
                    Ok(sub_id) => {
                        // Send SubscriptionCreated to track this load-more subscription
                        let _ = msg_tx.send(Message::SubscriptionCreated {
                            feed,
                            subscription_id: sub_id.val,
                        });
                    }
                    Err(e) => {
                        log::error!("Failed to load more events: {e}");
                    }
                }
            }
            NostrCommand::Subscribe { feed } => {
                match &feed {
                    FeedKind::Home => {
                        log::warn!("Home feed should be initialized, not subscribed via command");
                    }
                    FeedKind::Mention => {
                        let Ok(signer) = client.signer().await else {
                            log::error!("No signer available, cannot subscribe to mention feed");
                            return;
                        };
                        let Ok(own_pubkey) = signer.get_public_key().await else {
                            log::error!(
                                "Failed to get public key, cannot subscribe to mention feed"
                            );
                            return;
                        };

                        let [backward_filter, forward_filter] =
                            mention_feed_filters(own_pubkey, Timestamp::now());

                        let result = tokio::try_join!(
                            client.subscribe(backward_filter, None),
                            client.subscribe(forward_filter, None)
                        );

                        match result {
                            Ok((sub_id1, sub_id2)) => {
                                let _ = msg_tx.send(Message::SubscriptionCreated {
                                    feed: feed.clone(),
                                    subscription_id: sub_id1.val,
                                });
                                let _ = msg_tx.send(Message::SubscriptionCreated {
                                    feed,
                                    subscription_id: sub_id2.val,
                                });
                            }
                            Err(e) => {
                                log::error!("Failed to subscribe to mention feed: {e}");
                            }
                        }
                    }
                    FeedKind::Author(pubkey) => {
                        // Subscribe to both backward (historical) and forward (real-time) events
                        let [backward_filter, forward_filter] =
                            user_feed_filters(*pubkey, Timestamp::now());

                        // Subscribe to both filters concurrently
                        let result = tokio::try_join!(
                            client.subscribe(backward_filter, None),
                            client.subscribe(forward_filter, None)
                        );

                        match result {
                            Ok((sub_id1, sub_id2)) => {
                                // Send SubscriptionCreated messages for both subscriptions
                                let _ = msg_tx.send(Message::SubscriptionCreated {
                                    feed: feed.clone(),
                                    subscription_id: sub_id1.val,
                                });
                                let _ = msg_tx.send(Message::SubscriptionCreated {
                                    feed,
                                    subscription_id: sub_id2.val,
                                });
                            }
                            Err(e) => {
                                log::error!("Failed to subscribe to author feed: {e}");
                            }
                        }
                    }
                }
            }
            NostrCommand::Unsubscribe { subscription_ids } => {
                log::info!(
                    "Unsubscribing from {} subscriptions",
                    subscription_ids.len()
                );
                for sub_id in subscription_ids {
                    client.unsubscribe(&sub_id).await;
                    log::info!("Unsubscribed from {sub_id:?}");
                }
            }
            NostrCommand::Shutdown => {
                // Shutdown is handled in the main loop
            }
        }
    }

    /// Main subscription loop that processes notifications and commands
    async fn run_subscription_loop(
        client: Client,
        contact_list_cache: Arc<RwLock<Option<Vec<PublicKey>>>>,
        msg_tx: mpsc::UnboundedSender<Message>,
        mut cmd_rx: mpsc::UnboundedReceiver<NostrCommand>,
    ) {
        // Initialize the home feed subscription
        let mut notifications =
            Self::initialize_home_feed(&client, Arc::clone(&contact_list_cache), &msg_tx).await;

        loop {
            tokio::select! {
                // Handle incoming notifications from relays
                notification = notifications.recv() => {
                    match notification {
                        Ok(notif) => {
                            if msg_tx.send(Message::Notification(Box::new(notif))).is_err() {
                                // Receiver dropped, exit loop
                                break;
                            }
                        }
                        Err(_) => {
                            // Notification channel closed, exit loop
                            break;
                        }
                    }
                }
                // Handle incoming commands
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(NostrCommand::Shutdown) => {
                            // Disconnect from all relays and exit
                            client.disconnect().await;
                            break;
                        }
                        Some(cmd) => {
                            Self::handle_command(cmd, &client, Arc::clone(&contact_list_cache), &msg_tx).await;
                        }
                        None => {
                            // Command channel closed, exit loop
                            break;
                        }
                    }
                }
            }
        }
    }
}

impl SubscriptionSource for NostrEvents {
    type Output = Message;

    fn stream(&self) -> BoxStream<'static, Self::Output> {
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        // Clone the Arc, not the Client itself
        let client = Arc::clone(&self.client);
        let contact_list_cache = Arc::clone(&self.contact_list);

        tokio::spawn(async move {
            // Send Ready message with command sender
            if msg_tx.send(Message::Ready { sender: cmd_tx }).is_err() {
                // Receiver dropped before ready, exit early
                return;
            }

            // Run the main subscription loop
            // Dereference Arc to get &Client for the function call
            Self::run_subscription_loop((*client).clone(), contact_list_cache, msg_tx, cmd_rx)
                .await;
        });

        stream::unfold(msg_rx, |mut rx| async move {
            let msg = rx.recv().await?;
            Some((msg, rx))
        })
        .boxed()
    }

    fn id(&self) -> SubscriptionId {
        // Use the Arc pointer address as a unique ID
        // Same Arc<Client> instance = same ID, different Client instance = different ID
        let ptr = Arc::as_ptr(&self.client) as usize as u64;
        SubscriptionId::of::<Self>(ptr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_first_message_is_ready() {
        let client = Arc::new(Client::default());
        let nostr_events = NostrEvents::new(client);

        let mut stream = nostr_events.stream();

        // The subscription emits a Ready message (carrying the command sender)
        // before anything else.
        let first = stream
            .next()
            .await
            .expect("subscription should emit a first message");
        assert!(matches!(first, Message::Ready { .. }));
    }

    #[test]
    fn test_subscription_id_uses_arc_pointer() {
        use tears::SubscriptionSource;

        let client = Arc::new(Client::default());
        let nostr_events1 = NostrEvents::new(Arc::clone(&client));
        let nostr_events2 = nostr_events1.clone();

        // Same Arc<Client> should produce same ID
        assert_eq!(
            nostr_events1.id(),
            nostr_events2.id(),
            "Cloned NostrEvents should share the same Arc and produce the same ID"
        );

        // Verify ID is not zero (regression test for the bug where ID was always 0)
        let id1 = nostr_events1.id();
        let ptr1 = Arc::as_ptr(&nostr_events1.client) as usize as u64;
        assert_eq!(
            SubscriptionId::of::<NostrEvents>(ptr1),
            id1,
            "ID should be based on Arc pointer address"
        );
        assert_ne!(
            ptr1, 0,
            "Arc pointer address should not be zero in normal circumstances"
        );

        // Reusing the same Arc should produce the same ID
        let nostr_events3 = NostrEvents::new(Arc::clone(&client));
        assert_eq!(
            nostr_events1.id(),
            nostr_events3.id(),
            "Different NostrEvents instances with the same Arc<Client> should have the same ID"
        );
    }

    #[test]
    fn test_subscription_id_different_clients() {
        use tears::SubscriptionSource;

        // Create two separate clients with different Arcs
        let client1 = Arc::new(Client::default());
        let client2 = Arc::new(Client::default());

        let nostr_events1 = NostrEvents::new(Arc::clone(&client1));
        let nostr_events2 = NostrEvents::new(Arc::clone(&client2));

        // Different Arc<Client> instances should produce different IDs
        assert_ne!(
            nostr_events1.id(),
            nostr_events2.id(),
            "Different Arc<Client> instances should produce different subscription IDs"
        );

        // Verify both IDs use actual pointer addresses
        let ptr1 = Arc::as_ptr(&nostr_events1.client) as usize as u64;
        let ptr2 = Arc::as_ptr(&nostr_events2.client) as usize as u64;
        assert_ne!(
            ptr1, ptr2,
            "Different Arc instances should have different pointer addresses"
        );
    }

    #[test]
    fn test_subscription_id_different_arc_instances() {
        use tears::SubscriptionSource;

        let client = Client::default();

        // Creating separate Arc instances produces different IDs
        let nostr_events1 = NostrEvents::new(Arc::new(client.clone()));
        let nostr_events2 = NostrEvents::new(Arc::new(client));

        // Different Arc instances should produce different IDs
        assert_ne!(
            nostr_events1.id(),
            nostr_events2.id(),
            "Different Arc instances produce different subscription IDs"
        );

        // This demonstrates why you must share the same Arc<Client>
        // when you need consistent subscription identity across frames
    }
}
