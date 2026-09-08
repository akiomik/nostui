use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};

use futures::{
    stream::{self, BoxStream},
    StreamExt,
};
use nostr_sdk::prelude::*;
use tears::SubscriptionSource;
use tokio::sync::{mpsc, RwLock};

use crate::domain::nostr::feed_filter::{
    home_feed_filters, home_load_more_filter, mention_feed_filters, mention_load_more_filter,
    user_feed_filters, user_load_more_filter, with_own_pubkey,
};
use crate::domain::nostr::FeedKind;
use crate::model::nostr_gateway::{CommandError, Message, NostrCommand};

const CONTACT_LIST_TIMEOUT: Duration = Duration::from_secs(10);

fn followings_from_latest_contact_list(events: &BTreeSet<Event>) -> Vec<PublicKey> {
    events
        .first()
        .map(|event| event.tags.public_keys().collect())
        .unwrap_or_default()
}

#[derive(Debug, Clone)]
pub struct NostrEvents {
    client: Arc<Client>,
    pubkey: PublicKey,
    keys: Option<Keys>,
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
    /// use nostr_sdk::prelude::Client;
    /// use nostui::infrastructure::subscription::nostr::NostrEvents;
    ///
    /// let client = Arc::new(Client::default());
    /// let keys = nostr_sdk::prelude::Keys::generate();
    /// let nostr_events = NostrEvents::new(Arc::clone(&client), keys.public_key(), Some(keys));
    /// ```
    #[must_use]
    pub fn new(client: Arc<Client>, pubkey: PublicKey, keys: Option<Keys>) -> Self {
        Self {
            client,
            pubkey,
            keys,
            contact_list: Arc::new(RwLock::new(None)),
        }
    }

    /// Initialize the home feed subscription by fetching the contact list and subscribing to filters
    /// Also caches the contact list for future use (e.g., loading more events)
    /// Sends SubscriptionCreated messages for NostrState to track
    async fn initialize_home_feed(
        client: &Client,
        pubkey: PublicKey,
        contact_list_cache: Arc<RwLock<Option<Vec<PublicKey>>>>,
        msg_tx: &mpsc::UnboundedSender<Message>,
    ) -> BoxStream<'static, ClientNotification> {
        let filter = Filter::new()
            .author(pubkey)
            .kind(Kind::ContactList)
            .limit(1);

        match client
            .fetch_events(filter)
            .timeout(CONTACT_LIST_TIMEOUT)
            .await
        {
            Ok(events) => {
                let mut followings = followings_from_latest_contact_list(&events);
                // Always include the user's own posts in the home feed,
                // even if they don't follow themselves.
                followings = with_own_pubkey(followings, pubkey);

                // Cache the contact list (including own pubkey) for future use
                {
                    let mut cache = contact_list_cache.write().await;
                    *cache = Some(followings.clone());
                }

                let [feed_backward_filter, feed_forward_filter, profile_filter] =
                    home_feed_filters(followings, Timestamp::now());

                // Subscribe to both feed and profile data concurrently
                let result = tokio::try_join!(
                    client.subscribe(feed_backward_filter),
                    client.subscribe(feed_forward_filter),
                    client.subscribe(profile_filter)
                );

                if let Ok((sub_id1, sub_id2, sub_id3)) = result {
                    // Send SubscriptionCreated messages for NostrState to track
                    let feed = FeedKind::Home;
                    let _ = msg_tx.send(Message::SubscriptionCreated {
                        feed: feed.clone(),
                        subscription_id: sub_id1.value,
                    });
                    let _ = msg_tx.send(Message::SubscriptionCreated {
                        feed: feed.clone(),
                        subscription_id: sub_id2.value,
                    });
                    let _ = msg_tx.send(Message::SubscriptionCreated {
                        feed,
                        subscription_id: sub_id3.value,
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

    /// Decide whether a completed `send_event` actually reached a relay.
    ///
    /// `Ok` from nostr-sdk does not mean the event was accepted: the pool returns
    /// `Ok(output)` once it has tried every relay, recording per-relay rejections and
    /// ack timeouts in `output.failed` and reserving `Err` for setup problems. An event
    /// every relay refused therefore arrives here as `Ok` with an empty `success` set.
    ///
    /// One relay accepting is enough — the event exists on the network — so a partial
    /// failure is logged rather than reported.
    fn relay_verdict(output: &SendEventOutput) -> Result<(), String> {
        if !output.success.is_empty() {
            if !output.failed.is_empty() {
                log::warn!(
                    "Event {} accepted by {} relay(s), refused by {}",
                    output.value,
                    output.success.len(),
                    Self::describe_failures(&output.failed)
                );
            }
            return Ok(());
        }

        Err(if output.failed.is_empty() {
            String::from("no relay accepted the event")
        } else {
            format!(
                "no relay accepted the event: {}",
                Self::describe_failures(&output.failed)
            )
        })
    }

    /// Render per-relay failures as `url: reason`, in a stable order so the same
    /// outcome always reads the same way.
    fn describe_failures(failed: &HashMap<RelayUrl, String>) -> String {
        let mut reasons: Vec<String> = failed
            .iter()
            .map(|(url, reason)| format!("{url}: {reason}"))
            .collect();
        reasons.sort();
        reasons.join(", ")
    }

    /// Run a single command and report its outcome to the application.
    async fn handle_command(
        cmd: NostrCommand,
        client: &Client,
        pubkey: PublicKey,
        keys: Option<&Keys>,
        contact_list_cache: Arc<RwLock<Option<Vec<PublicKey>>>>,
        msg_tx: &mpsc::UnboundedSender<Message>,
    ) {
        match cmd {
            NostrCommand::SendEventBuilder { event_builder } => {
                let result: Result<(), String> = match keys {
                    Some(keys) => match event_builder.finalize(keys) {
                        Ok(event) => match client.send_event(&event).await {
                            Ok(output) => Self::relay_verdict(&output),
                            Err(e) => Err(e.to_string()),
                        },
                        Err(e) => Err(e.to_string()),
                    },
                    None => Err(String::from("cannot send events in read-only mode")),
                };
                // Reported either way: the application shows a publish as pending until
                // this arrives, so a success that says nothing would leave it pending
                // forever.
                let _ = msg_tx.send(Message::EventPublished { result });
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
                    FeedKind::Mention => mention_load_more_filter(pubkey, since),
                    FeedKind::Author(pubkey) => user_load_more_filter(*pubkey, since),
                };

                match client.subscribe(filter).await {
                    Ok(sub_id) => {
                        // Send SubscriptionCreated to track this load-more subscription
                        let _ = msg_tx.send(Message::SubscriptionCreated {
                            feed,
                            subscription_id: sub_id.value,
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
                        let [backward_filter, forward_filter] =
                            mention_feed_filters(pubkey, Timestamp::now());

                        let result = tokio::try_join!(
                            client.subscribe(backward_filter),
                            client.subscribe(forward_filter)
                        );

                        match result {
                            Ok((sub_id1, sub_id2)) => {
                                let _ = msg_tx.send(Message::SubscriptionCreated {
                                    feed: feed.clone(),
                                    subscription_id: sub_id1.value,
                                });
                                let _ = msg_tx.send(Message::SubscriptionCreated {
                                    feed,
                                    subscription_id: sub_id2.value,
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
                            client.subscribe(backward_filter),
                            client.subscribe(forward_filter)
                        );

                        match result {
                            Ok((sub_id1, sub_id2)) => {
                                // Send SubscriptionCreated messages for both subscriptions
                                let _ = msg_tx.send(Message::SubscriptionCreated {
                                    feed: feed.clone(),
                                    subscription_id: sub_id1.value,
                                });
                                let _ = msg_tx.send(Message::SubscriptionCreated {
                                    feed,
                                    subscription_id: sub_id2.value,
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
                    match client.unsubscribe(&sub_id).await {
                        Ok(_) => log::info!("Unsubscribed from {sub_id:?}"),
                        Err(e) => log::warn!("Failed to unsubscribe from {sub_id:?}: {e}"),
                    }
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
        pubkey: PublicKey,
        keys: Option<Keys>,
        contact_list_cache: Arc<RwLock<Option<Vec<PublicKey>>>>,
        msg_tx: mpsc::UnboundedSender<Message>,
        mut cmd_rx: mpsc::UnboundedReceiver<NostrCommand>,
    ) {
        // Initialize the home feed subscription
        let mut notifications =
            Self::initialize_home_feed(&client, pubkey, Arc::clone(&contact_list_cache), &msg_tx)
                .await;

        loop {
            tokio::select! {
                // Handle incoming notifications from relays
                notification = notifications.next() => {
                    match notification {
                        Some(notif) => {
                            if msg_tx.send(Message::Notification(Box::new(notif))).is_err() {
                                // Receiver dropped, exit loop
                                break;
                            }
                        }
                        None => {
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
                        // Awaited inline on purpose, and load-bearing: the application
                        // matches `EventPublished` reports to submissions by position,
                        // which only holds because this never starts a second publish
                        // before the first has reported. Spawning this to stop a slow ack
                        // blocking the loop would silently settle outcomes against the
                        // wrong publish — that needs a correlation id first. The blocking
                        // itself is #515.
                        Some(cmd) => {
                            Self::handle_command(cmd, &client, pubkey, keys.as_ref(), Arc::clone(&contact_list_cache), &msg_tx).await;
                        }
                        None => {
                            // Command channel closed, exit loop
                            break;
                        }
                    }
                }
            }
        }

        // Close before draining, not after. `cmd_rx` would otherwise stay open until this
        // task returns, and a publish sent in that window would be accepted by the sender
        // — so the application pushes a pending entry — and then never dequeued by anyone.
        // Closing first makes those sends fail instead, which the application settles
        // immediately. Items already buffered are still receivable.
        cmd_rx.close();

        // Drained with `recv`, not `try_recv`. `send` bumps the message count before it
        // pushes the value, and `try_recv` reports an empty queue as `Empty` rather than
        // `Disconnected` while that count is non-zero — so a send caught mid-flight by
        // the `close` above would end the loop and be left in the channel unread. `recv`
        // waits it out and returns `None` only once the channel is closed and genuinely
        // drained; it cannot wait forever, because `close` stops any further send from
        // starting.
        //
        // Whatever is still queued will never run now. The application holds a pending
        // entry per submitted event and settles them in report order, so a publish that
        // simply vanished here would sit as "Sending" forever and shift every later
        // outcome onto the wrong submission.
        while let Some(cmd) = cmd_rx.recv().await {
            if matches!(cmd, NostrCommand::SendEventBuilder { .. }) {
                let _ = msg_tx.send(Message::EventPublished {
                    result: Err(String::from(
                        "the connection closed before the event was sent",
                    )),
                });
            }
        }
    }
}

impl SubscriptionSource for NostrEvents {
    type Output = Message;
    type Key = u64;

    fn stream(&self) -> BoxStream<'static, Self::Output> {
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        // Clone the Arc, not the Client itself
        let client = Arc::clone(&self.client);
        let pubkey = self.pubkey;
        let keys = self.keys.clone();
        let contact_list_cache = Arc::clone(&self.contact_list);

        tokio::spawn(async move {
            // Send Ready message with command sender
            if msg_tx.send(Message::Ready { sender: cmd_tx }).is_err() {
                // Receiver dropped before ready, exit early
                return;
            }

            // Run the main subscription loop
            // Dereference Arc to get &Client for the function call
            Self::run_subscription_loop(
                (*client).clone(),
                pubkey,
                keys,
                contact_list_cache,
                msg_tx,
                cmd_rx,
            )
            .await;
        });

        stream::unfold(msg_rx, |mut rx| async move {
            let msg = rx.recv().await?;
            Some((msg, rx))
        })
        .boxed()
    }

    fn key(&self) -> Self::Key {
        // Use the Arc pointer address as the structural key
        // Same Arc<Client> instance = same key, different Client instance = different key
        Arc::as_ptr(&self.client) as usize as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    fn relay_url(url: &str) -> RelayUrl {
        RelayUrl::parse(url).expect("valid relay url")
    }

    fn send_output(success: &[&str], failed: &[(&str, &str)]) -> SendEventOutput {
        let mut output: SendEventOutput = Output::new(EventId::from_byte_array([0u8; 32]));
        for url in success {
            output.success.insert(relay_url(url), EventSendStatus::Sent);
        }
        for (url, reason) in failed {
            output.failed.insert(relay_url(url), (*reason).to_owned());
        }
        output
    }

    #[test]
    fn a_send_every_relay_refused_is_a_failure() {
        // nostr-sdk returns `Ok` once it has tried every relay, so an all-refused send
        // arrives as `Ok` with nothing in `success`. Reporting that as a publish would
        // put "Posted" on screen for a note no relay stored.
        let output = send_output(&[], &[("wss://a.example", "blocked")]);

        let error = NostrEvents::relay_verdict(&output).expect_err("should be a failure");
        assert!(
            error.contains("no relay accepted") && error.contains("blocked"),
            "expected the relay's reason to survive, got: {error}"
        );
    }

    #[test]
    fn a_send_no_relay_answered_is_a_failure() {
        let output = send_output(&[], &[]);

        assert!(NostrEvents::relay_verdict(&output).is_err());
    }

    #[test]
    fn one_relay_accepting_is_enough() {
        // The event exists on the network, so a partial failure is logged, not reported.
        let output = send_output(&["wss://a.example"], &[("wss://b.example", "rate-limited")]);

        assert_eq!(NostrEvents::relay_verdict(&output), Ok(()));
    }

    #[test]
    fn failure_reasons_read_the_same_way_every_time() {
        // `failed` is a HashMap, so without sorting the same outcome would render in a
        // different order each run.
        let output = send_output(
            &[],
            &[
                ("wss://b.example", "blocked"),
                ("wss://a.example", "bad sig"),
            ],
        );

        let error = NostrEvents::relay_verdict(&output).expect_err("should be a failure");
        assert!(
            error.find("a.example").expect("a") < error.find("b.example").expect("b"),
            "expected a stable order, got: {error}"
        );
    }

    #[test]
    fn followings_use_only_the_latest_contact_list() {
        let author = Keys::generate();
        let unfollowed = Keys::generate();
        let following = Keys::generate();

        let old_contact_list = EventBuilder::new(Kind::ContactList, "")
            .tags([Tag::public_key(unfollowed.public_key())])
            .custom_created_at(Timestamp::from(1))
            .finalize(&author)
            .expect("valid contact list event");
        let latest_contact_list = EventBuilder::new(Kind::ContactList, "")
            .tags([Tag::public_key(following.public_key())])
            .custom_created_at(Timestamp::from(2))
            .finalize(&author)
            .expect("valid contact list event");

        let events = BTreeSet::from([old_contact_list, latest_contact_list]);

        assert_eq!(
            followings_from_latest_contact_list(&events),
            vec![following.public_key()]
        );
    }

    #[tokio::test]
    async fn test_first_message_is_ready() {
        let client = Arc::new(Client::default());
        let nostr_events = NostrEvents::new(client, Keys::generate().public_key(), None);

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
    fn test_subscription_key_uses_arc_pointer() {
        use tears::SubscriptionSource;

        let client = Arc::new(Client::default());
        let nostr_events1 =
            NostrEvents::new(Arc::clone(&client), Keys::generate().public_key(), None);
        let nostr_events2 = nostr_events1.clone();

        // Same Arc<Client> should produce same key
        assert_eq!(
            nostr_events1.key(),
            nostr_events2.key(),
            "Cloned NostrEvents should share the same Arc and produce the same key"
        );

        // Verify key is not zero (regression test for the bug where ID was always 0)
        let key1 = nostr_events1.key();
        let ptr1 = Arc::as_ptr(&nostr_events1.client) as usize as u64;
        assert_eq!(ptr1, key1, "Key should be based on Arc pointer address");
        assert_ne!(
            ptr1, 0,
            "Arc pointer address should not be zero in normal circumstances"
        );

        // Reusing the same Arc should produce the same key
        let nostr_events3 =
            NostrEvents::new(Arc::clone(&client), Keys::generate().public_key(), None);
        assert_eq!(
            nostr_events1.key(),
            nostr_events3.key(),
            "Different NostrEvents instances with the same Arc<Client> should have the same key"
        );
    }

    #[test]
    fn test_subscription_key_different_clients() {
        use tears::SubscriptionSource;

        // Create two separate clients with different Arcs
        let client1 = Arc::new(Client::default());
        let client2 = Arc::new(Client::default());

        let nostr_events1 =
            NostrEvents::new(Arc::clone(&client1), Keys::generate().public_key(), None);
        let nostr_events2 =
            NostrEvents::new(Arc::clone(&client2), Keys::generate().public_key(), None);

        // Different Arc<Client> instances should produce different keys
        assert_ne!(
            nostr_events1.key(),
            nostr_events2.key(),
            "Different Arc<Client> instances should produce different subscription keys"
        );

        // Verify both keys use actual pointer addresses
        let ptr1 = Arc::as_ptr(&nostr_events1.client) as usize as u64;
        let ptr2 = Arc::as_ptr(&nostr_events2.client) as usize as u64;
        assert_ne!(
            ptr1, ptr2,
            "Different Arc instances should have different pointer addresses"
        );
    }

    #[test]
    fn test_subscription_key_different_arc_instances() {
        use tears::SubscriptionSource;

        let client = Client::default();

        // Creating separate Arc instances produces different keys
        let nostr_events1 = NostrEvents::new(
            Arc::new(client.clone()),
            Keys::generate().public_key(),
            None,
        );
        let nostr_events2 = NostrEvents::new(Arc::new(client), Keys::generate().public_key(), None);

        // Different Arc instances should produce different keys
        assert_ne!(
            nostr_events1.key(),
            nostr_events2.key(),
            "Different Arc instances produce different subscription keys"
        );

        // This demonstrates why you must share the same Arc<Client>
        // when you need consistent subscription identity across frames
    }
}
