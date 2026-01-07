use nostr_sdk::prelude::*;
use sorted_vec::ReverseSortedSet;
use std::collections::HashMap;

use crate::domain::{collections::EventSet, nostr::EventWrapper};

/// Timeline-related state
#[derive(Debug, Clone)]
pub struct TimelineState {
    pub notes: ReverseSortedSet<EventWrapper>,
    pub reactions: HashMap<EventId, EventSet>,
    pub reposts: HashMap<EventId, EventSet>,
    pub zap_receipts: HashMap<EventId, EventSet>,
    pub selected_index: Option<usize>,
}

impl Default for TimelineState {
    fn default() -> Self {
        Self {
            notes: ReverseSortedSet::new(),
            reactions: HashMap::new(),
            reposts: HashMap::new(),
            zap_receipts: HashMap::new(),
            selected_index: None,
        }
    }
}

impl TimelineState {
    /// Get the length of the timeline
    pub fn len(&self) -> usize {
        self.notes.len()
    }

    /// Check if the timeline is empty
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// Get the selected note
    pub fn selected_note(&self) -> Option<&Event> {
        self.selected_index
            .and_then(|i| self.notes.get(i))
            .map(|sortable| &sortable.0.event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeline_properties() {
        let state = TimelineState::default();

        assert_eq!(state.len(), 0);
        assert!(state.is_empty());
    }

    #[test]
    fn test_selected_note() {
        let mut state = TimelineState::default();

        // The default is unselected
        assert!(state.selected_note().is_none());

        // Returns None if the index is set, but the timeline is empty
        state.selected_index = Some(0);
        assert_eq!(state.selected_note(), None);
    }
}
