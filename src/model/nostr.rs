use nostr_sdk::prelude::*;
use std::collections::HashMap;

use crate::domain::nostr::FeedKind;
use crate::model::nostr_gateway::NostrCommand;

pub enum Message {
    ConnectionReady,
    EventSubmitted {
        event_builder: EventBuilder,
    },
    SubscriptionRequested {
        feed: FeedKind,
    },
    SubscriptionCreated {
        feed: FeedKind,
        sub_id: SubscriptionId,
    },
    SubscriptionClosed {
        feed: FeedKind,
    },
    HistoryRequested {
        feed: FeedKind,
        since: Timestamp,
    },
    ConnectionClosed,
}

/// Follow-up effect the application must dispatch after a [`Nostr`] update.
///
/// `Nostr`, like the rest of `model`, is side-effect free: `update` mutates
/// state and returns this outcome instead of sending on the gateway channel.
/// The application layer owns the command sender and performs the send.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum NostrOutcome {
    /// No command needs to be sent.
    #[default]
    None,
    /// Dispatch this command to the subscription worker.
    Send(NostrCommand),
}

#[derive(Debug, Clone, Default)]
pub struct Nostr {
    /// Whether the subscription worker is connected and accepting commands.
    /// Set when the subscription emits a `Ready` message, cleared on shutdown.
    connected: bool,

    /// Track subscription IDs for each feed
    /// The home feed has 3 subscriptions (backward, forward, profile)
    /// Author feeds have 1 subscription
    feed_subscriptions: HashMap<FeedKind, Vec<nostr_sdk::SubscriptionId>>,
}

impl Nostr {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_ready(&self) -> bool {
        self.connected
    }

    pub fn is_subscribed(&self, feed: &FeedKind) -> bool {
        self.feed_subscriptions
            .get(feed)
            .is_some_and(|subs| !subs.is_empty())
    }

    /// Find the feed that owns a specific subscription ID
    pub fn find_tab_by_subscription(&self, subscription_id: &SubscriptionId) -> Option<&FeedKind> {
        self.feed_subscriptions
            .iter()
            .find(|(_, sub_ids)| sub_ids.contains(subscription_id))
            .map(|(feed, _)| feed)
    }

    pub fn update(&mut self, message: Message) -> NostrOutcome {
        match message {
            Message::ConnectionReady => {
                self.connected = true;
                NostrOutcome::None
            }
            Message::EventSubmitted { event_builder } => {
                if self.connected {
                    NostrOutcome::Send(NostrCommand::SendEventBuilder { event_builder })
                } else {
                    NostrOutcome::None
                }
            }
            Message::SubscriptionRequested { feed } => {
                if matches!(feed, FeedKind::Home) {
                    return NostrOutcome::None;
                }

                if self.feed_subscriptions.contains_key(&feed) {
                    // Already subscribed or in-flight.
                    return NostrOutcome::None;
                }

                if !self.connected {
                    return NostrOutcome::None;
                }

                // Mark as in-flight before dispatching, so repeated calls are rejected.
                self.feed_subscriptions.insert(feed.clone(), Vec::new());
                NostrOutcome::Send(NostrCommand::Subscribe { feed })
            }
            Message::SubscriptionCreated { feed, sub_id } => {
                self.feed_subscriptions
                    .entry(feed)
                    .or_default()
                    .push(sub_id);
                NostrOutcome::None
            }
            Message::SubscriptionClosed { feed } => {
                let subscription_ids = self.feed_subscriptions.remove(&feed).unwrap_or_default();

                if !subscription_ids.is_empty() && self.connected {
                    NostrOutcome::Send(NostrCommand::Unsubscribe { subscription_ids })
                } else {
                    NostrOutcome::None
                }
            }
            Message::HistoryRequested { feed, since } => {
                if self.connected {
                    NostrOutcome::Send(NostrCommand::LoadMore { feed, since })
                } else {
                    NostrOutcome::None
                }
            }
            Message::ConnectionClosed => {
                let was_connected = self.connected;
                self.connected = false;
                self.feed_subscriptions.clear();

                if was_connected {
                    NostrOutcome::Send(NostrCommand::Shutdown)
                } else {
                    NostrOutcome::None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_feed() -> FeedKind {
        FeedKind::Author(PublicKey::from_slice(&[0u8; 32]).expect("valid public key"))
    }

    #[test]
    fn test_new_creates_default_instance() {
        let nostr = Nostr::new();

        assert!(!nostr.is_ready());
        assert_eq!(nostr.feed_subscriptions, HashMap::new());
    }

    #[test]
    fn test_is_ready_returns_false_when_not_connected() {
        let nostr = Nostr::new();

        assert!(!nostr.is_ready());
    }

    #[test]
    fn test_is_ready_returns_true_after_connection_ready() {
        let mut nostr = Nostr::new();

        assert_eq!(nostr.update(Message::ConnectionReady), NostrOutcome::None);

        assert!(nostr.is_ready());
    }

    #[test]
    fn test_is_subscribed_returns_false_when_no_subscription() {
        let nostr = Nostr::new();
        let feed = create_test_feed();

        assert!(!nostr.is_subscribed(&feed));
    }

    #[test]
    fn test_is_subscribed_returns_false_when_subscription_list_is_empty() {
        let mut nostr = Nostr::new();
        let feed = create_test_feed();

        nostr.feed_subscriptions.insert(feed.clone(), Vec::new());

        assert!(!nostr.is_subscribed(&feed));
    }

    #[test]
    fn test_is_subscribed_returns_true_when_subscription_exists() {
        let mut nostr = Nostr::new();
        let feed = create_test_feed();
        let sub_id = SubscriptionId::new("test_sub");

        nostr.update(Message::SubscriptionCreated {
            feed: feed.clone(),
            sub_id,
        });

        assert!(nostr.is_subscribed(&feed));
    }

    #[test]
    fn test_find_tab_by_subscription_returns_none_when_not_found() {
        let nostr = Nostr::new();
        let sub_id = SubscriptionId::new("test_sub");

        assert_eq!(nostr.find_tab_by_subscription(&sub_id), None);
    }

    #[test]
    fn test_find_tab_by_subscription_returns_feed_when_found() {
        let mut nostr = Nostr::new();
        let feed = create_test_feed();
        let sub_id = SubscriptionId::new("test_sub");

        nostr.update(Message::SubscriptionCreated {
            feed: feed.clone(),
            sub_id: sub_id.clone(),
        });

        assert_eq!(nostr.find_tab_by_subscription(&sub_id), Some(&feed));
    }

    #[test]
    fn test_update_connection_ready_sets_connected() {
        let mut nostr = Nostr::new();

        let outcome = nostr.update(Message::ConnectionReady);

        assert_eq!(outcome, NostrOutcome::None);
        assert!(nostr.is_ready());
    }

    #[test]
    fn test_update_event_submitted_returns_send_when_ready() {
        let mut nostr = Nostr::new();
        let event_builder = EventBuilder::text_note("test");

        nostr.update(Message::ConnectionReady);

        let outcome = nostr.update(Message::EventSubmitted {
            event_builder: event_builder.clone(),
        });

        assert_eq!(
            outcome,
            NostrOutcome::Send(NostrCommand::SendEventBuilder { event_builder })
        );
    }

    #[test]
    fn test_update_event_submitted_returns_none_when_not_ready() {
        let mut nostr = Nostr::new();
        let event_builder = EventBuilder::text_note("test");

        let outcome = nostr.update(Message::EventSubmitted { event_builder });

        assert_eq!(outcome, NostrOutcome::None);
    }

    #[test]
    fn test_update_subscription_requested_ignores_home_tab() {
        let mut nostr = Nostr::new();

        nostr.update(Message::ConnectionReady);

        let outcome = nostr.update(Message::SubscriptionRequested {
            feed: FeedKind::Home,
        });

        assert_eq!(outcome, NostrOutcome::None);
        assert!(!nostr.feed_subscriptions.contains_key(&FeedKind::Home));
    }

    #[test]
    fn test_update_subscription_requested_creates_subscription() {
        let mut nostr = Nostr::new();
        let feed = create_test_feed();

        nostr.update(Message::ConnectionReady);

        let outcome = nostr.update(Message::SubscriptionRequested { feed: feed.clone() });

        // Verify in-flight mark was set
        assert!(nostr.feed_subscriptions.contains_key(&feed));
        assert_eq!(
            nostr
                .feed_subscriptions
                .get(&feed)
                .expect("subscription exists"),
            &Vec::<SubscriptionId>::new()
        );

        // Verify the subscribe command was reported
        assert_eq!(
            outcome,
            NostrOutcome::Send(NostrCommand::Subscribe { feed })
        );
    }

    #[test]
    fn test_update_subscription_requested_ignores_request_when_not_connected() {
        let mut nostr = Nostr::new();
        let feed = create_test_feed();

        // Not connected yet, so no in-flight mark and no command.
        let outcome = nostr.update(Message::SubscriptionRequested { feed: feed.clone() });

        assert_eq!(outcome, NostrOutcome::None);
        assert!(!nostr.feed_subscriptions.contains_key(&feed));
    }

    #[test]
    fn test_update_subscription_requested_ignores_duplicate_request() {
        let mut nostr = Nostr::new();
        let feed = create_test_feed();

        nostr.update(Message::ConnectionReady);

        let first = nostr.update(Message::SubscriptionRequested { feed: feed.clone() });
        assert_eq!(
            first,
            NostrOutcome::Send(NostrCommand::Subscribe { feed: feed.clone() })
        );

        // Second request should be ignored
        let second = nostr.update(Message::SubscriptionRequested { feed });
        assert_eq!(second, NostrOutcome::None);
    }

    #[test]
    fn test_update_subscription_created_adds_subscription_id() {
        let mut nostr = Nostr::new();
        let feed = create_test_feed();
        let sub_id = SubscriptionId::new("test_sub");

        nostr.update(Message::SubscriptionCreated {
            feed: feed.clone(),
            sub_id: sub_id.clone(),
        });

        let subs = nostr
            .feed_subscriptions
            .get(&feed)
            .expect("subscription exists");
        assert_eq!(subs, &vec![sub_id]);
    }

    #[test]
    fn test_update_subscription_created_appends_to_existing_subscriptions() {
        let mut nostr = Nostr::new();
        let feed = create_test_feed();
        let sub_id1 = SubscriptionId::new("test_sub1");
        let sub_id2 = SubscriptionId::new("test_sub2");

        nostr.update(Message::SubscriptionCreated {
            feed: feed.clone(),
            sub_id: sub_id1.clone(),
        });

        nostr.update(Message::SubscriptionCreated {
            feed: feed.clone(),
            sub_id: sub_id2.clone(),
        });

        let subs = nostr
            .feed_subscriptions
            .get(&feed)
            .expect("subscription exists");
        assert_eq!(subs, &vec![sub_id1, sub_id2]);
    }

    #[test]
    fn test_update_subscription_closed_removes_subscription_and_returns_unsubscribe() {
        let mut nostr = Nostr::new();
        let feed = create_test_feed();
        let sub_id = SubscriptionId::new("test_sub");

        nostr.update(Message::ConnectionReady);

        nostr.update(Message::SubscriptionCreated {
            feed: feed.clone(),
            sub_id: sub_id.clone(),
        });

        let outcome = nostr.update(Message::SubscriptionClosed { feed: feed.clone() });

        // Verify subscription was removed
        assert!(!nostr.feed_subscriptions.contains_key(&feed));

        // Verify unsubscribe command was reported
        assert_eq!(
            outcome,
            NostrOutcome::Send(NostrCommand::Unsubscribe {
                subscription_ids: vec![sub_id]
            })
        );
    }

    #[test]
    fn test_update_subscription_closed_handles_non_existent_subscription() {
        let mut nostr = Nostr::new();
        let feed = create_test_feed();

        nostr.update(Message::ConnectionReady);

        let outcome = nostr.update(Message::SubscriptionClosed { feed });

        // No unsubscribe command for an empty subscription list
        assert_eq!(outcome, NostrOutcome::None);
    }

    #[test]
    fn test_update_history_requested_returns_load_more_command() {
        let mut nostr = Nostr::new();
        let feed = create_test_feed();
        let since = Timestamp::from(1234567890);

        nostr.update(Message::ConnectionReady);

        let outcome = nostr.update(Message::HistoryRequested {
            feed: feed.clone(),
            since,
        });

        assert_eq!(
            outcome,
            NostrOutcome::Send(NostrCommand::LoadMore { feed, since })
        );
    }

    #[test]
    fn test_update_connection_closed_clears_state_and_returns_shutdown() {
        let mut nostr = Nostr::new();
        let feed = create_test_feed();
        let sub_id = SubscriptionId::new("test_sub");

        nostr.update(Message::ConnectionReady);

        nostr.update(Message::SubscriptionCreated { feed, sub_id });

        let outcome = nostr.update(Message::ConnectionClosed);

        // Verify state was cleared
        assert!(!nostr.is_ready());
        assert_eq!(nostr.feed_subscriptions, HashMap::new());

        // Verify shutdown command was reported
        assert_eq!(outcome, NostrOutcome::Send(NostrCommand::Shutdown));
    }

    #[test]
    fn test_update_connection_closed_returns_none_when_not_connected() {
        let mut nostr = Nostr::new();

        let outcome = nostr.update(Message::ConnectionClosed);

        assert_eq!(outcome, NostrOutcome::None);
    }
}
