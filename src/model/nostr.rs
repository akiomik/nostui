use nostr_sdk::prelude::*;
use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::{
    infrastructure::subscription::nostr::NostrCommand, model::timeline::tab::TimelineTabType,
};

pub enum Message {
    ConnectionReady {
        command_sender: mpsc::UnboundedSender<NostrCommand>,
    },
    EventSubmitted {
        event_builder: EventBuilder,
    },
    SubscriptionRequested {
        tab_type: TimelineTabType,
    },
    SubscriptionCreated {
        tab_type: TimelineTabType,
        sub_id: SubscriptionId,
    },
    SubscriptionClosed {
        tab_type: TimelineTabType,
    },
    HistoryRequested {
        tab_type: TimelineTabType,
        since: Timestamp,
    },
    ConnectionClosed,
}

#[derive(Debug, Clone, Default)]
pub struct Nostr {
    /// Command sender for NostrEvents subscription
    /// This is set when the subscription emits a Ready message
    command_sender: Option<mpsc::UnboundedSender<NostrCommand>>,

    /// Track subscription IDs for each timeline tab
    /// Home tab has 3 subscriptions (backward, forward, profile)
    /// User timelines have 1 subscription
    timeline_subscriptions: HashMap<TimelineTabType, Vec<nostr_sdk::SubscriptionId>>,
}

impl Nostr {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_ready(&self) -> bool {
        self.command_sender.is_some()
    }

    pub fn is_subscribed(&self, tab_type: &TimelineTabType) -> bool {
        self.timeline_subscriptions
            .get(tab_type)
            .is_some_and(|subs| !subs.is_empty())
    }

    /// Find the tab type that owns a specific subscription ID
    pub fn find_tab_by_subscription(
        &self,
        subscription_id: &SubscriptionId,
    ) -> Option<&TimelineTabType> {
        self.timeline_subscriptions
            .iter()
            .find(|(_, sub_ids)| sub_ids.contains(subscription_id))
            .map(|(tab_type, _)| tab_type)
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::ConnectionReady { command_sender } => {
                self.command_sender = Some(command_sender);
            }
            Message::EventSubmitted { event_builder } => {
                if let Some(sender) = self.command_sender.as_ref() {
                    let _ = sender.send(NostrCommand::SendEventBuilder { event_builder });
                }
            }
            Message::SubscriptionRequested { tab_type } => {
                if matches!(tab_type, TimelineTabType::Home) {
                    return;
                }

                if self.timeline_subscriptions.contains_key(&tab_type) {
                    // Already subscribed or in-flight.
                    return;
                }

                if let Some(sender) = self.command_sender.as_ref() {
                    // Mark as in-flight before sending, so repeated calls are rejected.
                    self.timeline_subscriptions
                        .insert(tab_type.clone(), Vec::new());

                    if sender
                        .send(NostrCommand::SubscribeTimeline {
                            tab_type: tab_type.clone(),
                        })
                        .is_err()
                    {
                        // NOTE: Avoid leaving an "in-flight" mark when the command didn't go through.
                        self.timeline_subscriptions.remove(&tab_type);
                    }
                }
            }
            Message::SubscriptionCreated { tab_type, sub_id } => {
                self.timeline_subscriptions
                    .entry(tab_type)
                    .or_default()
                    .push(sub_id);
            }
            Message::SubscriptionClosed { tab_type } => {
                let subscription_ids = self
                    .timeline_subscriptions
                    .remove(&tab_type)
                    .unwrap_or_default();

                if !subscription_ids.is_empty() {
                    if let Some(sender) = self.command_sender.as_ref() {
                        let _ = sender.send(NostrCommand::Unsubscribe { subscription_ids });
                    }
                }

                self.timeline_subscriptions.remove(&tab_type);
            }
            Message::HistoryRequested { tab_type, since } => {
                if let Some(sender) = self.command_sender.as_ref() {
                    let _ = sender.send(NostrCommand::LoadMoreTimeline { tab_type, since });
                }
            }
            Message::ConnectionClosed => {
                if let Some(sender) = self.command_sender.as_ref() {
                    let _ = sender.send(NostrCommand::Shutdown);
                }

                self.command_sender = None;
                self.timeline_subscriptions.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_tab_type() -> TimelineTabType {
        TimelineTabType::UserTimeline {
            pubkey: PublicKey::from_slice(&[0u8; 32]).expect("valid public key"),
        }
    }

    #[test]
    fn test_new_creates_default_instance() {
        let nostr = Nostr::new();

        assert!(nostr.command_sender.is_none());
        assert_eq!(nostr.timeline_subscriptions, HashMap::new());
    }

    #[test]
    fn test_is_ready_returns_false_when_no_command_sender() {
        let nostr = Nostr::new();

        assert!(!nostr.is_ready());
    }

    #[test]
    fn test_is_ready_returns_true_when_command_sender_exists() {
        let mut nostr = Nostr::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        nostr.update(Message::ConnectionReady { command_sender: tx });

        assert!(nostr.is_ready());
    }

    #[test]
    fn test_is_subscribed_returns_false_when_no_subscription() {
        let nostr = Nostr::new();
        let tab_type = create_test_tab_type();

        assert!(!nostr.is_subscribed(&tab_type));
    }

    #[test]
    fn test_is_subscribed_returns_false_when_subscription_list_is_empty() {
        let mut nostr = Nostr::new();
        let tab_type = create_test_tab_type();

        nostr
            .timeline_subscriptions
            .insert(tab_type.clone(), Vec::new());

        assert!(!nostr.is_subscribed(&tab_type));
    }

    #[test]
    fn test_is_subscribed_returns_true_when_subscription_exists() {
        let mut nostr = Nostr::new();
        let tab_type = create_test_tab_type();
        let sub_id = SubscriptionId::new("test_sub");

        nostr.update(Message::SubscriptionCreated {
            tab_type: tab_type.clone(),
            sub_id,
        });

        assert!(nostr.is_subscribed(&tab_type));
    }

    #[test]
    fn test_find_tab_by_subscription_returns_none_when_not_found() {
        let nostr = Nostr::new();
        let sub_id = SubscriptionId::new("test_sub");

        assert_eq!(nostr.find_tab_by_subscription(&sub_id), None);
    }

    #[test]
    fn test_find_tab_by_subscription_returns_tab_type_when_found() {
        let mut nostr = Nostr::new();
        let tab_type = create_test_tab_type();
        let sub_id = SubscriptionId::new("test_sub");

        nostr.update(Message::SubscriptionCreated {
            tab_type: tab_type.clone(),
            sub_id: sub_id.clone(),
        });

        assert_eq!(nostr.find_tab_by_subscription(&sub_id), Some(&tab_type));
    }

    #[test]
    fn test_update_connection_ready_sets_command_sender() {
        let mut nostr = Nostr::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        nostr.update(Message::ConnectionReady { command_sender: tx });

        assert!(nostr.command_sender.is_some());
    }

    #[test]
    fn test_update_event_submitted_sends_command_when_ready() {
        let mut nostr = Nostr::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let event_builder = EventBuilder::text_note("test");

        nostr.update(Message::ConnectionReady { command_sender: tx });

        nostr.update(Message::EventSubmitted {
            event_builder: event_builder.clone(),
        });

        // Verify command was sent
        let received = rx.try_recv();
        assert_eq!(
            received,
            Ok(NostrCommand::SendEventBuilder { event_builder })
        );
    }

    #[test]
    fn test_update_event_submitted_does_nothing_when_not_ready() {
        let mut nostr = Nostr::new();
        let event_builder = EventBuilder::text_note("test");

        // No command sender set, so nothing should happen
        nostr.update(Message::EventSubmitted { event_builder });

        // No panic means test passed
    }

    #[test]
    fn test_update_subscription_requested_ignores_home_tab() {
        let mut nostr = Nostr::new();
        let (tx, mut rx) = mpsc::unbounded_channel();

        nostr.update(Message::ConnectionReady { command_sender: tx });

        nostr.update(Message::SubscriptionRequested {
            tab_type: TimelineTabType::Home,
        });

        // No command should be sent for Home tab
        assert!(rx.try_recv().is_err());
        assert!(!nostr
            .timeline_subscriptions
            .contains_key(&TimelineTabType::Home));
    }

    #[test]
    fn test_update_subscription_requested_creates_subscription() {
        let mut nostr = Nostr::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let tab_type = create_test_tab_type();

        nostr.update(Message::ConnectionReady { command_sender: tx });

        nostr.update(Message::SubscriptionRequested {
            tab_type: tab_type.clone(),
        });

        // Verify in-flight mark was set
        assert!(nostr.timeline_subscriptions.contains_key(&tab_type));
        assert_eq!(
            nostr
                .timeline_subscriptions
                .get(&tab_type)
                .expect("subscription exists"),
            &Vec::<SubscriptionId>::new()
        );

        // Verify command was sent
        let received = rx.try_recv();
        assert_eq!(received, Ok(NostrCommand::SubscribeTimeline { tab_type }));
    }

    #[test]
    fn test_update_subscription_requested_ignores_duplicate_request() {
        let mut nostr = Nostr::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let tab_type = create_test_tab_type();

        nostr.update(Message::ConnectionReady { command_sender: tx });

        nostr.update(Message::SubscriptionRequested {
            tab_type: tab_type.clone(),
        });

        // First command should be sent
        assert!(rx.try_recv().is_ok());

        // Second request should be ignored
        nostr.update(Message::SubscriptionRequested { tab_type });

        // No second command should be sent
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_update_subscription_created_adds_subscription_id() {
        let mut nostr = Nostr::new();
        let tab_type = create_test_tab_type();
        let sub_id = SubscriptionId::new("test_sub");

        nostr.update(Message::SubscriptionCreated {
            tab_type: tab_type.clone(),
            sub_id: sub_id.clone(),
        });

        let subs = nostr
            .timeline_subscriptions
            .get(&tab_type)
            .expect("subscription exists");
        assert_eq!(subs, &vec![sub_id]);
    }

    #[test]
    fn test_update_subscription_created_appends_to_existing_subscriptions() {
        let mut nostr = Nostr::new();
        let tab_type = create_test_tab_type();
        let sub_id1 = SubscriptionId::new("test_sub1");
        let sub_id2 = SubscriptionId::new("test_sub2");

        nostr.update(Message::SubscriptionCreated {
            tab_type: tab_type.clone(),
            sub_id: sub_id1.clone(),
        });

        nostr.update(Message::SubscriptionCreated {
            tab_type: tab_type.clone(),
            sub_id: sub_id2.clone(),
        });

        let subs = nostr
            .timeline_subscriptions
            .get(&tab_type)
            .expect("subscription exists");
        assert_eq!(subs, &vec![sub_id1, sub_id2]);
    }

    #[test]
    fn test_update_subscription_closed_removes_subscription_and_sends_unsubscribe() {
        let mut nostr = Nostr::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let tab_type = create_test_tab_type();
        let sub_id = SubscriptionId::new("test_sub");

        nostr.update(Message::ConnectionReady { command_sender: tx });

        nostr.update(Message::SubscriptionCreated {
            tab_type: tab_type.clone(),
            sub_id: sub_id.clone(),
        });

        nostr.update(Message::SubscriptionClosed {
            tab_type: tab_type.clone(),
        });

        // Verify subscription was removed
        assert!(!nostr.timeline_subscriptions.contains_key(&tab_type));

        // Verify unsubscribe command was sent
        let received = rx.try_recv();
        assert_eq!(
            received,
            Ok(NostrCommand::Unsubscribe {
                subscription_ids: vec![sub_id]
            })
        );
    }

    #[test]
    fn test_update_subscription_closed_handles_non_existent_subscription() {
        let mut nostr = Nostr::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let tab_type = create_test_tab_type();

        nostr.update(Message::ConnectionReady { command_sender: tx });

        nostr.update(Message::SubscriptionClosed { tab_type });

        // No unsubscribe command should be sent for empty subscription list
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_update_history_requested_sends_load_more_command() {
        let mut nostr = Nostr::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let tab_type = create_test_tab_type();
        let since = Timestamp::from(1234567890);

        nostr.update(Message::ConnectionReady { command_sender: tx });

        nostr.update(Message::HistoryRequested {
            tab_type: tab_type.clone(),
            since,
        });

        // Verify command was sent
        let received = rx.try_recv();
        assert_eq!(
            received,
            Ok(NostrCommand::LoadMoreTimeline { tab_type, since })
        );
    }

    #[test]
    fn test_update_connection_closed_clears_state_and_sends_shutdown() {
        let mut nostr = Nostr::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let tab_type = create_test_tab_type();
        let sub_id = SubscriptionId::new("test_sub");

        nostr.update(Message::ConnectionReady { command_sender: tx });

        nostr.update(Message::SubscriptionCreated { tab_type, sub_id });

        nostr.update(Message::ConnectionClosed);

        // Verify state was cleared
        assert!(nostr.command_sender.is_none());
        assert_eq!(nostr.timeline_subscriptions, HashMap::new());

        // Verify shutdown command was sent
        let received = rx.try_recv();
        assert_eq!(received, Ok(NostrCommand::Shutdown));
    }
}
