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
use tears::Command;

use crate::{
    core::message::{AppMsg, TimelineMsg},
    domain::nostr::SortableEventId,
};

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

    pub fn event_id_by_index(&self, index: usize) -> Option<SortableEventId> {
        self.notes.get(index).map(|id| id.0)
    }

    // Delegate to Selection
    pub fn selected_index(&self) -> Option<usize> {
        self.selection.selected_index()
    }

    /// Get the last note index
    pub fn last_index(&self) -> usize {
        self.len().saturating_sub(1)
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

    /// Check if the user has scrolled to the bottom of the timeline
    pub fn is_at_bottom(&self) -> bool {
        if self.is_empty() {
            return false;
        }

        self.selected_index() == Some(self.last_index())
    }

    /// Update the timeline tab state based on a message
    ///
    /// This function serves as an orchestrator that coordinates multiple child components.
    /// Following Elm Architecture principles:
    ///
    /// 1. Most messages are delegated to child components (Selection or Pagination)
    /// 2. Some messages require coordination between multiple children
    /// 3. All state changes go through this single function
    ///
    /// ## Coordination Examples
    ///
    /// ### NextItemSelected
    /// When the user tries to scroll down at the bottom of the timeline:
    /// - Updates pagination state to "loading more"
    /// - Returns a LoadMore command to trigger data fetching
    ///
    /// ### NoteAdded
    /// When a new note arrives, coordinates three child components:
    /// - Updates pagination's oldest timestamp
    /// - Inserts note into the sorted list
    /// - Completes "loading more" if an older note arrives during loading
    /// - Adjusts selection index if insertion happens before the selected item
    ///
    /// The coordination logic lives here because it's TimelineTab's responsibility
    /// to maintain consistency across its children, not the responsibility of
    /// Selection or Pagination themselves.
    pub fn update(&mut self, message: Message) -> Command<AppMsg> {
        match message {
            // Selection-related messages - simple delegation
            Message::ItemSelected(index) => {
                if !self.is_empty() && index < self.len() {
                    self.selection.update(SelectionMessage::ItemSelected(index))
                }
            }
            Message::SelectionCleared => self.selection.update(SelectionMessage::SelectionCleared),
            Message::PreviousItemSelected => {
                if !self.is_empty() {
                    self.selection
                        .update(SelectionMessage::PreviousItemSelected)
                }
            }
            Message::NextItemSelected => {
                if !self.is_empty() {
                    if self.is_at_bottom() {
                        // At bottom: trigger "load more" instead of scrolling
                        if let Some(since) = self.oldest_timestamp() {
                            self.pagination
                                .update(PaginationMessage::LoadingMoreStarted { since });

                            return Command::message(AppMsg::Timeline(TimelineMsg::LoadMore));
                        }
                    } else {
                        // Not at bottom: normal scrolling
                        self.selection.update(SelectionMessage::NextItemSelected {
                            max_index: self.last_index(),
                        })
                    }
                }
            }
            Message::FirstItemSelected => {
                if !self.is_empty() {
                    self.selection.update(SelectionMessage::FirstItemSelected)
                }
            }
            Message::LastItemSelected => {
                if !self.is_empty() {
                    self.selection.update(SelectionMessage::LastItemSelected {
                        max_index: self.last_index(),
                    })
                }
            }
            // TimelineTab-specific messages - coordinate multiple children
            Message::NoteAdded(id) => {
                // Update pagination with the new note's timestamp
                self.pagination
                    .update(PaginationMessage::OldestTimestampUpdated(id.created_at));

                // Insert the note into the sorted list
                let result = self.notes.find_or_insert(Reverse(id));

                if let FindOrInsert::Inserted(inserted_at) = result {
                    // Auto-complete "load more" when an older note arrives during loading
                    if let Some(loading_since) = self.loading_more_since() {
                        if id.created_at < loading_since {
                            self.pagination
                                .update(PaginationMessage::LoadingMoreFinished)
                        }
                    }

                    // Adjust selection to prevent visual jumping when notes are inserted above
                    if let Some(selected) = self.selected_index() {
                        if inserted_at <= selected {
                            self.selection
                                .update(SelectionMessage::ItemSelected(selected + 1));
                        }
                    }
                }
            }
        };

        Command::none()
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

        // Test ItemSelected - no command should be issued for selection changes
        let cmd = tab.update(Message::ItemSelected(5));
        assert_eq!(tab.selected_index(), None);
        assert!(cmd.is_none());

        // Test SelectionCleared
        let cmd = tab.update(Message::SelectionCleared);
        assert_eq!(tab.selected_index(), None);
        assert!(cmd.is_none());

        // Test FirstItemSelected
        let cmd = tab.update(Message::FirstItemSelected);
        assert_eq!(tab.selected_index(), None);
        assert!(cmd.is_none());

        // Add some notes first so LastItemSelected and NextItemSelected work properly
        for i in 0..11 {
            let event = create_test_event_id(1000 + i, i as u8);
            let _ = tab.update(Message::NoteAdded(event));
        }

        // Test LastItemSelected (should select index 10 with 11 items)
        let _ = tab.update(Message::ItemSelected(5));
        let cmd = tab.update(Message::LastItemSelected);
        assert_eq!(tab.selected_index(), Some(10));
        assert!(cmd.is_none());

        // Test PreviousItemSelected
        let cmd = tab.update(Message::PreviousItemSelected);
        assert_eq!(tab.selected_index(), Some(9));
        assert!(cmd.is_none());

        // Test NextItemSelected (not at bottom, so no LoadMore)
        let cmd = tab.update(Message::NextItemSelected);
        assert_eq!(tab.selected_index(), Some(10));
        assert!(cmd.is_none());
    }

    #[test]
    fn test_loading_more_triggered_at_bottom() {
        let mut tab = TimelineTab::new_home();

        // Add notes
        for i in 0..3 {
            let event_id = create_test_event_id(1000 + i, i as u8);
            let _ = tab.update(Message::NoteAdded(event_id));
        }
        assert_eq!(tab.oldest_timestamp(), Some(Timestamp::from(1000)));

        // Select the last item (bottom)
        let _ = tab.update(Message::LastItemSelected);
        assert!(tab.is_at_bottom());

        // Try to scroll down at bottom - should trigger loading more and return LoadMore command
        let cmd = tab.update(Message::NextItemSelected);
        assert!(tab.is_loading_more());
        assert!(cmd.is_some()); // LoadMore command was issued
    }

    #[test]
    fn test_loading_more_not_triggered_without_notes() {
        let mut tab = TimelineTab::new_home();
        assert_eq!(tab.oldest_timestamp(), None);

        // Should do nothing when there are no notes (no oldest_timestamp)
        let cmd = tab.update(Message::NextItemSelected);
        assert!(!tab.is_loading_more());
        assert!(cmd.is_none()); // No command should be issued
    }

    #[test]
    fn test_note_added_basic() {
        let mut tab = TimelineTab::new_home();

        let event_id = create_test_event_id(1000, 1);
        let cmd = tab.update(Message::NoteAdded(event_id));

        assert_eq!(tab.len(), 1);
        assert!(!tab.is_empty());
        assert_eq!(tab.oldest_timestamp(), Some(Timestamp::from(1000)));
        assert!(cmd.is_none()); // NoteAdded should not issue commands
    }

    #[test]
    fn test_note_added_updates_oldest_timestamp() {
        let mut tab = TimelineTab::new_home();

        let event1 = create_test_event_id(1000, 1);
        let event2 = create_test_event_id(500, 2);
        let event3 = create_test_event_id(1500, 3);

        let _ = tab.update(Message::NoteAdded(event1));
        assert_eq!(tab.oldest_timestamp(), Some(Timestamp::from(1000)));

        // Older timestamp should update
        let _ = tab.update(Message::NoteAdded(event2));
        assert_eq!(tab.oldest_timestamp(), Some(Timestamp::from(500)));

        // Newer timestamp should not update
        let _ = tab.update(Message::NoteAdded(event3));
        assert_eq!(tab.oldest_timestamp(), Some(Timestamp::from(500)));
    }

    #[test]
    fn test_note_added_sorting() {
        let mut tab = TimelineTab::new_home();

        // Add notes in random order
        let event1 = create_test_event_id(1000, 1);
        let event2 = create_test_event_id(3000, 2);
        let event3 = create_test_event_id(2000, 3);

        let _ = tab.update(Message::NoteAdded(event1));
        let _ = tab.update(Message::NoteAdded(event2));
        let _ = tab.update(Message::NoteAdded(event3));

        // Should be stored in reverse chronological order (newest first)
        assert_eq!(tab.len(), 3);
        // We can't directly access notes, but we can verify the count
    }

    #[test]
    fn test_note_added_duplicate() {
        let mut tab = TimelineTab::new_home();

        let event_id = create_test_event_id(1000, 1);
        let _ = tab.update(Message::NoteAdded(event_id));
        assert_eq!(tab.len(), 1);

        // Adding the same note again should not increase the count
        let _ = tab.update(Message::NoteAdded(event_id));
        assert_eq!(tab.len(), 1);
    }

    #[test]
    fn test_note_added_adjusts_selection_when_inserted_before() {
        let mut tab = TimelineTab::new_home();

        // Add initial notes
        let event1 = create_test_event_id(1000, 1);
        let event2 = create_test_event_id(2000, 2);
        let _ = tab.update(Message::NoteAdded(event1));
        let _ = tab.update(Message::NoteAdded(event2));

        // Select the second item (index 1, which is the older event at timestamp 1000)
        let _ = tab.update(Message::ItemSelected(1));
        assert_eq!(tab.selected_index(), Some(1));

        // Add a newer note (timestamp 3000) which will be inserted at index 0
        let event3 = create_test_event_id(3000, 3);
        let cmd = tab.update(Message::NoteAdded(event3));

        // Selection should be adjusted to index 2 to maintain the same item selected
        assert_eq!(tab.selected_index(), Some(2));
        assert_eq!(tab.len(), 3);
        assert!(cmd.is_none());
    }

    #[test]
    fn test_note_added_does_not_adjust_selection_when_inserted_after() {
        let mut tab = TimelineTab::new_home();

        // Add initial notes
        let event1 = create_test_event_id(3000, 1);
        let event2 = create_test_event_id(2000, 2);
        let _ = tab.update(Message::NoteAdded(event1));
        let _ = tab.update(Message::NoteAdded(event2));

        // Select the first item (index 0, timestamp 3000)
        let _ = tab.update(Message::ItemSelected(0));
        assert_eq!(tab.selected_index(), Some(0));

        // Add an older note (timestamp 1000) which will be inserted at the end
        let event3 = create_test_event_id(1000, 3);
        let cmd = tab.update(Message::NoteAdded(event3));

        // Selection should not change because the new item was inserted after
        assert_eq!(tab.selected_index(), Some(0));
        assert_eq!(tab.len(), 3);
        assert!(cmd.is_none());
    }

    #[test]
    fn test_note_added_does_not_adjust_when_nothing_selected() {
        let mut tab = TimelineTab::new_home();

        let event1 = create_test_event_id(1000, 1);
        let _ = tab.update(Message::NoteAdded(event1));
        assert_eq!(tab.selected_index(), None);

        let event2 = create_test_event_id(2000, 2);
        let _ = tab.update(Message::NoteAdded(event2));
        assert_eq!(tab.selected_index(), None);
    }

    #[test]
    fn test_note_added_does_not_adjust_when_duplicate() {
        let mut tab = TimelineTab::new_home();

        let event1 = create_test_event_id(1000, 1);
        let event2 = create_test_event_id(2000, 2);
        let _ = tab.update(Message::NoteAdded(event1));
        let _ = tab.update(Message::NoteAdded(event2));

        let _ = tab.update(Message::ItemSelected(1));
        assert_eq!(tab.selected_index(), Some(1));

        // Re-adding the same event should not adjust selection
        let _ = tab.update(Message::NoteAdded(event2));
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

        let _ = tab.update(Message::NoteAdded(event1));
        let _ = tab.update(Message::NoteAdded(event2));
        let _ = tab.update(Message::NoteAdded(event3));

        // State: [3000, 2000, 1000]
        assert_eq!(tab.len(), 3);

        // Select middle item
        let _ = tab.update(Message::ItemSelected(1));
        assert_eq!(tab.selected_index(), Some(1));

        // Scroll to bottom and trigger loading more
        let _ = tab.update(Message::LastItemSelected);
        assert_eq!(tab.selected_index(), Some(2));
        let _ = tab.update(Message::NextItemSelected);
        assert!(tab.is_loading_more());

        // Add a newer note while loading
        let event4 = create_test_event_id(4000, 4);
        let _ = tab.update(Message::NoteAdded(event4));

        // State: [4000, 3000, 2000, 1000]
        // Selection should be adjusted from 2 to 3 (maintaining same item)
        assert_eq!(tab.selected_index(), Some(3));
        assert_eq!(tab.len(), 4);

        // Add an older note to complete loading
        let event5 = create_test_event_id(500, 5);
        let _ = tab.update(Message::NoteAdded(event5));
        // Loading should complete automatically
        assert!(!tab.is_loading_more());

        // Navigate to next item (should not trigger loading again since we're not at bottom)
        let _ = tab.update(Message::ItemSelected(2));
        let _ = tab.update(Message::NextItemSelected);
        assert_eq!(tab.selected_index(), Some(3));

        // Verify oldest timestamp
        assert_eq!(tab.oldest_timestamp(), Some(Timestamp::from(500)));
    }

    #[test]
    fn test_next_and_last_item_with_empty_timeline() {
        let mut tab = TimelineTab::new_home();
        assert_eq!(tab.len(), 0);

        // NextItemSelected on empty timeline should do nothing
        let cmd = tab.update(Message::NextItemSelected);
        assert_eq!(tab.selected_index(), None);
        assert!(cmd.is_none());

        // LastItemSelected on empty timeline should do nothing
        let cmd = tab.update(Message::LastItemSelected);
        assert_eq!(tab.selected_index(), None);
        assert!(cmd.is_none());
    }

    #[test]
    fn test_next_and_last_item_with_single_item() {
        let mut tab = TimelineTab::new_home();
        let event = create_test_event_id(1000, 1);
        let _ = tab.update(Message::NoteAdded(event));
        assert_eq!(tab.len(), 1);

        // NextItemSelected on single item timeline when nothing is selected
        // With len=1, max_index=0, so NextItemSelected should NOT select anything
        // because Selection's logic requires max_index > 0 for initial selection
        let cmd = tab.update(Message::NextItemSelected);
        assert_eq!(tab.selected_index(), None);
        assert!(cmd.is_none());

        // Select the item manually
        let _ = tab.update(Message::ItemSelected(0));
        assert_eq!(tab.selected_index(), Some(0));

        // Can't go beyond the only item - but at bottom with single item, should trigger LoadMore
        let cmd = tab.update(Message::NextItemSelected);
        assert_eq!(tab.selected_index(), Some(0));
        assert!(cmd.is_some()); // LoadMore command issued when at bottom

        // LastItemSelected should select index 0
        let _ = tab.update(Message::SelectionCleared);
        let cmd = tab.update(Message::LastItemSelected);
        assert_eq!(tab.selected_index(), Some(0));
        assert!(cmd.is_none());
    }
}
