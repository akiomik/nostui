pub mod pagination;
pub mod selection;
pub mod tab;
pub mod text_note;

use std::collections::HashMap;

use nostr_sdk::prelude::*;
use tears::Command;

use crate::{
    core::message::AppMsg,
    domain::nostr::SortableEventId,
    model::timeline::{
        tab::{Message as TabMessage, TimelineTab, TimelineTabType},
        text_note::{Message as TextNoteMessage, TextNote},
    },
};

pub enum Message {
    NoteAddedToTab {
        event: Event,
        tab_type: TimelineTabType,
    },
    ReactionAdded {
        event: Event,
    },
    RepostAdded {
        event: Event,
    },
    ZapReceiptAdded {
        event: Event,
    },
    PreviousItemSelected,
    NextItemSelected,
    FirstItemSelected,
    LastItemSelected,
    ItemSelected {
        index: usize,
    },
    ItemSelectionCleared,
    TabAdded {
        tab_type: TimelineTabType,
    },
    TabRemoved {
        index: usize,
    },
    TabSelected {
        index: usize,
    },
    NextTabSelected,
    PreviousTabSelected,
}

impl Message {
    /// Check if this message represents a user operation
    ///
    /// User operations are navigation and selection actions that should be blocked during loading.
    /// Data updates (notes, reactions, etc.) and tab management (adding/removing tabs) are not
    /// considered user operations and are allowed during loading.
    pub fn is_user_operation(&self) -> bool {
        matches!(
            self,
            Message::PreviousItemSelected
                | Message::NextItemSelected
                | Message::FirstItemSelected
                | Message::LastItemSelected
                | Message::ItemSelected { .. }
                | Message::ItemSelectionCleared
                | Message::TabSelected { .. }
                | Message::NextTabSelected
                | Message::PreviousTabSelected
        )
    }
}

#[derive(Debug, Clone)]
pub struct Timeline {
    // Tab management
    tabs: Vec<TimelineTab>,
    active_tab_index: usize,

    // Centralized event storage (shared across all tabs)
    // Each event is stored once here and referenced by EventId from tabs
    notes: HashMap<EventId, TextNote>,

    // Loading state for initial load
    is_loading: bool,
}

impl Timeline {
    /// Extract the last event ID from 'e' tags
    fn find_event_id_from_last_e_tag(event: &Event) -> Option<EventId> {
        event
            .tags
            .filter_standardized(TagKind::SingleLetter(SingleLetterTag::lowercase(
                Alphabet::E,
            )))
            .last()
            .and_then(|tag| match tag {
                TagStandard::Event { event_id, .. } => Some(*event_id),
                _ => None,
            })
    }

    /// Create a Timeline
    pub fn new(&self) -> Self {
        Self::default()
    }

    /// Get all tabs
    pub fn tabs(&self) -> &[TimelineTab] {
        &self.tabs
    }

    /// Get the active tab index
    pub fn active_tab_index(&self) -> usize {
        self.active_tab_index
    }

    /// Check if timeline is loading
    pub fn is_loading(&self) -> bool {
        self.is_loading
    }

    /// Get the last tab index
    pub fn last_tab_index(&self) -> usize {
        self.tabs.len().saturating_sub(1)
    }

    /// Get the active tab
    ///
    /// # Panics
    /// Panics if active_tab_index is out of bounds (this indicates a bug in the implementation)
    pub fn active_tab(&self) -> &TimelineTab {
        self.tabs
            .get(self.active_tab_index)
            .expect("BUG: active_tab_index is out of bounds")
    }

    /// Get the active tab mutably
    ///
    /// # Panics
    /// Panics if active_tab_index is out of bounds (this indicates a bug in the implementation)
    fn active_tab_mut(&mut self) -> &mut TimelineTab {
        self.tabs
            .get_mut(self.active_tab_index)
            .expect("BUG: active_tab_index is out of bounds")
    }

    /// Find a tab by its type
    /// Returns the index of the tab if found, or None if not found
    pub fn find_tab_by_type(&self, tab_type: &TimelineTabType) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.tab_type() == tab_type)
    }

    /// Get the length of the active timeline
    pub fn len(&self) -> usize {
        self.active_tab().len()
    }

    /// Check if the active timeline is empty
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// Get the index of currently selected note in the active tab
    pub fn selected_index(&self) -> Option<usize> {
        self.active_tab().selected_index()
    }

    /// Get the currently selected note from the active tab
    pub fn selected_note(&self) -> Option<&TextNote> {
        // Get the SortableEventId from the selected index, then look up in the HashMap
        let index = self.selected_index()?;
        self.note_by_index(index)
    }

    pub fn note_by_index(&self, index: usize) -> Option<&TextNote> {
        let event_id = self.active_tab().event_id_by_index(index)?.id;
        self.notes.get(&event_id)
    }

    /// Check if currently loading more events
    pub fn is_loading_more_for_tab(&self, tab_type: &TimelineTabType) -> Option<bool> {
        self.find_tab_by_type(tab_type)
            .map(|index| self.tabs[index].is_loading_more())
    }

    /// Get the oldest timestamp in the active timeline (for pagination)
    pub fn oldest_timestamp(&self) -> Option<Timestamp> {
        self.active_tab().oldest_timestamp()
    }

    /// Check if the user has scrolled to the bottom of the active timeline
    pub fn is_at_bottom(&self) -> bool {
        let tab = self.active_tab();
        tab.is_at_bottom()
    }

    pub fn update(&mut self, message: Message) -> Command<AppMsg> {
        // Block user operations during initial loading
        if self.is_loading && message.is_user_operation() {
            return Command::none();
        }

        match message {
            Message::NoteAddedToTab { event, tab_type } => {
                // Mark initial loading as complete when first note arrives
                self.is_loading = false;

                // Find the tab index for the specified tab type
                let tab_index = match self.find_tab_by_type(&tab_type) {
                    Some(index) => index,
                    None => {
                        // Tab not found - cannot add note
                        log::warn!("Cannot add note: tab {tab_type:?} not found");
                        return Command::none();
                    }
                };

                // Store event in centralized storage
                let event_id = event.id;
                let created_at = event.created_at;
                self.notes
                    .entry(event_id)
                    .or_insert_with(|| TextNote::new(event.clone()));

                // Create SortableEventId and insert into tab
                let sortable_id = SortableEventId::new(event_id, created_at);
                let tab = &mut self.tabs[tab_index];

                // Store the insert result
                return tab.update(TabMessage::NoteAdded(sortable_id));
            }
            Message::ReactionAdded { event } => {
                if let Some(target_event_id) = Self::find_event_id_from_last_e_tag(&event) {
                    self.notes.entry(target_event_id).and_modify(|note| {
                        note.update(TextNoteMessage::ReactionReceived(event));
                    });
                }
            }
            Message::RepostAdded { event } => {
                if let Some(target_event_id) = Self::find_event_id_from_last_e_tag(&event) {
                    self.notes.entry(target_event_id).and_modify(|note| {
                        note.update(TextNoteMessage::RepostReceived(event));
                    });
                }
            }
            Message::ZapReceiptAdded { event } => {
                if let Some(target_event_id) = Self::find_event_id_from_last_e_tag(&event) {
                    self.notes.entry(target_event_id).and_modify(|note| {
                        note.update(TextNoteMessage::ZapReceiptReceived(event));
                    });
                }
            }
            Message::PreviousItemSelected => {
                let tab = self.active_tab_mut();
                return tab.update(TabMessage::PreviousItemSelected);
            }
            Message::NextItemSelected => {
                let tab = self.active_tab_mut();
                return tab.update(TabMessage::NextItemSelected);
            }
            Message::FirstItemSelected => {
                let tab = self.active_tab_mut();
                return tab.update(TabMessage::FirstItemSelected);
            }
            Message::LastItemSelected => {
                let tab = self.active_tab_mut();
                return tab.update(TabMessage::LastItemSelected);
            }
            Message::ItemSelected { index } => {
                let tab = self.active_tab_mut();
                return tab.update(TabMessage::ItemSelected(index));
            }
            Message::ItemSelectionCleared => {
                let tab = self.active_tab_mut();
                return tab.update(TabMessage::SelectionCleared);
            }
            Message::TabAdded { tab_type } => {
                // Check if a tab with the same type already exists
                if self.find_tab_by_type(&tab_type).is_some() {
                    log::warn!("Tab with this type already exists");
                    return Command::none();
                }

                // Create and add the new tab
                let new_tab = TimelineTab::new(tab_type);
                self.tabs.push(new_tab);

                // Switch to the new tab
                self.active_tab_index = self.last_tab_index();
            }
            Message::TabRemoved { index } => {
                // Validate index
                if index >= self.tabs.len() {
                    log::warn!("Tab index out of bounds");
                    return Command::none();
                }

                // Cannot remove the Home tab
                if matches!(self.tabs[index].tab_type(), TimelineTabType::Home) {
                    log::warn!("Cannot remove the Home tab");
                    return Command::none();
                }

                // Remove the tab
                self.tabs.remove(index);

                // Adjust active_tab_index if necessary
                if self.active_tab_index >= self.tabs.len() {
                    // If we removed the last tab and it was active, move to the previous tab
                    self.active_tab_index = self.last_tab_index();
                } else if index < self.active_tab_index {
                    // If we removed a tab before the active one, adjust the index
                    self.active_tab_index -= 1;
                } else if index == self.active_tab_index {
                    // If we removed the active tab, stay at the same index (which now points to the next tab)
                    // or move to the last tab if we removed the last one
                    if self.active_tab_index >= self.tabs.len() {
                        self.active_tab_index = self.last_tab_index();
                    }
                }
            }
            Message::TabSelected { index } => {
                if index < self.tabs.len() {
                    self.active_tab_index = index;
                }
            }
            Message::NextTabSelected => {
                if self.active_tab_index < self.tabs.len() - 1 {
                    self.active_tab_index += 1;
                }
            }
            Message::PreviousTabSelected => {
                self.active_tab_index = self.active_tab_index.saturating_sub(1);
            }
        }

        Command::none()
    }
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            tabs: vec![TimelineTab::new_home()],
            active_tab_index: 0,
            notes: HashMap::new(),
            is_loading: true,
        }
    }
}

#[cfg(test)]
impl Timeline {
    /// Create a Timeline for testing with loading completed
    ///
    /// This is a convenience method for tests that don't need to test loading behavior.
    pub fn new_loaded() -> Self {
        Self {
            tabs: vec![TimelineTab::new_home()],
            active_tab_index: 0,
            notes: HashMap::new(),
            is_loading: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create test events
    fn create_test_event(timestamp: u64, id_suffix: u8, content: &str) -> Event {
        let keys = Keys::generate();
        let mut id_bytes = [0u8; 32];
        id_bytes[31] = id_suffix; // Make each ID unique

        // Create a basic text note event
        EventBuilder::text_note(content)
            .custom_created_at(Timestamp::from(timestamp))
            .sign_with_keys(&keys)
            .expect("Failed to create test event")
    }

    // Helper function to create a reaction event
    fn create_reaction_event(target_event: &Event, timestamp: u64) -> Event {
        let keys = Keys::generate();
        EventBuilder::reaction(target_event, "+")
            .custom_created_at(Timestamp::from(timestamp))
            .sign_with_keys(&keys)
            .expect("Failed to create reaction event")
    }

    // Helper function to create a repost event
    fn create_repost_event(target_event: &Event, timestamp: u64) -> Event {
        let keys = Keys::generate();
        EventBuilder::repost(target_event, None)
            .custom_created_at(Timestamp::from(timestamp))
            .sign_with_keys(&keys)
            .expect("Failed to create repost event")
    }

    // Helper function to create a zap receipt event
    fn create_zap_receipt_event(target_event_id: EventId, timestamp: u64) -> Event {
        let keys = Keys::generate();
        EventBuilder::new(Kind::ZapReceipt, "")
            .tags(vec![Tag::event(target_event_id)])
            .custom_created_at(Timestamp::from(timestamp))
            .sign_with_keys(&keys)
            .expect("Failed to create zap receipt event")
    }

    #[test]
    fn test_timeline_default() {
        let timeline = Timeline::default();

        assert_eq!(timeline.tabs().len(), 1);
        assert_eq!(timeline.active_tab_index(), 0);
        assert_eq!(timeline.last_tab_index(), 0);
        assert_eq!(timeline.len(), 0);
        assert!(timeline.is_empty());
        assert_eq!(timeline.selected_index(), None);
        assert!(timeline.is_loading());
    }

    #[test]
    fn test_active_tab() {
        let timeline = Timeline::default();
        let active_tab = timeline.active_tab();

        assert_eq!(active_tab.tab_type(), &TimelineTabType::Home);
        assert_eq!(active_tab.len(), 0);
    }

    #[test]
    fn test_find_tab_by_type() {
        let mut timeline = Timeline::default();

        // Home tab should be at index 0
        assert_eq!(timeline.find_tab_by_type(&TimelineTabType::Home), Some(0));

        // Non-existent tab should return None
        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        assert_eq!(
            timeline.find_tab_by_type(&TimelineTabType::UserTimeline { pubkey }),
            None
        );

        // Add a user timeline tab
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        assert_eq!(
            timeline.find_tab_by_type(&TimelineTabType::UserTimeline { pubkey }),
            Some(1)
        );
    }

    #[test]
    fn test_note_added_to_home_tab() {
        let mut timeline = Timeline::default();
        let event = create_test_event(1000, 1, "Hello, Nostr!");
        let event_id = event.id;

        let _ = timeline.update(Message::NoteAddedToTab {
            event,
            tab_type: TimelineTabType::Home,
        });

        assert_eq!(timeline.len(), 1);
        assert!(!timeline.is_empty());
        assert!(timeline.notes.contains_key(&event_id));
        assert_eq!(timeline.oldest_timestamp(), Some(Timestamp::from(1000)));
        assert!(!timeline.is_loading());
    }

    #[test]
    fn test_note_added_to_nonexistent_tab() {
        let mut timeline = Timeline::default();
        let event = create_test_event(1000, 1, "Hello, Nostr!");

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        let _ = timeline.update(Message::NoteAddedToTab {
            event,
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        // Note should not be added
        assert_eq!(timeline.len(), 0);
        assert!(timeline.is_empty());
    }

    #[test]
    fn test_note_added_shared_storage() {
        let mut timeline = Timeline::default();

        // Add a user timeline tab
        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        // Add the same event to both tabs
        let event = create_test_event(1000, 1, "Shared note");
        let event_id = event.id;

        let _ = timeline.update(Message::NoteAddedToTab {
            event: event.clone(),
            tab_type: TimelineTabType::Home,
        });

        let _ = timeline.update(Message::NoteAddedToTab {
            event,
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        // Event should be stored only once in centralized storage
        assert_eq!(timeline.notes.len(), 1);
        assert!(timeline.notes.contains_key(&event_id));

        // Both tabs should reference the event
        assert_eq!(timeline.tabs[0].len(), 1);
        assert_eq!(timeline.tabs[1].len(), 1);
    }

    #[test]
    fn test_multiple_notes_sorting() {
        let mut timeline = Timeline::default();

        // Add notes in non-chronological order
        let event1 = create_test_event(3000, 1, "Newest");
        let event2 = create_test_event(1000, 2, "Oldest");
        let event3 = create_test_event(2000, 3, "Middle");

        let _ = timeline.update(Message::NoteAddedToTab {
            event: event2,
            tab_type: TimelineTabType::Home,
        });

        let _ = timeline.update(Message::NoteAddedToTab {
            event: event1,
            tab_type: TimelineTabType::Home,
        });

        let _ = timeline.update(Message::NoteAddedToTab {
            event: event3,
            tab_type: TimelineTabType::Home,
        });

        assert_eq!(timeline.len(), 3);
        assert_eq!(timeline.oldest_timestamp(), Some(Timestamp::from(1000)));
    }

    #[test]
    fn test_reaction_added() {
        let mut timeline = Timeline::default();

        // Add a text note first
        let text_event = create_test_event(1000, 1, "Original note");
        let text_event_id = text_event.id;

        let _ = timeline.update(Message::NoteAddedToTab {
            event: text_event.clone(),
            tab_type: TimelineTabType::Home,
        });

        // Add a reaction to the text note
        let reaction_event = create_reaction_event(&text_event, 1001);
        let _ = timeline.update(Message::ReactionAdded {
            event: reaction_event,
        });

        // Verify the reaction was added to the note
        let note = timeline
            .notes
            .get(&text_event_id)
            .expect("Note should exist");
        assert_eq!(note.reactions_count(), 1);
    }

    #[test]
    fn test_reaction_added_to_nonexistent_note() {
        let mut timeline = Timeline::default();

        // Add a reaction to a non-existent note
        let nonexistent_event = create_test_event(999, 99, "Nonexistent");
        let reaction_event = create_reaction_event(&nonexistent_event, 1000);

        let _ = timeline.update(Message::ReactionAdded {
            event: reaction_event,
        });

        // Should not cause any issues
        assert_eq!(timeline.notes.len(), 0);
    }

    #[test]
    fn test_repost_added() {
        let mut timeline = Timeline::default();

        // Add a text note first
        let text_event = create_test_event(1000, 1, "Original note");
        let text_event_id = text_event.id;

        let _ = timeline.update(Message::NoteAddedToTab {
            event: text_event.clone(),
            tab_type: TimelineTabType::Home,
        });

        // Add a repost
        let repost_event = create_repost_event(&text_event, 1001);
        let _ = timeline.update(Message::RepostAdded {
            event: repost_event,
        });

        // Verify the repost was added
        let note = timeline
            .notes
            .get(&text_event_id)
            .expect("Note should exist");
        assert_eq!(note.reposts_count(), 1);
    }

    #[test]
    fn test_zap_receipt_added() {
        let mut timeline = Timeline::default();

        // Add a text note first
        let text_event = create_test_event(1000, 1, "Original note");
        let text_event_id = text_event.id;

        let _ = timeline.update(Message::NoteAddedToTab {
            event: text_event,
            tab_type: TimelineTabType::Home,
        });

        // Add a zap receipt
        let zap_event = create_zap_receipt_event(text_event_id, 1001);
        let _ = timeline.update(Message::ZapReceiptAdded { event: zap_event });

        // Verify the zap receipt was added
        let note = timeline
            .notes
            .get(&text_event_id)
            .expect("Note should exist");
        assert_eq!(note.zap_amount(), 0); // Amount is 0 because we didn't add an amount tag
    }

    #[test]
    fn test_item_selection() {
        let mut timeline = Timeline::default();

        // Add some notes
        for i in 0..5 {
            let event = create_test_event(1000 + i, i as u8, &format!("Note {i}"));
            let _ = timeline.update(Message::NoteAddedToTab {
                event,
                tab_type: TimelineTabType::Home,
            });
        }

        // Test selecting an item
        let _ = timeline.update(Message::ItemSelected { index: 2 });
        assert_eq!(timeline.selected_index(), Some(2));

        // Test clearing selection
        let _ = timeline.update(Message::ItemSelectionCleared);
        assert_eq!(timeline.selected_index(), None);
    }

    #[test]
    fn test_navigation_through_items() {
        let mut timeline = Timeline::default();

        // Add some notes
        for i in 0..5 {
            let event = create_test_event(1000 + i, i as u8, &format!("Note {i}"));
            let _ = timeline.update(Message::NoteAddedToTab {
                event,
                tab_type: TimelineTabType::Home,
            });
        }

        // Select first item
        let _ = timeline.update(Message::FirstItemSelected);
        assert_eq!(timeline.selected_index(), Some(0));

        // Move to next item
        let _ = timeline.update(Message::NextItemSelected);
        assert_eq!(timeline.selected_index(), Some(1));

        // Move to previous item
        let _ = timeline.update(Message::PreviousItemSelected);
        assert_eq!(timeline.selected_index(), Some(0));

        // Move to last item
        let _ = timeline.update(Message::LastItemSelected);
        assert_eq!(timeline.selected_index(), Some(4));
    }

    #[test]
    fn test_selected_note() {
        let mut timeline = Timeline::default();

        let event = create_test_event(1000, 1, "Test note");
        let event_id = event.id;

        let _ = timeline.update(Message::NoteAddedToTab {
            event,
            tab_type: TimelineTabType::Home,
        });

        let _ = timeline.update(Message::ItemSelected { index: 0 });

        let selected = timeline.selected_note();
        assert!(selected.is_some());
        assert_eq!(
            selected.expect("Should have selected note").as_event().id,
            event_id
        );
    }

    #[test]
    fn test_note_by_index() {
        let mut timeline = Timeline::default();

        let event = create_test_event(1000, 1, "Test note");
        let event_id = event.id;

        let _ = timeline.update(Message::NoteAddedToTab {
            event,
            tab_type: TimelineTabType::Home,
        });

        let note = timeline.note_by_index(0);
        assert!(note.is_some());
        assert_eq!(note.expect("Should have note").as_event().id, event_id);

        let out_of_bounds = timeline.note_by_index(10);
        assert!(out_of_bounds.is_none());
    }

    #[test]
    fn test_loading_more_started_when_scrolling_to_bottom() {
        let mut timeline = Timeline::default();

        // Add some notes
        for i in 0..3 {
            let event = create_test_event(1000 + i, i as u8, &format!("Note {i}"));
            let _ = timeline.update(Message::NoteAddedToTab {
                event,
                tab_type: TimelineTabType::Home,
            });
        }

        // Select the last item (bottom)
        let _ = timeline.update(Message::LastItemSelected);
        assert!(timeline.is_at_bottom());

        // Try to scroll down - this should trigger loading more
        let _ = timeline.update(Message::NextItemSelected);

        // Verify loading state
        let is_loading = timeline.is_loading_more_for_tab(&TimelineTabType::Home);
        assert_eq!(is_loading, Some(true));
    }

    #[test]
    fn test_loading_more_completes_when_older_event_arrives() {
        let mut timeline = Timeline::default();

        // Add initial note
        let event1 = create_test_event(2000, 1, "Recent note");
        let _ = timeline.update(Message::NoteAddedToTab {
            event: event1,
            tab_type: TimelineTabType::Home,
        });

        // Select the last item and scroll down to trigger loading more
        let _ = timeline.update(Message::LastItemSelected);
        let _ = timeline.update(Message::NextItemSelected);
        assert_eq!(
            timeline.is_loading_more_for_tab(&TimelineTabType::Home),
            Some(true)
        );

        // Add an older note (timestamp < loading_more_since)
        let event2 = create_test_event(1000, 2, "Older note");
        let _ = timeline.update(Message::NoteAddedToTab {
            event: event2,
            tab_type: TimelineTabType::Home,
        });

        // Loading should complete automatically
        assert_eq!(
            timeline.is_loading_more_for_tab(&TimelineTabType::Home),
            Some(false)
        );
    }

    #[test]
    fn test_is_at_bottom() {
        let mut timeline = Timeline::default();

        // Empty timeline is not at bottom
        assert!(!timeline.is_at_bottom());

        // Add some notes
        for i in 0..5 {
            let event = create_test_event(1000 + i, i as u8, &format!("Note {i}"));
            let _ = timeline.update(Message::NoteAddedToTab {
                event,
                tab_type: TimelineTabType::Home,
            });
        }

        // Select first item - not at bottom
        let _ = timeline.update(Message::FirstItemSelected);
        assert!(!timeline.is_at_bottom());

        // Select last item - at bottom
        let _ = timeline.update(Message::LastItemSelected);
        assert!(timeline.is_at_bottom());
    }

    #[test]
    fn test_tab_added() {
        let mut timeline = Timeline::default();
        assert_eq!(timeline.tabs().len(), 1);

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        assert_eq!(timeline.tabs().len(), 2);
        assert_eq!(timeline.active_tab_index(), 1); // Should switch to new tab
        assert_eq!(
            timeline.active_tab().tab_type(),
            &TimelineTabType::UserTimeline { pubkey }
        );
    }

    #[test]
    fn test_tab_added_duplicate() {
        let mut timeline = Timeline::default();

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        // Try to add the same tab again
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        // Should still have only 2 tabs (Home + UserTimeline)
        assert_eq!(timeline.tabs().len(), 2);
    }

    #[test]
    fn test_tab_removed() {
        let mut timeline = Timeline::default();

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        assert_eq!(timeline.tabs().len(), 2);
        assert_eq!(timeline.active_tab_index(), 1);

        // Remove the user timeline tab
        let _ = timeline.update(Message::TabRemoved { index: 1 });

        assert_eq!(timeline.tabs().len(), 1);
        assert_eq!(timeline.active_tab_index(), 0);
    }

    #[test]
    fn test_tab_removed_cannot_remove_home() {
        let mut timeline = Timeline::default();

        // Try to remove the Home tab
        let _ = timeline.update(Message::TabRemoved { index: 0 });

        // Home tab should still exist
        assert_eq!(timeline.tabs().len(), 1);
        assert_eq!(timeline.active_tab().tab_type(), &TimelineTabType::Home);
    }

    #[test]
    fn test_tab_removed_out_of_bounds() {
        let mut timeline = Timeline::default();

        // Try to remove a non-existent tab
        let _ = timeline.update(Message::TabRemoved { index: 10 });

        // Should have no effect
        assert_eq!(timeline.tabs().len(), 1);
    }

    #[test]
    fn test_tab_removed_adjusts_active_index() {
        let mut timeline = Timeline::default();

        let pubkey1 = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        let pubkey2 = PublicKey::from_slice(&[2u8; 32]).expect("Valid pubkey");

        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey: pubkey1 },
        });
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey: pubkey2 },
        });

        // Now we have: [Home, User1, User2], active = 2
        assert_eq!(timeline.tabs().len(), 3);
        assert_eq!(timeline.active_tab_index(), 2);

        // Remove the middle tab
        let _ = timeline.update(Message::TabRemoved { index: 1 });

        // Now we have: [Home, User2], active should be adjusted to 1
        assert_eq!(timeline.tabs().len(), 2);
        assert_eq!(timeline.active_tab_index(), 1);
    }

    #[test]
    fn test_tab_removed_active_tab() {
        let mut timeline = Timeline::default();

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        // Active tab is now the user timeline (index 1)
        assert_eq!(timeline.active_tab_index(), 1);

        // Remove the active tab
        let _ = timeline.update(Message::TabRemoved { index: 1 });

        // Should fall back to index 0 (Home)
        assert_eq!(timeline.tabs().len(), 1);
        assert_eq!(timeline.active_tab_index(), 0);
    }

    #[test]
    fn test_tab_selected() {
        let mut timeline = Timeline::new_loaded();

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        // Switch to Home tab
        let _ = timeline.update(Message::TabSelected { index: 0 });
        assert_eq!(timeline.active_tab_index(), 0);

        // Switch back to user timeline
        let _ = timeline.update(Message::TabSelected { index: 1 });
        assert_eq!(timeline.active_tab_index(), 1);
    }

    #[test]
    fn test_tab_selected_out_of_bounds() {
        let mut timeline = Timeline::default();

        let _ = timeline.update(Message::TabSelected { index: 10 });

        // Should have no effect
        assert_eq!(timeline.active_tab_index(), 0);
    }

    #[test]
    fn test_next_tab_selected() {
        let mut timeline = Timeline::new_loaded();

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        // Start at Home (index 0)
        let _ = timeline.update(Message::TabSelected { index: 0 });
        assert_eq!(timeline.active_tab_index(), 0);

        // Move to next tab
        let _ = timeline.update(Message::NextTabSelected);
        assert_eq!(timeline.active_tab_index(), 1);

        // Try to move beyond last tab
        let _ = timeline.update(Message::NextTabSelected);
        assert_eq!(timeline.active_tab_index(), 1); // Should stay at last tab
    }

    #[test]
    fn test_previous_tab_selected() {
        let mut timeline = Timeline::new_loaded();

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        // Start at user timeline (index 1)
        assert_eq!(timeline.active_tab_index(), 1);

        // Move to previous tab
        let _ = timeline.update(Message::PreviousTabSelected);
        assert_eq!(timeline.active_tab_index(), 0);

        // Try to move before first tab
        let _ = timeline.update(Message::PreviousTabSelected);
        assert_eq!(timeline.active_tab_index(), 0); // Should stay at first tab
    }

    #[test]
    fn test_complex_scenario_multiple_tabs_and_notes() {
        let mut timeline = Timeline::default();

        // Add a user timeline tab
        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        // Add notes to Home tab
        let _ = timeline.update(Message::TabSelected { index: 0 });
        for i in 0..3 {
            let event = create_test_event(1000 + i, i as u8, &format!("Home note {i}"));
            let _ = timeline.update(Message::NoteAddedToTab {
                event,
                tab_type: TimelineTabType::Home,
            });
        }

        // Add notes to user timeline tab
        for i in 3..6 {
            let event = create_test_event(2000 + i, i as u8, &format!("User note {i}"));
            let _ = timeline.update(Message::NoteAddedToTab {
                event,
                tab_type: TimelineTabType::UserTimeline { pubkey },
            });
        }

        // Verify Home tab
        assert_eq!(timeline.tabs[0].len(), 3);

        // Verify user timeline tab
        assert_eq!(timeline.tabs[1].len(), 3);

        // Verify centralized storage
        assert_eq!(timeline.notes.len(), 6);

        // Switch to user timeline and select an item
        let _ = timeline.update(Message::TabSelected { index: 1 });
        let _ = timeline.update(Message::FirstItemSelected);
        assert_eq!(timeline.selected_index(), Some(0));

        // Switch back to Home - selection should be independent
        let _ = timeline.update(Message::TabSelected { index: 0 });
        assert_eq!(timeline.selected_index(), None);
    }

    #[test]
    fn test_find_event_id_from_last_e_tag() {
        let keys = Keys::generate();
        let target_id = EventId::all_zeros();

        let event = EventBuilder::new(Kind::Reaction, "+")
            .tags(vec![Tag::event(target_id)])
            .sign_with_keys(&keys)
            .expect("Failed to create event");

        let found_id = Timeline::find_event_id_from_last_e_tag(&event);
        assert_eq!(found_id, Some(target_id));
    }

    #[test]
    fn test_find_event_id_from_last_e_tag_multiple_tags() {
        let keys = Keys::generate();
        let first_id = EventId::all_zeros();
        let last_id = EventId::from_slice(&[1u8; 32]).expect("Valid event ID");

        let event = EventBuilder::new(Kind::Reaction, "+")
            .tags(vec![Tag::event(first_id), Tag::event(last_id)])
            .sign_with_keys(&keys)
            .expect("Failed to create event");

        // Should return the last 'e' tag
        let found_id = Timeline::find_event_id_from_last_e_tag(&event);
        assert_eq!(found_id, Some(last_id));
    }

    #[test]
    fn test_find_event_id_from_last_e_tag_no_tags() {
        let keys = Keys::generate();

        let event = EventBuilder::new(Kind::Reaction, "+")
            .sign_with_keys(&keys)
            .expect("Failed to create event");

        let found_id = Timeline::find_event_id_from_last_e_tag(&event);
        assert_eq!(found_id, None);
    }

    #[test]
    fn test_last_tab_index() {
        let mut timeline = Timeline::default();
        assert_eq!(timeline.last_tab_index(), 0);

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });

        assert_eq!(timeline.last_tab_index(), 1);
    }

    #[test]
    fn test_is_loading_changes_on_first_note() {
        let mut timeline = Timeline::default();
        assert!(timeline.is_loading());

        let event = create_test_event(1000, 1, "First note");
        let _ = timeline.update(Message::NoteAddedToTab {
            event,
            tab_type: TimelineTabType::Home,
        });

        assert!(!timeline.is_loading());
    }

    #[test]
    fn test_is_loading_remains_false_after_first_note() {
        let mut timeline = Timeline::default();

        // Add first note
        let event1 = create_test_event(1000, 1, "First note");
        let _ = timeline.update(Message::NoteAddedToTab {
            event: event1,
            tab_type: TimelineTabType::Home,
        });

        assert!(!timeline.is_loading());

        // Add second note
        let event2 = create_test_event(2000, 2, "Second note");
        let _ = timeline.update(Message::NoteAddedToTab {
            event: event2,
            tab_type: TimelineTabType::Home,
        });

        assert!(!timeline.is_loading());
    }

    #[test]
    fn test_user_operations_blocked_when_loading() {
        let mut timeline = Timeline::default();
        // Timeline starts in loading state
        assert!(timeline.is_loading());

        // Try to select an item - should be ignored
        let _ = timeline.update(Message::ItemSelected { index: 0 });
        assert_eq!(timeline.selected_index(), None);

        // Try to navigate - should be ignored
        let _ = timeline.update(Message::PreviousItemSelected);
        assert_eq!(timeline.selected_index(), None);

        let _ = timeline.update(Message::NextItemSelected);
        assert_eq!(timeline.selected_index(), None);

        // Try to select first/last - should be ignored
        let _ = timeline.update(Message::FirstItemSelected);
        assert_eq!(timeline.selected_index(), None);

        let _ = timeline.update(Message::LastItemSelected);
        assert_eq!(timeline.selected_index(), None);
    }

    #[test]
    fn test_user_operations_allowed_after_loading() {
        let mut timeline = Timeline::new_loaded();

        // Add a note so we have something to select
        let event = create_test_event(1000, 1, "Test note");
        let _ = timeline.update(Message::NoteAddedToTab {
            event,
            tab_type: TimelineTabType::Home,
        });

        assert!(!timeline.is_loading());

        // Now user operations should work
        let _ = timeline.update(Message::ItemSelected { index: 0 });
        assert_eq!(timeline.selected_index(), Some(0));
    }

    #[test]
    fn test_tab_selection_blocked_when_loading() {
        let mut timeline = Timeline::default();
        assert!(timeline.is_loading());

        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");

        // Add tab should work during loading (it's data management)
        let _ = timeline.update(Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        });
        assert_eq!(timeline.tabs().len(), 2); // Home + new tab

        // But tab selection should be blocked
        let original_index = timeline.active_tab_index();
        let _ = timeline.update(Message::TabSelected { index: 0 });
        assert_eq!(timeline.active_tab_index(), original_index);

        // Tab navigation should also be blocked
        let _ = timeline.update(Message::NextTabSelected);
        assert_eq!(timeline.active_tab_index(), original_index);

        let _ = timeline.update(Message::PreviousTabSelected);
        assert_eq!(timeline.active_tab_index(), original_index);
    }

    #[test]
    fn test_data_updates_allowed_when_loading() {
        let mut timeline = Timeline::default();
        assert!(timeline.is_loading());

        // Data updates should work even during loading
        let event = create_test_event(1000, 1, "Test note");
        let event_id = event.id;

        let _ = timeline.update(Message::NoteAddedToTab {
            event,
            tab_type: TimelineTabType::Home,
        });

        // Note should be added
        assert_eq!(timeline.len(), 1);
        assert!(timeline.notes.contains_key(&event_id));

        // And loading should now be complete
        assert!(!timeline.is_loading());
    }

    #[test]
    fn test_reactions_allowed_when_loading() {
        let mut timeline = Timeline::default();

        // Add a note first (stops loading)
        let text_event = create_test_event(1000, 1, "Original note");
        let text_event_id = text_event.id;
        let _ = timeline.update(Message::NoteAddedToTab {
            event: text_event.clone(),
            tab_type: TimelineTabType::Home,
        });

        // Manually set back to loading state for testing
        timeline.is_loading = true;

        // Reaction should still work during loading
        let reaction_event = create_reaction_event(&text_event, 1001);
        let _ = timeline.update(Message::ReactionAdded {
            event: reaction_event,
        });

        let note = timeline
            .notes
            .get(&text_event_id)
            .expect("Note should exist");
        assert_eq!(note.reactions_count(), 1);
    }

    #[test]
    fn test_message_is_user_operation() {
        // User operations (should return true)
        assert!(Message::PreviousItemSelected.is_user_operation());
        assert!(Message::NextItemSelected.is_user_operation());
        assert!(Message::FirstItemSelected.is_user_operation());
        assert!(Message::LastItemSelected.is_user_operation());
        assert!(Message::ItemSelected { index: 0 }.is_user_operation());
        assert!(Message::ItemSelectionCleared.is_user_operation());
        assert!(Message::TabSelected { index: 0 }.is_user_operation());
        assert!(Message::NextTabSelected.is_user_operation());
        assert!(Message::PreviousTabSelected.is_user_operation());

        // Data operations (should return false)
        let event = create_test_event(1000, 1, "Test");
        assert!(!Message::NoteAddedToTab {
            event: event.clone(),
            tab_type: TimelineTabType::Home,
        }
        .is_user_operation());
        assert!(!Message::ReactionAdded {
            event: event.clone(),
        }
        .is_user_operation());
        assert!(!Message::RepostAdded {
            event: event.clone(),
        }
        .is_user_operation());
        assert!(!Message::ZapReceiptAdded { event }.is_user_operation());

        // Tab management (should return false)
        let pubkey = PublicKey::from_slice(&[1u8; 32]).expect("Valid pubkey");
        assert!(!Message::TabAdded {
            tab_type: TimelineTabType::UserTimeline { pubkey },
        }
        .is_user_operation());
        assert!(!Message::TabRemoved { index: 0 }.is_user_operation());
    }
}
