use color_eyre::eyre::eyre;
use color_eyre::Result;
use nostr_sdk::prelude::*;
use nowhear::Track;
use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::{
    core::state::timeline::TimelineTabType, infrastructure::subscription::nostr::NostrCommand,
};

#[derive(Debug, Clone, Default)]
pub struct NostrState {
    /// Command sender for NostrEvents subscription
    /// This is set when the subscription emits a Ready message
    command_sender: Option<mpsc::UnboundedSender<NostrCommand>>,
    /// Track subscription IDs for each timeline tab
    /// Home tab has 3 subscriptions (backward, forward, profile)
    /// User timelines have 1 subscription
    timeline_subscriptions: HashMap<TimelineTabType, Vec<nostr_sdk::SubscriptionId>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscribeTabError {
    /// Nostr subscription is not ready (no command sender).
    NotConnected,
    /// Home tab subscription is managed during NostrEvents initialization.
    HomeTabIsManaged,
    /// Already subscribed (or subscription is in-flight).
    AlreadySubscribed,
    /// Failed to send command to subscription task.
    SendFailed,
}

impl NostrState {
    pub fn set_command_sender(&mut self, command_sender: mpsc::UnboundedSender<NostrCommand>) {
        self.command_sender = Some(command_sender);
    }

    /// Send a signed event to relays
    pub fn send_event_builder(&self, event_builder: EventBuilder) -> Result<()> {
        if let Some(sender) = &self.command_sender {
            sender
                .send(NostrCommand::SendEventBuilder { event_builder })
                .map_err(|e| e.into())
        } else {
            Err(eyre!("Not connected to Nostr"))
        }
    }

    /// Load more timeline events
    pub fn load_more_timeline(&self, tab_type: TimelineTabType, until: Timestamp) -> Result<()> {
        if let Some(sender) = &self.command_sender {
            sender
                .send(NostrCommand::LoadMoreTimeline { tab_type, until })
                .map_err(|e| e.into())
        } else {
            Err(eyre!("Not connected to Nostr"))
        }
    }

    /// Add a subscription ID for a specific tab type
    pub fn add_timeline_subscription(&mut self, tab_type: TimelineTabType, sub_id: SubscriptionId) {
        self.timeline_subscriptions
            .entry(tab_type)
            .or_default()
            .push(sub_id);
    }

    /// Remove and return all subscription IDs for a specific tab type
    fn remove_timeline_subscription(&mut self, tab_type: &TimelineTabType) -> Vec<SubscriptionId> {
        self.timeline_subscriptions
            .remove(tab_type)
            .unwrap_or_default()
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

    /// Subscribe a timeline for a tab.
    ///
    /// - Home tab is managed by subscription initialization, and cannot be subscribed via command.
    /// - To avoid duplicate subscribe requests (e.g. repeated UI actions), this records an empty
    ///   entry in the local tracking map as "in-flight".
    pub fn subscribe_tab(&mut self, tab_type: &TimelineTabType) -> Result<(), SubscribeTabError> {
        if matches!(tab_type, TimelineTabType::Home) {
            return Err(SubscribeTabError::HomeTabIsManaged);
        }

        if self.timeline_subscriptions.contains_key(tab_type) {
            // Already subscribed or in-flight.
            return Err(SubscribeTabError::AlreadySubscribed);
        }

        let sender = self
            .command_sender
            .as_ref()
            .ok_or(SubscribeTabError::NotConnected)?;

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
            self.timeline_subscriptions.remove(tab_type);
            return Err(SubscribeTabError::SendFailed);
        }

        Ok(())
    }

    /// Unsubscribe subscriptions associated with a specific tab.
    ///
    /// This always removes the local subscription tracking for the tab.
    /// If `command_sender` is available, it also sends an `Unsubscribe` command.
    pub fn unsubscribe_tab(&mut self, tab_type: &TimelineTabType) {
        let subscription_ids = self.remove_timeline_subscription(tab_type);

        if subscription_ids.is_empty() {
            return;
        }

        if let Some(sender) = self.command_sender.as_ref() {
            let _ = sender.send(NostrCommand::Unsubscribe { subscription_ids });
        }
    }

    /// Unsubscribe all subscriptions associated with all tabs.
    ///
    /// This drains the local subscription tracking map to ensure the state is
    /// consistent even if we are already disconnected.
    pub fn unsubscribe_all_tabs(&mut self) {
        let all_subscription_ids: Vec<SubscriptionId> = self
            .timeline_subscriptions
            .drain()
            .flat_map(|(_, ids)| ids)
            .collect();

        if all_subscription_ids.is_empty() {
            return;
        }

        if let Some(sender) = self.command_sender.as_ref() {
            let _ = sender.send(NostrCommand::Unsubscribe {
                subscription_ids: all_subscription_ids,
            });
        }
    }

    /// Gracefully shutdown Nostr connection.
    ///
    /// This unsubscribes all timeline subscriptions, then sends `Shutdown`.
    /// After calling this, `command_sender` is cleared to avoid sending further commands.
    pub fn shutdown(&mut self) {
        self.unsubscribe_all_tabs();

        if let Some(sender) = self.command_sender.as_ref() {
            let _ = sender.send(NostrCommand::Shutdown);
        }

        self.command_sender = None;
    }

    pub fn live_status_with_content_from_track(track: Track) -> Option<(LiveStatus, String)> {
        if track.title.is_empty() || track.artist.is_empty() || track.duration.is_none() {
            return None;
        }

        let content = format!("{} - {}", track.title, track.artist.join(", "));
        let reference =
            percent_encoding::utf8_percent_encode(&content, percent_encoding::NON_ALPHANUMERIC)
                .to_string();
        let status = LiveStatus {
            status_type: StatusType::Music,
            expiration: track.duration.map(|duration| Timestamp::now() + duration),
            reference: Some(reference),
        };

        Some((status, content))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use nostr_sdk::PublicKey;

    #[test]
    fn test_send_event_builder_without_sender_returns_error() -> Result<()> {
        let state = NostrState::default();
        let event_builder = EventBuilder::text_note("foo");

        let result = state.send_event_builder(event_builder);
        assert!(result.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_send_event_builder_with_sender_sends_command() -> Result<()> {
        let mut state = NostrState::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        state.set_command_sender(tx);
        let event_builder = EventBuilder::text_note("foo");

        let result = state.send_event_builder(event_builder.clone());
        assert!(result.is_ok());

        let cmd = rx.recv().await.expect("should receive command");
        assert_eq!(cmd, NostrCommand::SendEventBuilder { event_builder });

        Ok(())
    }

    #[test]
    fn test_load_more_timeline_without_sender_returns_error() -> Result<()> {
        let state = NostrState::default();

        let result = state.load_more_timeline(TimelineTabType::Home, Timestamp::now());
        assert!(result.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_load_more_timeline_with_sender_sends_command() -> Result<()> {
        let mut state = NostrState::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        state.set_command_sender(tx);
        let now = Timestamp::now();

        let result = state.load_more_timeline(TimelineTabType::Home, now);
        assert!(result.is_ok());

        let cmd = rx.recv().await.expect("should receive command");
        assert_eq!(
            cmd,
            NostrCommand::LoadMoreTimeline {
                tab_type: TimelineTabType::Home,
                until: now
            }
        );

        Ok(())
    }

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

    #[test]
    fn test_subscribe_tab_without_sender_returns_error() {
        let mut state = NostrState::default();
        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("valid pubkey");
        let tab_type = TimelineTabType::UserTimeline { pubkey };

        let result = state.subscribe_tab(&tab_type);
        assert_eq!(result, Err(SubscribeTabError::NotConnected));
        assert!(!state.timeline_subscriptions.contains_key(&tab_type));
    }

    #[test]
    fn test_subscribe_tab_home_is_rejected() {
        let mut state = NostrState::default();
        let result = state.subscribe_tab(&TimelineTabType::Home);
        assert_eq!(result, Err(SubscribeTabError::HomeTabIsManaged));
    }

    #[tokio::test]
    async fn test_subscribe_tab_sends_command_and_marks_in_flight() -> color_eyre::Result<()> {
        let mut state = NostrState::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        state.set_command_sender(tx);

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("valid pubkey");
        let tab_type = TimelineTabType::UserTimeline { pubkey };

        state
            .subscribe_tab(&tab_type)
            .map_err(|e| color_eyre::eyre::eyre!(format!("subscribe_tab failed: {e:?}")))?;

        // In-flight marker should exist (empty Vec).
        assert!(state.timeline_subscriptions.contains_key(&tab_type));
        assert_eq!(
            state.timeline_subscriptions.get(&tab_type).cloned(),
            Some(Vec::<SubscriptionId>::new())
        );

        let cmd = rx.recv().await.expect("should receive command");
        match cmd {
            NostrCommand::SubscribeTimeline { tab_type: sent } => {
                assert_eq!(sent, tab_type);
            }
            other => {
                return Err(color_eyre::eyre::eyre!(format!(
                    "unexpected command: {other:?}"
                )))
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_subscribe_tab_duplicate_is_rejected() -> color_eyre::Result<()> {
        let mut state = NostrState::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        state.set_command_sender(tx);

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("valid pubkey");
        let tab_type = TimelineTabType::UserTimeline { pubkey };

        state
            .subscribe_tab(&tab_type)
            .map_err(|e| color_eyre::eyre::eyre!(format!("subscribe_tab failed: {e:?}")))?;
        // Drain first command.
        let _ = rx.recv().await.expect("should receive command");

        let result = state.subscribe_tab(&tab_type);
        assert_eq!(result, Err(SubscribeTabError::AlreadySubscribed));

        Ok(())
    }

    #[test]
    fn test_subscribe_tab_send_failed_does_not_mark_in_flight() {
        let mut state = NostrState::default();

        let (tx, rx) = mpsc::unbounded_channel::<NostrCommand>();
        drop(rx); // make send fail
        state.set_command_sender(tx);

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("valid pubkey");
        let tab_type = TimelineTabType::UserTimeline { pubkey };

        let result = state.subscribe_tab(&tab_type);
        assert_eq!(result, Err(SubscribeTabError::SendFailed));

        // Should not stay marked in-flight.
        assert!(!state.timeline_subscriptions.contains_key(&tab_type));
    }

    #[test]
    fn test_unsubscribe_tab_without_sender_removes_tracking() {
        let mut state = NostrState::default();
        let sub_id1 = nostr_sdk::SubscriptionId::new("sub1");
        let sub_id2 = nostr_sdk::SubscriptionId::new("sub2");

        state.add_timeline_subscription(TimelineTabType::Home, sub_id1.clone());
        state.add_timeline_subscription(TimelineTabType::Home, sub_id2.clone());

        state.unsubscribe_tab(&TimelineTabType::Home);

        assert_eq!(state.find_tab_by_subscription(&sub_id1), None);
        assert_eq!(state.find_tab_by_subscription(&sub_id2), None);
    }

    #[tokio::test]
    async fn test_unsubscribe_tab_with_sender_sends_command() -> color_eyre::Result<()> {
        let mut state = NostrState::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        state.set_command_sender(tx);

        let sub_id1 = nostr_sdk::SubscriptionId::new("sub1");
        let sub_id2 = nostr_sdk::SubscriptionId::new("sub2");

        state.add_timeline_subscription(TimelineTabType::Home, sub_id1.clone());
        state.add_timeline_subscription(TimelineTabType::Home, sub_id2.clone());

        state.unsubscribe_tab(&TimelineTabType::Home);

        let cmd = rx.recv().await.expect("should receive command");
        match cmd {
            NostrCommand::Unsubscribe { subscription_ids } => {
                assert_eq!(subscription_ids.len(), 2);
                assert!(subscription_ids.contains(&sub_id1));
                assert!(subscription_ids.contains(&sub_id2));
            }
            other => {
                return Err(color_eyre::eyre::eyre!(format!(
                    "unexpected command: {other:?}"
                )))
            }
        }

        // Local tracking must be cleared.
        assert_eq!(state.find_tab_by_subscription(&sub_id1), None);
        assert_eq!(state.find_tab_by_subscription(&sub_id2), None);

        Ok(())
    }

    #[test]
    fn test_unsubscribe_all_tabs_without_sender_drains_tracking() {
        let mut state = NostrState::default();
        let sub_home = nostr_sdk::SubscriptionId::new("home");
        let sub_user = nostr_sdk::SubscriptionId::new("user");

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("valid pubkey");
        let user_tab = TimelineTabType::UserTimeline { pubkey };

        state.add_timeline_subscription(TimelineTabType::Home, sub_home.clone());
        state.add_timeline_subscription(user_tab, sub_user.clone());

        state.unsubscribe_all_tabs();

        assert_eq!(state.find_tab_by_subscription(&sub_home), None);
        assert_eq!(state.find_tab_by_subscription(&sub_user), None);
    }

    #[tokio::test]
    async fn test_unsubscribe_all_tabs_with_sender_sends_all_ids() -> color_eyre::Result<()> {
        let mut state = NostrState::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        state.set_command_sender(tx);

        let sub_home1 = nostr_sdk::SubscriptionId::new("home1");
        let sub_home2 = nostr_sdk::SubscriptionId::new("home2");
        let sub_user = nostr_sdk::SubscriptionId::new("user");

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("valid pubkey");
        let user_tab = TimelineTabType::UserTimeline { pubkey };

        state.add_timeline_subscription(TimelineTabType::Home, sub_home1.clone());
        state.add_timeline_subscription(TimelineTabType::Home, sub_home2.clone());
        state.add_timeline_subscription(user_tab.clone(), sub_user.clone());

        state.unsubscribe_all_tabs();

        let cmd = rx.recv().await.expect("should receive command");
        match cmd {
            NostrCommand::Unsubscribe { subscription_ids } => {
                assert_eq!(subscription_ids.len(), 3);
                assert!(subscription_ids.contains(&sub_home1));
                assert!(subscription_ids.contains(&sub_home2));
                assert!(subscription_ids.contains(&sub_user));
            }
            other => {
                return Err(color_eyre::eyre::eyre!(format!(
                    "unexpected command: {other:?}"
                )))
            }
        }

        // Local tracking must be cleared.
        assert_eq!(state.find_tab_by_subscription(&sub_home1), None);
        assert_eq!(state.find_tab_by_subscription(&sub_home2), None);
        assert_eq!(state.find_tab_by_subscription(&sub_user), None);

        Ok(())
    }

    #[tokio::test]
    async fn test_shutdown_unsubscribes_then_sends_shutdown_and_clears_sender(
    ) -> color_eyre::Result<()> {
        let mut state = NostrState::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        state.set_command_sender(tx);

        let sub_home = nostr_sdk::SubscriptionId::new("home");
        state.add_timeline_subscription(TimelineTabType::Home, sub_home.clone());

        state.shutdown();

        let cmd1 = rx.recv().await.expect("should receive first command");
        let cmd2 = rx.recv().await.expect("should receive second command");

        match cmd1 {
            NostrCommand::Unsubscribe { subscription_ids } => {
                assert_eq!(subscription_ids, vec![sub_home]);
            }
            other => {
                return Err(color_eyre::eyre::eyre!(format!(
                    "unexpected command: {other:?}"
                )))
            }
        }

        match cmd2 {
            NostrCommand::Shutdown => {}
            other => {
                return Err(color_eyre::eyre::eyre!(format!(
                    "unexpected command: {other:?}"
                )))
            }
        }

        assert!(state.command_sender.is_none());

        Ok(())
    }

    #[test]
    fn test_live_status_with_content_from_track_returns_none_when_title_is_empty() {
        let track = Track {
            title: String::new(),
            artist: vec!["Queen".to_string()],
            album: None,
            album_artist: None,
            track_number: None,
            duration: Some(Duration::from_secs(120)),
            art_url: None,
        };

        let result = NostrState::live_status_with_content_from_track(track);
        assert_eq!(result, None);
    }

    #[test]
    fn test_live_status_with_content_from_track_returns_none_when_artist_is_empty() {
        let track = Track {
            title: "Bohemian Rhapsody".to_string(),
            artist: Vec::new(),
            album: None,
            album_artist: None,
            track_number: None,
            duration: Some(Duration::from_secs(120)),
            art_url: None,
        };

        let result = NostrState::live_status_with_content_from_track(track);
        assert_eq!(result, None);
    }

    #[test]
    fn test_live_status_with_content_from_track_returns_none_when_duration_is_none() {
        let track = Track {
            title: "Bohemian Rhapsody".to_string(),
            artist: vec!["Queen".to_string()],
            album: None,
            album_artist: None,
            track_number: None,
            duration: None,
            art_url: None,
        };

        let result = NostrState::live_status_with_content_from_track(track);
        assert_eq!(result, None);
    }

    #[test]
    fn test_live_status_with_content_from_track_builds_status_and_content() {
        let duration = Duration::from_secs(120);
        let track = Track {
            title: "Bohemian Rhapsody".to_string(),
            artist: vec!["Queen".to_string(), "Freddie Mercury".to_string()],
            album: None,
            album_artist: None,
            track_number: None,
            duration: Some(duration),
            art_url: None,
        };

        let before = Timestamp::now();
        let result = NostrState::live_status_with_content_from_track(track)
            .expect("should build live status and content");
        let after = Timestamp::now();

        let (status, content) = result;

        let expected_content = "Bohemian Rhapsody - Queen, Freddie Mercury".to_string();
        let expected_reference =
            "Bohemian%20Rhapsody%20%2D%20Queen%2C%20Freddie%20Mercury".to_string();

        assert_eq!(content, expected_content);
        assert_eq!(status.status_type, StatusType::Music);
        assert_eq!(status.reference, Some(expected_reference));

        let expiration = status.expiration.expect("expiration should exist");
        let expected_min = before + duration;
        let expected_max = after + duration;
        assert!(expiration >= expected_min);
        assert!(expiration <= expected_max);
    }
}
