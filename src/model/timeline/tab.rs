//! Timeline tab model
//!
//! This module follows the Elm Architecture pattern and acts as a parent component
//! that coordinates multiple child components (Selection and Pagination).
//!
//! Design decisions:
//! - Messages are defined at the TimelineTab level rather than wrapping child messages
//!   (e.g., `ItemSelected` instead of `Selection(SelectionMessage::ItemSelected)`)
//! - This keeps the calling code simple and maintains TimelineTab's responsibility
//!   as an orchestrator that coordinates multiple children
//! - The `update` function handles both simple delegation and complex coordination
//!   (e.g., `NoteAdded` updates pagination, notes, and selection together)

use std::cmp::Reverse;

use nostr_sdk::prelude::*;
use sorted_vec::{FindOrInsert, ReverseSortedSet};

use crate::domain::nostr::SortableEventId;

use super::{
    pagination::{Message as PaginationMessage, Pagination},
    selection::{Message as SelectionMessage, Selection},
};

/// Represents the type of timeline tab
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TimelineTabType {
    /// Home timeline (global feed)
    Home,
    /// User timeline (specific author's posts)
    UserTimeline { pubkey: PublicKey },
}

/// Messages that can be sent to update the timeline tab state
///
/// Following Elm conventions, messages are named in past tense.
/// Messages are grouped by their concern:
/// - Selection-related: Control which item is currently selected
/// - Pagination-related: Control loading more items
/// - Tab-specific: Coordinate multiple children (e.g., adding a note)
pub enum Message {
    // Selection-related messages (delegated to Selection)
    /// A specific item was selected by index
    ItemSelected(usize),
    /// The selection was cleared
    SelectionCleared,
    /// The previous item was selected
    PreviousItemSelected,
    /// The next item was selected
    NextItemSelected,
    /// The first item was selected
    FirstItemSelected,
    /// The last item was selected
    LastItemSelected,

    // Pagination-related messages (delegated to Pagination)
    /// Loading more items was started
    LoadingMoreStarted,
    /// Loading more items finished
    LoadingMoreFinished,

    // TimelineTab-specific messages (coordinate multiple children)
    /// A new note was added to the timeline
    /// This requires coordinating pagination, notes list, and selection state
    NoteAdded(SortableEventId),
}

/// Represents a single timeline tab with its own state
#[derive(Debug, Clone)]
pub struct TimelineTab {
    tab_type: TimelineTabType,
    /// Sorted list of event IDs (newest first)
    /// The actual event data is stored in TimelineState::events
    notes: ReverseSortedSet<SortableEventId>,
    selection: Selection,
    pagination: Pagination,
}

impl TimelineTab {
    /// Create a new timeline tab with the specified type
    pub fn new(tab_type: TimelineTabType) -> Self {
        Self {
            tab_type,
            notes: ReverseSortedSet::new(),
            selection: Selection::new(),
            pagination: Pagination::new(),
        }
    }

    /// Create a new Home timeline tab
    pub fn new_home() -> Self {
        Self::new(TimelineTabType::Home)
    }

    pub fn tab_type(&self) -> &TimelineTabType {
        &self.tab_type
    }

    // Delegate to Selection
    pub fn selected_index(&self) -> Option<usize> {
        self.selection.selected_index()
    }

    // Delegate to Pagination
    pub fn oldest_timestamp(&self) -> Option<Timestamp> {
        self.pagination.oldest_timestamp()
    }

    pub fn is_loading_more(&self) -> bool {
        self.pagination.is_loading_more()
    }

    pub fn loading_more_since(&self) -> Option<Timestamp> {
        self.pagination.loading_more_since()
    }

    pub fn len(&self) -> usize {
        self.notes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// Update the timeline tab state based on a message
    ///
    /// This function serves as an orchestrator that coordinates multiple child components.
    /// Following Elm Architecture principles:
    ///
    /// 1. Most messages are delegated to child components (Selection or Pagination)
    /// 2. Some messages require coordination between multiple children (e.g., NoteAdded)
    /// 3. All state changes go through this single function
    ///
    /// The coordination logic (e.g., adjusting selection when a note is added) lives here
    /// because it's the responsibility of TimelineTab to maintain consistency across
    /// its children, not the responsibility of Selection or Pagination.
    pub fn update(&mut self, message: Message) {
        match message {
            // Selection-related messages - simple delegation
            Message::ItemSelected(index) => {
                self.selection.update(SelectionMessage::ItemSelected(index))
            }
            Message::SelectionCleared => self.selection.update(SelectionMessage::SelectionCleared),
            Message::PreviousItemSelected => self
                .selection
                .update(SelectionMessage::PreviousItemSelected),
            Message::NextItemSelected => {
                let max_index = self.notes.len().saturating_sub(1);
                self.selection
                    .update(SelectionMessage::NextItemSelected { max_index })
            }
            Message::FirstItemSelected => {
                self.selection.update(SelectionMessage::FirstItemSelected)
            }
            Message::LastItemSelected => {
                let max_index = self.notes.len().saturating_sub(1);
                self.selection
                    .update(SelectionMessage::LastItemSelected { max_index })
            }

            // Pagination-related messages - simple delegation
            Message::LoadingMoreStarted => {
                if let Some(since) = self.oldest_timestamp() {
                    self.pagination
                        .update(PaginationMessage::LoadingMoreStarted { since });
                }
            }
            Message::LoadingMoreFinished => self
                .pagination
                .update(PaginationMessage::LoadingMoreFinished),

            // TimelineTab-specific messages - coordinate multiple children
            // This is a good example of why TimelineTab needs to orchestrate:
            // - Selection doesn't know about notes or pagination
            // - Pagination doesn't know about notes or selection
            // - Only TimelineTab knows how to coordinate all three
            Message::NoteAdded(id) => {
                // Update pagination with the new note's timestamp
                self.pagination
                    .update(PaginationMessage::OldestTimestampUpdated(id.created_at));

                // Insert the note into the sorted list
                let result = self.notes.find_or_insert(Reverse(id));

                // Adjust selection index if a new item was inserted before the currently selected item
                // This prevents the visual selection from jumping when new notes arrive above it
                if let FindOrInsert::Inserted(inserted_at) = result {
                    if let Some(selected) = self.selected_index() {
                        if inserted_at <= selected {
                            self.selection
                                .update(SelectionMessage::ItemSelected(selected + 1));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_event_id(timestamp: u64, id_suffix: u8) -> SortableEventId {
        let mut id_bytes = [0u8; 32];
        id_bytes[31] = id_suffix; // Make each ID unique
        SortableEventId {
            id: EventId::from_byte_array(id_bytes),
            created_at: Timestamp::from(timestamp),
        }
    }

    #[test]
    fn test_timeline_tab_default() {
        let tab = TimelineTab::new_home();
        assert_eq!(tab.len(), 0);
        assert!(tab.is_empty());
        assert_eq!(tab.selected_index(), None);
        assert_eq!(tab.oldest_timestamp(), None);
        assert!(!tab.is_loading_more());
    }

    #[test]
    fn test_timeline_tab_type() {
        let home_tab = TimelineTab::new_home();
        assert_eq!(home_tab.tab_type, TimelineTabType::Home);

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        let user_tab = TimelineTab::new(TimelineTabType::UserTimeline { pubkey });
        assert_eq!(user_tab.tab_type, TimelineTabType::UserTimeline { pubkey });
    }

    #[test]
    fn test_selection_message_delegation() {
        let mut tab = TimelineTab::new_home();

        // Test ItemSelected
        tab.update(Message::ItemSelected(5));
        assert_eq!(tab.selected_index(), Some(5));

        // Test SelectionCleared
        tab.update(Message::SelectionCleared);
        assert_eq!(tab.selected_index(), None);

        // Test FirstItemSelected
        tab.update(Message::FirstItemSelected);
        assert_eq!(tab.selected_index(), Some(0));

        // Add some notes first so LastItemSelected and NextItemSelected work properly
        for i in 0..11 {
            let event = create_test_event_id(1000 + i, i as u8);
            tab.update(Message::NoteAdded(event));
        }

        // Test LastItemSelected (should select index 10 with 11 items)
        tab.update(Message::ItemSelected(5));
        tab.update(Message::LastItemSelected);
        assert_eq!(tab.selected_index(), Some(10));

        // Test PreviousItemSelected
        tab.update(Message::PreviousItemSelected);
        assert_eq!(tab.selected_index(), Some(9));

        // Test NextItemSelected
        tab.update(Message::NextItemSelected);
        assert_eq!(tab.selected_index(), Some(10));
    }

    #[test]
    fn test_pagination_message_delegation() {
        let mut tab = TimelineTab::new_home();

        // Add a note first to establish oldest_timestamp
        let event_id = create_test_event_id(1000, 1);
        tab.update(Message::NoteAdded(event_id));
        assert_eq!(tab.oldest_timestamp(), Some(Timestamp::from(1000)));

        // Test LoadingMoreStarted - should use the oldest_timestamp
        tab.update(Message::LoadingMoreStarted);
        assert!(tab.is_loading_more());

        // Test LoadingMoreFinished
        tab.update(Message::LoadingMoreFinished);
        assert!(!tab.is_loading_more());
    }

    #[test]
    fn test_loading_more_started_without_notes() {
        let mut tab = TimelineTab::new_home();
        assert_eq!(tab.oldest_timestamp(), None);

        // Should do nothing when there's no oldest_timestamp
        tab.update(Message::LoadingMoreStarted);
        assert!(!tab.is_loading_more());
    }

    #[test]
    fn test_note_added_basic() {
        let mut tab = TimelineTab::new_home();

        let event_id = create_test_event_id(1000, 1);
        tab.update(Message::NoteAdded(event_id));

        assert_eq!(tab.len(), 1);
        assert!(!tab.is_empty());
        assert_eq!(tab.oldest_timestamp(), Some(Timestamp::from(1000)));
    }

    #[test]
    fn test_note_added_updates_oldest_timestamp() {
        let mut tab = TimelineTab::new_home();

        let event1 = create_test_event_id(1000, 1);
        let event2 = create_test_event_id(500, 2);
        let event3 = create_test_event_id(1500, 3);

        tab.update(Message::NoteAdded(event1));
        assert_eq!(tab.oldest_timestamp(), Some(Timestamp::from(1000)));

        // Older timestamp should update
        tab.update(Message::NoteAdded(event2));
        assert_eq!(tab.oldest_timestamp(), Some(Timestamp::from(500)));

        // Newer timestamp should not update
        tab.update(Message::NoteAdded(event3));
        assert_eq!(tab.oldest_timestamp(), Some(Timestamp::from(500)));
    }

    #[test]
    fn test_note_added_sorting() {
        let mut tab = TimelineTab::new_home();

        // Add notes in random order
        let event1 = create_test_event_id(1000, 1);
        let event2 = create_test_event_id(3000, 2);
        let event3 = create_test_event_id(2000, 3);

        tab.update(Message::NoteAdded(event1));
        tab.update(Message::NoteAdded(event2));
        tab.update(Message::NoteAdded(event3));

        // Should be stored in reverse chronological order (newest first)
        assert_eq!(tab.len(), 3);
        // We can't directly access notes, but we can verify the count
    }

    #[test]
    fn test_note_added_duplicate() {
        let mut tab = TimelineTab::new_home();

        let event_id = create_test_event_id(1000, 1);
        tab.update(Message::NoteAdded(event_id));
        assert_eq!(tab.len(), 1);

        // Adding the same note again should not increase the count
        tab.update(Message::NoteAdded(event_id));
        assert_eq!(tab.len(), 1);
    }

    #[test]
    fn test_note_added_adjusts_selection_when_inserted_before() {
        let mut tab = TimelineTab::new_home();

        // Add initial notes
        let event1 = create_test_event_id(1000, 1);
        let event2 = create_test_event_id(2000, 2);
        tab.update(Message::NoteAdded(event1));
        tab.update(Message::NoteAdded(event2));

        // Select the second item (index 1, which is the older event at timestamp 1000)
        tab.update(Message::ItemSelected(1));
        assert_eq!(tab.selected_index(), Some(1));

        // Add a newer note (timestamp 3000) which will be inserted at index 0
        let event3 = create_test_event_id(3000, 3);
        tab.update(Message::NoteAdded(event3));

        // Selection should be adjusted to index 2 to maintain the same item selected
        assert_eq!(tab.selected_index(), Some(2));
        assert_eq!(tab.len(), 3);
    }

    #[test]
    fn test_note_added_does_not_adjust_selection_when_inserted_after() {
        let mut tab = TimelineTab::new_home();

        // Add initial notes
        let event1 = create_test_event_id(3000, 1);
        let event2 = create_test_event_id(2000, 2);
        tab.update(Message::NoteAdded(event1));
        tab.update(Message::NoteAdded(event2));

        // Select the first item (index 0, timestamp 3000)
        tab.update(Message::ItemSelected(0));
        assert_eq!(tab.selected_index(), Some(0));

        // Add an older note (timestamp 1000) which will be inserted at the end
        let event3 = create_test_event_id(1000, 3);
        tab.update(Message::NoteAdded(event3));

        // Selection should not change because the new item was inserted after
        assert_eq!(tab.selected_index(), Some(0));
        assert_eq!(tab.len(), 3);
    }

    #[test]
    fn test_note_added_does_not_adjust_when_nothing_selected() {
        let mut tab = TimelineTab::new_home();

        let event1 = create_test_event_id(1000, 1);
        tab.update(Message::NoteAdded(event1));
        assert_eq!(tab.selected_index(), None);

        let event2 = create_test_event_id(2000, 2);
        tab.update(Message::NoteAdded(event2));
        assert_eq!(tab.selected_index(), None);
    }

    #[test]
    fn test_note_added_does_not_adjust_when_duplicate() {
        let mut tab = TimelineTab::new_home();

        let event1 = create_test_event_id(1000, 1);
        let event2 = create_test_event_id(2000, 2);
        tab.update(Message::NoteAdded(event1));
        tab.update(Message::NoteAdded(event2));

        tab.update(Message::ItemSelected(1));
        assert_eq!(tab.selected_index(), Some(1));

        // Re-adding the same event should not adjust selection
        tab.update(Message::NoteAdded(event2));
        assert_eq!(tab.selected_index(), Some(1));
        assert_eq!(tab.len(), 2);
    }

    #[test]
    fn test_complex_scenario() {
        let mut tab = TimelineTab::new_home();

        // Add some initial notes
        let event1 = create_test_event_id(1000, 1);
        let event2 = create_test_event_id(2000, 2);
        let event3 = create_test_event_id(3000, 3);

        tab.update(Message::NoteAdded(event1));
        tab.update(Message::NoteAdded(event2));
        tab.update(Message::NoteAdded(event3));

        // State: [3000, 2000, 1000]
        assert_eq!(tab.len(), 3);

        // Select middle item
        tab.update(Message::ItemSelected(1));
        assert_eq!(tab.selected_index(), Some(1));

        // Start loading more
        tab.update(Message::LoadingMoreStarted);
        assert!(tab.is_loading_more());

        // Add a newer note while loading
        let event4 = create_test_event_id(4000, 4);
        tab.update(Message::NoteAdded(event4));

        // State: [4000, 3000, 2000, 1000]
        // Selection should be adjusted to 2
        assert_eq!(tab.selected_index(), Some(2));
        assert_eq!(tab.len(), 4);

        // Finish loading
        tab.update(Message::LoadingMoreFinished);
        assert!(!tab.is_loading_more());

        // Navigate to next item
        tab.update(Message::NextItemSelected);
        assert_eq!(tab.selected_index(), Some(3));

        // Verify oldest timestamp
        assert_eq!(tab.oldest_timestamp(), Some(Timestamp::from(1000)));
    }

    #[test]
    fn test_next_and_last_item_with_empty_timeline() {
        let mut tab = TimelineTab::new_home();
        assert_eq!(tab.len(), 0);

        // NextItemSelected on empty timeline should do nothing
        tab.update(Message::NextItemSelected);
        assert_eq!(tab.selected_index(), None);

        // LastItemSelected on empty timeline should select index 0 (which doesn't exist)
        // This is handled by Selection's logic
        tab.update(Message::LastItemSelected);
        assert_eq!(tab.selected_index(), Some(0));
    }

    #[test]
    fn test_next_and_last_item_with_single_item() {
        let mut tab = TimelineTab::new_home();
        let event = create_test_event_id(1000, 1);
        tab.update(Message::NoteAdded(event));
        assert_eq!(tab.len(), 1);

        // NextItemSelected on single item timeline when nothing is selected
        // With len=1, max_index=0, so NextItemSelected should NOT select anything
        // because Selection's logic requires max_index > 0 for initial selection
        tab.update(Message::NextItemSelected);
        assert_eq!(tab.selected_index(), None);

        // Select the item manually
        tab.update(Message::ItemSelected(0));
        assert_eq!(tab.selected_index(), Some(0));

        // Can't go beyond the only item
        tab.update(Message::NextItemSelected);
        assert_eq!(tab.selected_index(), Some(0));

        // LastItemSelected should select index 0
        tab.update(Message::SelectionCleared);
        tab.update(Message::LastItemSelected);
        assert_eq!(tab.selected_index(), Some(0));
    }
}
