//! Pagination state management for timeline

use nostr_sdk::prelude::*;

/// Manages pagination state for timeline loading
#[derive(Debug, Clone, Default)]
pub struct PaginationState {
    oldest_timestamp: Option<Timestamp>,
    loading_more_since: Option<Timestamp>,
}

impl PaginationState {
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

    /// Update the oldest timestamp if the given timestamp is older
    pub fn update_oldest(&mut self, timestamp: Timestamp) {
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

    /// Start loading more events
    pub fn start_loading_more(&mut self, since: Timestamp) {
        self.loading_more_since = Some(since);
    }

    /// Finish loading more events
    pub fn finish_loading_more(&mut self) {
        self.loading_more_since = None;
    }

    /// Check if currently loading more events
    pub fn is_loading_more(&self) -> bool {
        self.loading_more_since.is_some()
    }

    /// Get the timestamp since which we're loading more
    pub fn loading_more_since(&self) -> Option<Timestamp> {
        self.loading_more_since
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_state_default() {
        let state = PaginationState::new();
        assert_eq!(state.oldest_timestamp(), None);
        assert!(!state.is_loading_more());
    }

    #[test]
    fn test_update_oldest() {
        let mut state = PaginationState::new();

        let ts1 = Timestamp::from(1000);
        let ts2 = Timestamp::from(500);
        let ts3 = Timestamp::from(1500);

        state.update_oldest(ts1);
        assert_eq!(state.oldest_timestamp(), Some(ts1));

        // Older timestamp updates
        state.update_oldest(ts2);
        assert_eq!(state.oldest_timestamp(), Some(ts2));

        // Newer timestamp doesn't update
        state.update_oldest(ts3);
        assert_eq!(state.oldest_timestamp(), Some(ts2));
    }

    #[test]
    fn test_loading_more() {
        let mut state = PaginationState::new();
        let since = Timestamp::from(1000);

        state.start_loading_more(since);
        assert!(state.is_loading_more());
        assert_eq!(state.loading_more_since(), Some(since));

        state.finish_loading_more();
        assert!(!state.is_loading_more());
        assert_eq!(state.loading_more_since(), None);
    }
}
