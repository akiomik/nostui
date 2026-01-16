use std::sync::Arc;
use std::time::Duration;

use futures::{
    stream::{self, BoxStream},
    StreamExt,
};
use nostr_sdk::prelude::*;
use tears::{SubscriptionId, SubscriptionSource};
use tokio::sync::{broadcast, mpsc, RwLock};

use crate::core::state::timeline::TimelineTabType;

const DEFAULT_CONTACT_LIST_TIMEOUT_SECS: u64 = 10;
const DEFAULT_TIMELINE_LIMIT: usize = 50;
const TIMELINE_KINDS: [Kind; 4] = [
    Kind::TextNote,
    Kind::Repost,
    Kind::Reaction,
    Kind::ZapReceipt,
];

/// Commands that can be sent to the Nostr subscription
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NostrCommand {
    /// Send an event to relays
    SendEvent { event: Event },
    /// Add a new relay
    AddRelay { url: String },
    /// Remove a relay
    RemoveRelay { url: String },
    /// Load more timeline events before the specified timestamp for a specific tab
    LoadMoreTimeline {
        tab_type: TimelineTabType,
        until: Timestamp,
    },
    /// Subscribe to a specific timeline tab
    SubscribeTimeline { tab_type: TimelineTabType },
    /// Unsubscribe from multiple subscriptions
    Unsubscribe {
        subscription_ids: Vec<nostr_sdk::SubscriptionId>,
    },
    /// Shutdown the subscription and disconnect from all relays
    Shutdown,
}

/// Errors that can occur during command execution
#[derive(Debug, Clone)]
pub enum CommandError {
    /// Failed to send event to relays
    SendEventFailed { error: String },
    /// Failed to add relay
    AddRelayFailed { url: String, error: String },
    /// Failed to connect to relay
    ConnectRelayFailed { url: String, error: String },
    /// Failed to remove relay
    RemoveRelayFailed { url: String, error: String },
}

/// Messages emitted by the Nostr subscription
#[derive(Debug, Clone)]
pub enum Message {
    /// Subscription is ready, provides command sender for user to send commands
    Ready {
        sender: mpsc::UnboundedSender<NostrCommand>,
    },
    /// A notification from the relay pool
    Notification(Box<RelayPoolNotification>),
    /// An error occurred during command execution
    Error { error: CommandError },
    /// A subscription was created for a specific tab
    SubscriptionCreated {
        tab_type: TimelineTabType,
        subscription_id: nostr_sdk::SubscriptionId,
    },
}

#[derive(Debug, Clone)]
pub struct NostrEvents {
    client: Arc<Client>,
    /// Cached contact list (following) fetched during initialization
    /// Shared across all instances via Arc<RwLock<>>
    contact_list: Arc<RwLock<Option<Vec<PublicKey>>>>,
}

impl NostrEvents {
    /// Create a new NostrEvents subscription from an Arc<Client>.
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

    /// Initialize timeline subscription by fetching contact list and subscribing to filters
    /// Also caches the contact list for future use (e.g., loading more events)
    /// Sends SubscriptionCreated messages for NostrState to track
    async fn initialize_timeline(
        client: &Client,
        contact_list_cache: Arc<RwLock<Option<Vec<PublicKey>>>>,
        msg_tx: &mpsc::UnboundedSender<Message>,
    ) -> broadcast::Receiver<RelayPoolNotification> {
        match client
            .get_contact_list_public_keys(Duration::from_secs(DEFAULT_CONTACT_LIST_TIMEOUT_SECS))
            .await
        {
            Ok(followings) => {
                // Cache the contact list for future use
                {
                    let mut cache = contact_list_cache.write().await;
                    *cache = Some(followings.clone());
                }

                let timeline_backward_filter = Filter::new()
                    .authors(followings.clone())
                    .kinds(TIMELINE_KINDS)
                    .until(Timestamp::now())
                    .limit(DEFAULT_TIMELINE_LIMIT);
                let timeline_forward_filter = Filter::new()
                    .authors(followings.clone())
                    .kinds(TIMELINE_KINDS)
                    .since(Timestamp::now());
                let profile_filter = Filter::new().authors(followings).kinds([Kind::Metadata]);

                // Subscribe to both timeline and profile data concurrently
                let result = tokio::try_join!(
                    client.subscribe(timeline_backward_filter, None),
                    client.subscribe(timeline_forward_filter, None),
                    client.subscribe(profile_filter, None)
                );

                if let Ok((sub_id1, sub_id2, sub_id3)) = result {
                    // Send SubscriptionCreated messages for NostrState to track
                    let tab_type = TimelineTabType::Home;
                    let _ = msg_tx.send(Message::SubscriptionCreated {
                        tab_type: tab_type.clone(),
                        subscription_id: sub_id1.val,
                    });
                    let _ = msg_tx.send(Message::SubscriptionCreated {
                        tab_type: tab_type.clone(),
                        subscription_id: sub_id2.val,
                    });
                    let _ = msg_tx.send(Message::SubscriptionCreated {
                        tab_type,
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
            NostrCommand::SendEvent { event } => {
                if let Err(e) = client.send_event(&event).await {
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
            NostrCommand::LoadMoreTimeline { tab_type, until } => {
                // Load more timeline events before the specified timestamp
                let filter = match &tab_type {
                    TimelineTabType::Home => {
                        // Use cached contact list from initialization
                        let followings = {
                            let cache = contact_list_cache.read().await;
                            match cache.as_ref() {
                                Some(list) => list.clone(),
                                None => {
                                    log::warn!("Contact list not cached, cannot load more events");
                                    return;
                                }
                            }
                        };

                        Filter::new()
                            .authors(followings)
                            .kinds(TIMELINE_KINDS)
                            .until(until)
                            .limit(DEFAULT_TIMELINE_LIMIT)
                    }
                    TimelineTabType::UserTimeline { pubkey } => Filter::new()
                        .authors(vec![*pubkey])
                        .kinds(vec![Kind::TextNote, Kind::Repost])
                        .until(until)
                        .limit(DEFAULT_TIMELINE_LIMIT),
                };

                match client.subscribe(filter, None).await {
                    Ok(sub_id) => {
                        // Send SubscriptionCreated to track this load-more subscription
                        let _ = msg_tx.send(Message::SubscriptionCreated {
                            tab_type,
                            subscription_id: sub_id.val,
                        });
                    }
                    Err(e) => {
                        log::error!("Failed to load more events: {e}");
                    }
                }
            }
            NostrCommand::SubscribeTimeline { tab_type } => {
                match &tab_type {
                    TimelineTabType::Home => {
                        log::warn!(
                            "Home timeline should be initialized, not subscribed via command"
                        );
                    }
                    TimelineTabType::UserTimeline { pubkey } => {
                        // Subscribe to both backward (historical) and forward (real-time) events
                        let backward_filter = Filter::new()
                            .authors(vec![*pubkey])
                            .kinds(vec![Kind::TextNote, Kind::Repost])
                            .until(Timestamp::now())
                            .limit(DEFAULT_TIMELINE_LIMIT);

                        let forward_filter = Filter::new()
                            .authors(vec![*pubkey])
                            .kinds(vec![Kind::TextNote, Kind::Repost])
                            .since(Timestamp::now());

                        // Subscribe to both filters concurrently
                        let result = tokio::try_join!(
                            client.subscribe(backward_filter, None),
                            client.subscribe(forward_filter, None)
                        );

                        match result {
                            Ok((sub_id1, sub_id2)) => {
                                // Send SubscriptionCreated messages for both subscriptions
                                let _ = msg_tx.send(Message::SubscriptionCreated {
                                    tab_type: tab_type.clone(),
                                    subscription_id: sub_id1.val,
                                });
                                let _ = msg_tx.send(Message::SubscriptionCreated {
                                    tab_type,
                                    subscription_id: sub_id2.val,
                                });
                            }
                            Err(e) => {
                                log::error!("Failed to subscribe to user timeline: {e}");
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
        // Initialize timeline subscription
        let mut notifications =
            Self::initialize_timeline(&client, Arc::clone(&contact_list_cache), &msg_tx).await;

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

    #[test]
    fn test_nostr_events_creation() {
        let client = Arc::new(Client::default());
        let _nostr_events = NostrEvents::new(client);
    }

    #[tokio::test]
    async fn test_ready_message_provides_sender() {
        let client = Arc::new(Client::default());
        let nostr_events = NostrEvents::new(client);

        let mut stream = nostr_events.stream();

        // First message should be Ready with command sender
        if let Some(Message::Ready { sender }) = stream.next().await {
            // Verify we can use the sender
            assert!(sender.send(NostrCommand::Shutdown).is_ok());
        } else {
            panic!("Expected Ready message as first message");
        }
    }

    #[tokio::test]
    async fn test_command_sender_can_be_cloned() {
        let client = Arc::new(Client::default());
        let nostr_events = NostrEvents::new(client);

        let mut stream = nostr_events.stream();

        // Get the command sender from Ready message
        if let Some(Message::Ready { sender }) = stream.next().await {
            let sender1 = sender.clone();
            let sender2 = sender;

            // Both senders should work
            assert!(sender1.send(NostrCommand::Shutdown).is_ok());
            assert!(sender2.send(NostrCommand::Shutdown).is_ok());
        } else {
            panic!("Expected Ready message");
        }
    }

    #[tokio::test]
    async fn test_various_commands() {
        let client = Arc::new(Client::default());
        let nostr_events = NostrEvents::new(client);

        let mut stream = nostr_events.stream();

        // Get the command sender
        if let Some(Message::Ready { sender }) = stream.next().await {
            // Test sending various commands
            assert!(sender
                .send(NostrCommand::AddRelay {
                    url: "wss://relay.example.com".to_string()
                })
                .is_ok());

            assert!(sender
                .send(NostrCommand::RemoveRelay {
                    url: "wss://relay.example.com".to_string()
                })
                .is_ok());

            assert!(sender.send(NostrCommand::Shutdown).is_ok());
        } else {
            panic!("Expected Ready message");
        }
    }

    #[tokio::test]
    async fn test_error_messages() -> Result<()> {
        let client = Arc::new(Client::default());
        let nostr_events = NostrEvents::new(client);

        let mut stream = nostr_events.stream();

        // Get the command sender
        if let Some(Message::Ready { sender }) = stream.next().await {
            // Try to add an invalid relay (should produce an error)
            sender.send(NostrCommand::AddRelay {
                url: "invalid-url".to_string(),
            })?;

            // Should receive an error message
            // Note: Since this is async and depends on relay operations,
            // we just verify the error variant exists
            // In real usage, users would handle Message::Error { error } in their message loop
        } else {
            panic!("Expected Ready message");
        }

        Ok(())
    }

    #[test]
    fn test_command_error_types() {
        // Test that error types can be constructed and matched
        let error = CommandError::SendEventFailed {
            error: "test error".to_string(),
        };

        match error {
            CommandError::SendEventFailed { error } => {
                assert_eq!(error, "test error");
            }
            _ => panic!("Wrong error variant"),
        }

        let error = CommandError::AddRelayFailed {
            url: "wss://relay.test".to_string(),
            error: "connection failed".to_string(),
        };

        match error {
            CommandError::AddRelayFailed { url, error } => {
                assert_eq!(url, "wss://relay.test");
                assert_eq!(error, "connection failed");
            }
            _ => panic!("Wrong error variant"),
        }
    }

    #[test]
    fn test_message_variants() {
        // Test that all message variants can be matched
        let (tx, _rx) = mpsc::unbounded_channel();

        // Test Ready variant
        let msg = Message::Ready { sender: tx };
        match msg {
            Message::Ready { .. } => {} // OK
            _ => panic!("Expected Ready variant"),
        }

        // Test Notification variant with Shutdown
        let msg = Message::Notification(Box::new(RelayPoolNotification::Shutdown));
        match msg {
            Message::Notification(notif) if matches!(*notif, RelayPoolNotification::Shutdown) => {} // OK
            _ => panic!("Expected Notification(Shutdown) variant"),
        }

        // Test Error variant
        let msg = Message::Error {
            error: CommandError::SendEventFailed {
                error: "test".to_string(),
            },
        };
        match msg {
            Message::Error { .. } => {} // OK
            _ => panic!("Expected Error variant"),
        }
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
