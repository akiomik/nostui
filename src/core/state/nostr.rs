use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::{
    core::state::timeline::TimelineTabType, infrastructure::subscription::nostr::NostrCommand,
};

#[derive(Debug, Clone, Default)]
pub struct NostrState {
    /// Command sender for NostrEvents subscription
    /// This is set when the subscription emits a Ready message
    pub command_sender: Option<mpsc::UnboundedSender<NostrCommand>>,
    /// Track subscription IDs for each timeline tab
    /// Home tab has 3 subscriptions (backward, forward, profile)
    /// User timelines have 1 subscription
    timeline_subscriptions: HashMap<TimelineTabType, Vec<nostr_sdk::SubscriptionId>>,
}

impl NostrState {
    /// Add a subscription ID for a specific tab type
    pub fn add_timeline_subscription(
        &mut self,
        tab_type: TimelineTabType,
        sub_id: nostr_sdk::SubscriptionId,
    ) {
        self.timeline_subscriptions
            .entry(tab_type)
            .or_default()
            .push(sub_id);
    }

    /// Remove and return all subscription IDs for a specific tab type
    pub fn remove_timeline_subscription(
        &mut self,
        tab_type: &TimelineTabType,
    ) -> Vec<nostr_sdk::SubscriptionId> {
        self.timeline_subscriptions
            .remove(tab_type)
            .unwrap_or_default()
    }

    /// Find the tab type that owns a specific subscription ID
    pub fn find_tab_by_subscription(
        &self,
        subscription_id: &nostr_sdk::SubscriptionId,
    ) -> Option<&TimelineTabType> {
        self.timeline_subscriptions
            .iter()
            .find(|(_, sub_ids)| sub_ids.contains(subscription_id))
            .map(|(tab_type, _)| tab_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::PublicKey;

    #[test]
    fn test_add_timeline_subscription() {
        let mut state = NostrState::default();
        let sub_id1 = nostr_sdk::SubscriptionId::new("sub1");
        let sub_id2 = nostr_sdk::SubscriptionId::new("sub2");

        // Add subscription to Home tab
        state.add_timeline_subscription(TimelineTabType::Home, sub_id1.clone());
        assert_eq!(
            state.find_tab_by_subscription(&sub_id1),
            Some(&TimelineTabType::Home)
        );

        // Add another subscription to Home tab (Home has 3 subscriptions)
        state.add_timeline_subscription(TimelineTabType::Home, sub_id2.clone());
        assert_eq!(
            state.find_tab_by_subscription(&sub_id2),
            Some(&TimelineTabType::Home)
        );
    }

    #[test]
    fn test_add_multiple_tabs() {
        let mut state = NostrState::default();
        let sub_id1 = nostr_sdk::SubscriptionId::new("sub1");
        let sub_id2 = nostr_sdk::SubscriptionId::new("sub2");

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("valid pubkey");
        let user_tab = TimelineTabType::UserTimeline { pubkey };

        // Add subscriptions to different tabs
        state.add_timeline_subscription(TimelineTabType::Home, sub_id1.clone());
        state.add_timeline_subscription(user_tab.clone(), sub_id2.clone());

        assert_eq!(
            state.find_tab_by_subscription(&sub_id1),
            Some(&TimelineTabType::Home)
        );
        assert_eq!(state.find_tab_by_subscription(&sub_id2), Some(&user_tab));
    }

    #[test]
    fn test_remove_timeline_subscription() {
        let mut state = NostrState::default();
        let sub_id1 = nostr_sdk::SubscriptionId::new("sub1");
        let sub_id2 = nostr_sdk::SubscriptionId::new("sub2");

        // Add multiple subscriptions to Home tab
        state.add_timeline_subscription(TimelineTabType::Home, sub_id1.clone());
        state.add_timeline_subscription(TimelineTabType::Home, sub_id2.clone());

        // Remove all subscriptions for Home tab
        let removed = state.remove_timeline_subscription(&TimelineTabType::Home);
        assert_eq!(removed.len(), 2);
        assert!(removed.contains(&sub_id1));
        assert!(removed.contains(&sub_id2));

        // Verify they're no longer found
        assert_eq!(state.find_tab_by_subscription(&sub_id1), None);
        assert_eq!(state.find_tab_by_subscription(&sub_id2), None);
    }

    #[test]
    fn test_remove_nonexistent_subscription() {
        let mut state = NostrState::default();
        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("valid pubkey");
        let user_tab = TimelineTabType::UserTimeline { pubkey };

        // Remove subscriptions for a tab that doesn't exist
        let removed = state.remove_timeline_subscription(&user_tab);
        assert_eq!(removed, Vec::<nostr_sdk::SubscriptionId>::new());
    }

    #[test]
    fn test_find_tab_by_subscription_not_found() {
        let state = NostrState::default();
        let sub_id = nostr_sdk::SubscriptionId::new("nonexistent");

        assert_eq!(state.find_tab_by_subscription(&sub_id), None);
    }

    #[test]
    fn test_multiple_subscriptions_per_tab() {
        let mut state = NostrState::default();
        let sub_id1 = nostr_sdk::SubscriptionId::new("sub1");
        let sub_id2 = nostr_sdk::SubscriptionId::new("sub2");
        let sub_id3 = nostr_sdk::SubscriptionId::new("sub3");

        // Home tab should have 3 subscriptions (backward, forward, profile)
        state.add_timeline_subscription(TimelineTabType::Home, sub_id1.clone());
        state.add_timeline_subscription(TimelineTabType::Home, sub_id2.clone());
        state.add_timeline_subscription(TimelineTabType::Home, sub_id3.clone());

        // All should point to Home tab
        assert_eq!(
            state.find_tab_by_subscription(&sub_id1),
            Some(&TimelineTabType::Home)
        );
        assert_eq!(
            state.find_tab_by_subscription(&sub_id2),
            Some(&TimelineTabType::Home)
        );
        assert_eq!(
            state.find_tab_by_subscription(&sub_id3),
            Some(&TimelineTabType::Home)
        );

        // Remove should return all 3
        let removed = state.remove_timeline_subscription(&TimelineTabType::Home);
        assert_eq!(removed.len(), 3);
    }

    #[test]
    fn test_user_timeline_subscription() {
        let mut state = NostrState::default();
        let sub_id = nostr_sdk::SubscriptionId::new("user_sub");

        let pubkey1 = PublicKey::from_slice(&[1u8; 32]).expect("valid pubkey");
        let pubkey2 = PublicKey::from_slice(&[2u8; 32]).expect("valid pubkey");
        let user_tab1 = TimelineTabType::UserTimeline { pubkey: pubkey1 };
        let user_tab2 = TimelineTabType::UserTimeline { pubkey: pubkey2 };

        // Add subscription to user timeline
        state.add_timeline_subscription(user_tab1.clone(), sub_id.clone());

        // Should find the correct tab
        assert_eq!(state.find_tab_by_subscription(&sub_id), Some(&user_tab1));

        // Should not match a different user tab
        let removed = state.remove_timeline_subscription(&user_tab2);
        assert_eq!(removed, Vec::<nostr_sdk::SubscriptionId>::new());

        // Should still find the original
        assert_eq!(state.find_tab_by_subscription(&sub_id), Some(&user_tab1));
    }
}
