//! Nostr subscription gateway contract
//!
//! Protocol types exchanged between the application/model and the Nostr
//! subscription adapter (`infrastructure::subscription::nostr`):
//! - `NostrCommand`: commands sent to the subscription worker
//! - `CommandError`: errors that can occur while executing a command
//! - `Message`: messages emitted by the subscription back to the application
//!
//! These types live in `model` (not `domain`) because they reference
//! `FeedKind`, a UI/model concept that the domain layer is intentionally
//! kept free of. The adapter in `infrastructure` depends inward on this
//! contract, inverting what used to be an `application`/`model` -> `infrastructure`
//! dependency.

use nostr_sdk::prelude::*;
use tokio::sync::mpsc;

use crate::domain::nostr::FeedKind;

/// Commands that can be sent to the Nostr subscription
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NostrCommand {
    /// Send an event to relays
    SendEventBuilder { event_builder: EventBuilder },
    /// Add a new relay
    AddRelay { url: String },
    /// Remove a relay
    RemoveRelay { url: String },
    /// Load more events for a feed, older than the given timestamp
    LoadMore { feed: FeedKind, since: Timestamp },
    /// Subscribe to a feed
    Subscribe { feed: FeedKind },
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
        feed: FeedKind,
        subscription_id: nostr_sdk::SubscriptionId,
    },
}
