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
    /// every relay refused therefore arrives as `Ok` with an empty `success` set, and
    /// reporting that as published would be the very claim this is here to stop.
    ///
    /// One relay accepting is enough — the event exists on the network — so a partial
    /// failure is logged rather than reported.
    ///
    /// "Accepting" means an `OK true`, checked rather than assumed. `success` also holds
    /// `EventSendStatus::Sent` — written to a socket, never acknowledged — under any
    /// policy but `AckPolicy::all`, so counting the set's size would make this correct
    /// only for as long as nobody changes the policy at the send. Checking the status
    /// makes a change there report failure loudly instead of reinstating the claim this
    /// exists to remove.
    fn relay_verdict(output: &SendEventOutput) -> Result<(), String> {
        if output.success.values().any(EventSendStatus::is_ack) {
            if !output.failed.is_empty() {
                log::warn!(
                    "Event {} accepted by {} relay(s), refused by {}",
                    output.value,
                    output
                        .success
                        .values()
                        .filter(|status| status.is_ack())
                        .count(),
                    Self::describe_failures(&output.failed)
                );
            }
            return Ok(());
        }

        // Everything to the log, a summary to the caller: this string ends up on one
        // line of the status bar, and losing reasons from the log too would leave the
        // failure undiagnosable anywhere.
        if output.failed.is_empty() {
            log::error!(
                "No relay accepted event {}, and none reported why",
                output.value
            );
        } else {
            log::error!(
                "No relay accepted event {}: {}",
                output.value,
                Self::describe_failures(&output.failed)
            );
        }

        Err(if output.failed.is_empty() {
            String::from("no relay accepted the event")
        } else {
            format!(
                "no relay accepted the event: {}",
                Self::summarise_failures(&output.failed)
            )
        })
    }

    /// How many relays' reasons a *reported* failure names before summarising the rest.
    ///
    /// The status bar is one non-wrapping line and the published content follows this
    /// string, so naming every relay would push the content off the end. The log is not
    /// so constrained, and is the only place the reasons can be read at leisure — so it
    /// gets all of them.
    const REPORTED_FAILURES: usize = 2;

    /// Per-relay failures as `url: reason`, in a stable order so the same outcome always
    /// reads the same way — `failed` is a `HashMap`.
    ///
    /// The one place this rendering exists. The log line and the status line describe the
    /// same failure and must not be able to describe it differently, so the short form
    /// below shortens this rather than rebuilding it.
    fn sorted_failures(failed: &HashMap<RelayUrl, String>) -> Vec<String> {
        let mut reasons: Vec<String> = failed
            .iter()
            .map(|(url, reason)| format!("{url}: {reason}"))
            .collect();
        reasons.sort();
        reasons
    }

    /// Every reason, for the log.
    fn describe_failures(failed: &HashMap<RelayUrl, String>) -> String {
        Self::sorted_failures(failed).join(", ")
    }

    /// The same list, shortened for a status bar that can show one line of it.
    fn summarise_failures(failed: &HashMap<RelayUrl, String>) -> String {
        let mut reasons = Self::sorted_failures(failed);

        let hidden = reasons.len().saturating_sub(Self::REPORTED_FAILURES);
        reasons.truncate(Self::REPORTED_FAILURES);

        if hidden == 0 {
            reasons.join(", ")
        } else {
            format!("{} (and {hidden} more)", reasons.join(", "))
        }
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
            NostrCommand::SendEventBuilder { id, event_builder } => {
                let result: Result<(), String> = match keys {
                    Some(keys) => match event_builder.finalize(keys) {
                        // `AckPolicy::all` is nostr-sdk's default, and stating it here
                        // rather than inheriting it is what makes `relay_verdict` sound:
                        // under any other policy a relay lands in `output.success` as
                        // `Sent` — dispatched, not acknowledged — and reporting that as
                        // published is the claim this whole change removes.
                        Ok(event) => {
                            match client.send_event(&event).ack_policy(AckPolicy::all()).await {
                                Ok(output) => Self::relay_verdict(&output),
                                Err(e) => Err(e.to_string()),
                            }
                        }
                        Err(e) => Err(e.to_string()),
                    },
                    None => Err(String::from("cannot send events in read-only mode")),
                };

                // Reported either way, when anybody is waiting. The application shows a
                // publish as pending until this arrives, so a success that says nothing
                // would leave it pending for good.
                match id {
                    Some(id) => {
                        let _ = msg_tx.send(Message::EventPublished { id, result });
                    }
                    // Nobody is waiting on it, but a failure still has to land somewhere:
                    // #521 keeps it off the status bar, not out of the log. Read-only
                    // mode fails every one of these, and without this there would be no
                    // trace of it anywhere.
                    None => {
                        if let Err(reason) = result {
                            log::error!("Untracked publish failed: {reason}");
                        }
                    }
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

        Self::report_unsent(&mut cmd_rx, &msg_tx).await;
    }

    /// Fail every publish still queued when the worker stops.
    ///
    /// Close first, then drain. `cmd_rx` would otherwise stay open until the task
    /// returns, and a publish sent in that window would be accepted by the sender and
    /// dequeued by nobody — leaving the application tracking one that can never be
    /// reported. Drained with `recv` rather than `try_recv` because `send` bumps the
    /// message count before it pushes the value, so `try_recv` reports an empty queue as
    /// `Empty` while a send is mid-flight and would end the loop early; `recv` waits it
    /// out and returns `None` only once the channel is closed and genuinely drained.
    async fn report_unsent(
        cmd_rx: &mut mpsc::UnboundedReceiver<NostrCommand>,
        msg_tx: &mpsc::UnboundedSender<Message>,
    ) {
        cmd_rx.close();

        while let Some(cmd) = cmd_rx.recv().await {
            if let NostrCommand::SendEventBuilder { id: Some(id), .. } = cmd {
                let _ = msg_tx.send(Message::EventPublished {
                    id,
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
    use crate::model::nostr_gateway::PublishId;
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
        // The premise of this whole change: nostr-sdk returns `Ok` once it has tried
        // every relay, so an all-refused send arrives as `Ok` with nothing in `success`.
        // Reporting that as published would put "Posted" on screen for a note no relay
        // stored — which is the bug this exists to remove.
        let output = send_output(&[], &[("wss://a.example", "blocked")]);

        let error = NostrEvents::relay_verdict(&output).expect_err("should be a failure");
        assert!(
            error.contains("no relay accepted") && error.contains("blocked"),
            "expected the relay's reason to survive, got: {error}"
        );
    }

    #[test]
    fn a_send_no_relay_answered_is_a_failure() {
        assert!(NostrEvents::relay_verdict(&send_output(&[], &[])).is_err());
    }

    #[test]
    fn a_send_nobody_acknowledged_is_not_a_publish() {
        // `Sent` means written to a socket without waiting for an `OK`. It appears under
        // any policy but `AckPolicy::all`, so if that setting is ever dropped from the
        // send this must report failure rather than quietly calling it published.
        //
        // The accepting direction cannot be tested from here: `EventSendStatus::Ack`
        // wraps an `EventSendAcknowledgement` with no public constructor.
        let output = send_output(&["wss://a.example"], &[("wss://b.example", "rate-limited")]);

        assert!(NostrEvents::relay_verdict(&output).is_err());
    }

    #[test]
    fn failure_reasons_read_the_same_way_every_time() {
        // `failed` is a `HashMap`, so without sorting the same outcome would render
        // differently from run to run.
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
    fn the_short_failure_list_is_a_prefix_of_the_full_one() {
        // The log line and the status line describe the same failure. They may differ in
        // length; they must not differ in what they say about the relays they both name.
        let failed: Vec<(String, String)> = (0..5)
            .map(|i| (format!("wss://relay{i}.example"), format!("reason {i}")))
            .collect();
        let map: HashMap<RelayUrl, String> = failed
            .iter()
            .map(|(url, reason)| (relay_url(url), reason.clone()))
            .collect();

        let full = NostrEvents::describe_failures(&map);
        let short = NostrEvents::summarise_failures(&map);
        let named = short.split(" (and ").next().expect("the named part");

        assert!(
            full.starts_with(named),
            "the short form must name the same relays in the same order\n full:  {full}\n short: {short}"
        );
    }

    #[test]
    fn a_failure_message_does_not_grow_with_the_relay_count() {
        // The status bar is one non-wrapping line and the published content follows this
        // string, so naming every relay would push the content off the end.
        let failed: Vec<(String, String)> = (0..6)
            .map(|i| (format!("wss://relay{i}.example"), format!("reason {i}")))
            .collect();
        let borrowed: Vec<(&str, &str)> = failed
            .iter()
            .map(|(url, reason)| (url.as_str(), reason.as_str()))
            .collect();

        let error = NostrEvents::relay_verdict(&send_output(&[], &borrowed))
            .expect_err("should be a failure");

        assert!(error.contains("(and 4 more)"), "got: {error}");
        assert!(!error.contains("relay5.example"), "got: {error}");
    }

    #[tokio::test]
    async fn a_queued_publish_is_failed_when_the_worker_stops() {
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();

        for cmd in [
            NostrCommand::SendEventBuilder {
                id: Some(PublishId(7)),
                event_builder: EventBuilder::new(Kind::TextNote, "hi"),
            },
            // Nobody waiting on this one, and this one is not a publish at all.
            NostrCommand::SendEventBuilder {
                id: None,
                event_builder: EventBuilder::new(Kind::TextNote, "background"),
            },
            NostrCommand::Subscribe {
                feed: FeedKind::Home,
            },
        ] {
            cmd_tx.send(cmd).expect("the receiver is alive");
        }

        NostrEvents::report_unsent(&mut cmd_rx, &msg_tx).await;

        // The tracked one is reported, so the application does not leave it on "Sending".
        let Some(Message::EventPublished { id, result }) = msg_rx.recv().await else {
            panic!("the queued publish should have been reported");
        };
        assert_eq!(id, PublishId(7));
        assert!(result.is_err());

        // Nothing else is: no one is waiting on the other two.
        assert!(msg_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn draining_shuts_the_channel_so_nothing_is_accepted_and_lost() {
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let (msg_tx, _msg_rx) = mpsc::unbounded_channel();

        NostrEvents::report_unsent(&mut cmd_rx, &msg_tx).await;

        // A send after this point must fail rather than land in a channel no one reads —
        // that is what `close` before the drain buys, and what tells the application to
        // settle the publish itself.
        assert!(cmd_tx.send(NostrCommand::Shutdown).is_err());
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
