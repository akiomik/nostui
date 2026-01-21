//! Pagination state management for timeline
//!
//! This module follows the Elm Architecture pattern:
//! - State is immutable and changes only through the `update` function
//! - All state transitions are explicitly defined as `Message` variants
//! - The module is self-contained and doesn't know about other timeline components

use nostr_sdk::prelude::*;

/// Messages that can be sent to update the pagination state
///
/// Following Elm conventions, messages are named in past tense
/// to indicate "what happened" rather than "what to do"
pub enum Message {
    /// The oldest timestamp was updated (usually when a new event arrives)
    OldestTimestampUpdated(Timestamp),
    /// Loading more events was started at a specific timestamp
    LoadingMoreStarted { since: Timestamp },
    /// Loading more events finished
    LoadingMoreFinished,
}

/// Manages pagination state for timeline loading
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pagination {
    oldest_timestamp: Option<Timestamp>,
    loading_more_since: Option<Timestamp>,
}

impl Pagination {
    /// Create a new pagination state
    pub fn new() -> Self {
        Self {
            oldest_timestamp: None,
            loading_more_since: None,
        }
    }

    /// Get the oldest timestamp seen so far
    pub fn oldest_timestamp(&self) -> Option<Timestamp> {
        self.oldest_timestamp
    }

    /// Check if currently loading more events
    pub fn is_loading_more(&self) -> bool {
        self.loading_more_since.is_some()
    }

    /// Get the timestamp since which we're loading more
    pub fn loading_more_since(&self) -> Option<Timestamp> {
        self.loading_more_since
    }

    /// Update the pagination state based on a message
    ///
    /// This is the only way to modify the pagination state, following Elm Architecture principles.
    /// All logic is implemented directly in the match arms rather than delegating to private methods,
    /// ensuring a single path for state changes.
    pub fn update(&mut self, message: Message) {
        match message {
            Message::OldestTimestampUpdated(timestamp) => {
                // Only update if the new timestamp is older than the current one
                match self.oldest_timestamp {
                    Some(current) if timestamp < current => {
                        self.oldest_timestamp = Some(timestamp);
                    }
                    None => {
                        self.oldest_timestamp = Some(timestamp);
                    }
                    _ => {}
                }
            }
            Message::LoadingMoreStarted { since } => {
                self.loading_more_since = Some(since);
            }
            Message::LoadingMoreFinished => {
                self.loading_more_since = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_state_default() {
        let state = Pagination::new();
        assert_eq!(state.oldest_timestamp(), None);
        assert!(!state.is_loading_more());
    }

    #[test]
    fn test_update_oldest() {
        let mut state = Pagination::new();

        let ts1 = Timestamp::from(1000);
        let ts2 = Timestamp::from(500);
        let ts3 = Timestamp::from(1500);

        state.update(Message::OldestTimestampUpdated(ts1));
        assert_eq!(state.oldest_timestamp(), Some(ts1));

        // Older timestamp updates
        state.update(Message::OldestTimestampUpdated(ts2));
        assert_eq!(state.oldest_timestamp(), Some(ts2));

        // Newer timestamp doesn't update
        state.update(Message::OldestTimestampUpdated(ts3));
        assert_eq!(state.oldest_timestamp(), Some(ts2));
    }

    #[test]
    fn test_loading_more() {
        let mut state = Pagination::new();
        let since = Timestamp::from(1000);

        state.update(Message::LoadingMoreStarted { since });
        assert!(state.is_loading_more());
        assert_eq!(state.loading_more_since(), Some(since));

        state.update(Message::LoadingMoreFinished);
        assert!(!state.is_loading_more());
        assert_eq!(state.loading_more_since(), None);
    }
}
