use nostr_sdk::prelude::*;
use sorted_vec::ReverseSortedSet;
use std::{cmp::Reverse, collections::HashMap};

use crate::domain::nostr::SortableEventId;
use crate::model::timeline as model_timeline;
use crate::model::timeline::text_note::{Message as TextNoteMessage, TextNote};

// Re-export for backward compatibility
pub use crate::model::timeline::TimelineTabType;

/// Represents a single timeline tab with its own state
///
/// This is a wrapper around the new Elm-style model to maintain backward compatibility
/// with the existing TimelineState API. Internally delegates to src/model/timeline.
#[derive(Debug, Clone)]
pub struct TimelineTab {
    pub tab_type: model_timeline::TimelineTabType,
    /// The new Elm-style model
    inner: model_timeline::TimelineTab,
    /// Sorted list of event IDs (newest first) - kept for public access
    /// The actual event data is stored in TimelineState::events
    pub notes: ReverseSortedSet<SortableEventId>,
}

impl TimelineTab {
    /// Create a new timeline tab with the specified type
    pub fn new(tab_type: TimelineTabType) -> Self {
        Self {
            tab_type: tab_type.clone(),
            inner: model_timeline::TimelineTab::new(tab_type),
            notes: ReverseSortedSet::new(),
        }
    }

    /// Create a new Home timeline tab
    pub fn new_home() -> Self {
        Self::new(TimelineTabType::Home)
    }

    // Delegate to inner model
    pub fn selected_index(&self) -> Option<usize> {
        self.inner.selected_index()
    }

    pub fn scroll_up(&mut self) {
        self.inner
            .update(model_timeline::tab::Message::PreviousItemSelected);
    }

    pub fn scroll_down(&mut self, _max_index: usize) {
        self.inner
            .update(model_timeline::tab::Message::NextItemSelected);
    }

    pub fn select_first(&mut self) {
        self.inner
            .update(model_timeline::tab::Message::FirstItemSelected);
    }

    pub fn select_last(&mut self, _max_index: usize) {
        self.inner
            .update(model_timeline::tab::Message::LastItemSelected);
    }

    pub fn deselect(&mut self) {
        self.inner
            .update(model_timeline::tab::Message::SelectionCleared);
    }

    pub fn oldest_timestamp(&self) -> Option<Timestamp> {
        self.inner.oldest_timestamp()
    }

    pub fn is_loading_more(&self) -> bool {
        self.inner.is_loading_more()
    }

    pub fn start_loading_more(&mut self) {
        self.inner
            .update(model_timeline::tab::Message::LoadingMoreStarted);
    }

    pub fn finish_loading_more(&mut self) {
        self.inner
            .update(model_timeline::tab::Message::LoadingMoreFinished);
    }

    // Note management
    pub fn add_note(&mut self, sortable_id: SortableEventId) {
        self.inner
            .update(model_timeline::tab::Message::NoteAdded(sortable_id));
        // Keep the public notes field in sync
        let _ = self.notes.find_or_insert(Reverse(sortable_id));
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Timeline-related state
#[derive(Debug, Clone)]
pub struct TimelineState {
    // Tab management
    tabs: Vec<TimelineTab>,
    active_tab_index: usize,

    // Centralized event storage (shared across all tabs)
    // Each event is stored once here and referenced by EventId from tabs
    notes: HashMap<EventId, TextNote>,
}

impl Default for TimelineState {
    fn default() -> Self {
        Self {
            tabs: vec![TimelineTab::new_home()],
            active_tab_index: 0,
            notes: HashMap::new(),
        }
    }
}

impl TimelineState {
    /// Get the active tab
    ///
    /// # Panics
    /// Panics if active_tab_index is out of bounds (this indicates a bug in the implementation)
    fn active_tab(&self) -> &TimelineTab {
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

    /// Get the length of the active timeline
    pub fn len(&self) -> usize {
        self.active_tab().len()
    }

    /// Check if the active timeline is empty
    pub fn is_empty(&self) -> bool {
        self.active_tab().is_empty()
    }

    /// Get the index of currently selected note in the active tab
    pub fn selected_index(&self) -> Option<usize> {
        self.active_tab().selected_index()
    }

    pub fn note_by_index(&self, index: usize) -> Option<&TextNote> {
        let event_id = self.active_tab().notes.get(index)?.0.id;
        self.notes.get(&event_id)
    }

    /// Add a text note to a specific timeline tab
    ///
    /// Returns a tuple of (was_inserted, loading_completed)
    /// - was_inserted: `true` if the event was newly inserted, `false` if it already existed
    /// - loading_completed: `true` if this event completed a LoadMore operation
    ///
    /// Automatically adjusts the selected index if a new item is inserted before it (only for active tab)
    pub fn add_note_to_tab(&mut self, event: Event, tab_type: &TimelineTabType) -> (bool, bool) {
        // Find the tab index for the specified tab type
        let tab_index = match self.find_tab_by_type(tab_type) {
            Some(index) => index,
            None => {
                // Tab not found - cannot add note
                log::warn!("Cannot add note: tab {tab_type:?} not found");
                return (false, false);
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
        let insert_result = tab.notes.find_or_insert(Reverse(sortable_id));

        // For inactive tabs, we need to preserve the selection state
        // because NoteAdded will adjust it, but we only want adjustment for active tabs
        let is_active = self.active_tab_index == tab_index;
        let original_selection = if !is_active {
            tab.selected_index()
        } else {
            None
        };

        // Update inner model (this will adjust selection for active tab)
        tab.inner
            .update(model_timeline::tab::Message::NoteAdded(sortable_id));

        // Restore original selection for inactive tabs
        if !is_active {
            if let Some(index) = original_selection {
                tab.inner
                    .update(model_timeline::tab::Message::ItemSelected(index));
            } else {
                tab.inner
                    .update(model_timeline::tab::Message::SelectionCleared);
            }
        }

        // Check if this event completes a LoadMore operation
        let loading_completed = if let Some(loading_since) = tab.inner.loading_more_since() {
            if created_at < loading_since {
                // An older event arrived - loading completed
                tab.finish_loading_more();
                true
            } else {
                false
            }
        } else {
            false
        };

        // Update oldest timestamp - handled by add_note which calls inner.update(NoteAdded)
        // (no need to call separately as add_note already updates pagination)

        // Adjust selected index if a new item was inserted before it
        // This prevents the selection from shifting when new events arrive
        // NOTE: Only adjust if this tab is currently active
        // The inner model handles this automatically via NoteAdded message
        if let sorted_vec::FindOrInsert::Inserted(_inserted_at) = insert_result {
            (true, loading_completed)
        } else {
            (false, loading_completed)
        }
    }

    /// Add a reaction event to the timeline (shared across all tabs)
    /// Returns the ID of the event being reacted to, or `None` if no valid target event is found
    pub fn add_reaction(&mut self, event: Event) -> Option<EventId> {
        // Extract the last event ID from 'e' tags
        let target_event_id = event
            .tags
            .filter_standardized(TagKind::SingleLetter(SingleLetterTag::lowercase(
                Alphabet::E,
            )))
            .last()
            .and_then(|tag| match tag {
                TagStandard::Event { event_id, .. } => Some(*event_id),
                _ => None,
            })?;

        self.notes.entry(target_event_id).and_modify(|note| {
            note.update(TextNoteMessage::ReactionReceived(event));
        });

        Some(target_event_id)
    }

    /// Add a repost event to the timeline (shared across all tabs)
    /// Returns the ID of the event being reposted, or `None` if no valid target event is found
    pub fn add_repost(&mut self, event: Event) -> Option<EventId> {
        // Extract the last event ID from 'e' tags
        let target_event_id = event
            .tags
            .filter_standardized(TagKind::SingleLetter(SingleLetterTag::lowercase(
                Alphabet::E,
            )))
            .last()
            .and_then(|tag| match tag {
                TagStandard::Event { event_id, .. } => Some(*event_id),
                _ => None,
            })?;

        self.notes.entry(target_event_id).and_modify(|note| {
            note.update(TextNoteMessage::RepostReceived(event));
        });

        Some(target_event_id)
    }

    /// Add a zap receipt event to the timeline (shared across all tabs)
    /// Returns the ID of the event being zapped, or `None` if no valid target event is found
    pub fn add_zap_receipt(&mut self, event: Event) -> Option<EventId> {
        // Extract the last event ID from 'e' tags
        let target_event_id = event
            .tags
            .filter_standardized(TagKind::SingleLetter(SingleLetterTag::lowercase(
                Alphabet::E,
            )))
            .last()
            .and_then(|tag| match tag {
                TagStandard::Event { event_id, .. } => Some(*event_id),
                _ => None,
            })?;

        self.notes.entry(target_event_id).and_modify(|note| {
            note.update(TextNoteMessage::ZapReceiptReceived(event));
        });

        Some(target_event_id)
    }

    /// Move selection up by one item in the active tab
    /// If no item is selected, selects the first item
    pub fn scroll_up(&mut self) {
        let tab = self.active_tab_mut();

        if let Some(current) = tab.selected_index() {
            if current > 0 {
                tab.inner
                    .update(model_timeline::tab::Message::ItemSelected(current - 1));
            }
        } else if !tab.is_empty() {
            tab.select_first();
        }
    }

    /// Move selection down by one item in the active tab
    /// If no item is selected, selects the first item
    pub fn scroll_down(&mut self) {
        let tab = self.active_tab_mut();

        tab.scroll_down(0); // max_index is now calculated internally
        if tab.selected_index().is_none() && !tab.is_empty() {
            tab.select_first();
        }
    }

    /// Get the currently selected note from the active tab
    pub fn selected_note(&self) -> Option<&TextNote> {
        // Get the SortableEventId from the selected index, then look up in the HashMap
        let index = self.selected_index()?;
        let sortable_id = self.active_tab().notes.get(index)?;
        let event_id = sortable_id.0.id;
        self.notes.get(&event_id)
    }

    /// Select a note at the specified index in the active tab
    /// If the index is out of bounds, deselects the current selection
    pub fn select(&mut self, index: usize) {
        let tab = self.active_tab_mut();

        if index < tab.len() {
            tab.inner
                .update(model_timeline::tab::Message::ItemSelected(index));
        } else {
            tab.deselect();
        }
    }

    /// Select the first note in the active timeline
    pub fn select_first(&mut self) {
        let tab = self.active_tab_mut();

        if !tab.is_empty() {
            tab.select_first();
        }
    }

    /// Select the last note in the active timeline
    pub fn select_last(&mut self) {
        let tab = self.active_tab_mut();

        if !tab.is_empty() {
            let max_index = tab.len().saturating_sub(1);
            tab.select_last(max_index);
        }
    }

    /// Clear the current selection in the active tab
    pub fn deselect(&mut self) {
        self.active_tab_mut().deselect();
    }

    /// Get the oldest timestamp in the active timeline (for pagination)
    pub fn oldest_timestamp(&self) -> Option<Timestamp> {
        self.active_tab().oldest_timestamp()
    }

    /// Check if the user has scrolled to the bottom of the active timeline
    pub fn is_at_bottom(&self) -> bool {
        let tab = self.active_tab();

        if tab.is_empty() {
            return false;
        }
        let max_index = tab.len().saturating_sub(1);
        tab.selected_index() == Some(max_index)
    }

    /// Mark that a LoadMore operation has started in the active tab
    pub fn start_loading_more(&mut self) {
        self.active_tab_mut().start_loading_more();
    }

    /// Check if currently loading more events in the active tab
    pub fn is_loading_more(&self) -> bool {
        self.active_tab().is_loading_more()
    }

    // ===== Tab Management Methods =====

    /// Get the number of tabs
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Get all tabs
    pub fn tabs(&self) -> &[TimelineTab] {
        &self.tabs
    }

    /// Get the active tab index
    pub fn active_tab_index(&self) -> usize {
        self.active_tab_index
    }

    /// Add a new tab with the specified type
    /// Returns the index of the newly added tab, or an error if the tab already exists
    pub fn add_tab(&mut self, tab_type: TimelineTabType) -> Result<usize, String> {
        // Check if a tab with the same type already exists
        if self.find_tab_by_type(&tab_type).is_some() {
            return Err("Tab with this type already exists".to_string());
        }

        // Create and add the new tab
        let new_tab = TimelineTab::new(tab_type);
        self.tabs.push(new_tab);
        let new_index = self.tabs.len() - 1;

        Ok(new_index)
    }

    /// Remove a tab at the specified index
    /// Returns an error if trying to remove the Home tab or if the index is out of bounds
    pub fn remove_tab(&mut self, index: usize) -> Result<(), String> {
        // Validate index
        if index >= self.tabs.len() {
            return Err("Tab index out of bounds".to_string());
        }

        // Cannot remove the Home tab
        if matches!(self.tabs[index].tab_type, TimelineTabType::Home) {
            return Err("Cannot remove the Home tab".to_string());
        }

        // Remove the tab
        self.tabs.remove(index);

        // Adjust active_tab_index if necessary
        if self.active_tab_index >= self.tabs.len() {
            // If we removed the last tab and it was active, move to the previous tab
            self.active_tab_index = self.tabs.len().saturating_sub(1);
        } else if index < self.active_tab_index {
            // If we removed a tab before the active one, adjust the index
            self.active_tab_index -= 1;
        } else if index == self.active_tab_index {
            // If we removed the active tab, stay at the same index (which now points to the next tab)
            // or move to the last tab if we removed the last one
            if self.active_tab_index >= self.tabs.len() {
                self.active_tab_index = self.tabs.len().saturating_sub(1);
            }
        }

        Ok(())
    }

    /// Find a tab by its type
    /// Returns the index of the tab if found, or None if not found
    pub fn find_tab_by_type(&self, tab_type: &TimelineTabType) -> Option<usize> {
        self.tabs.iter().position(|tab| &tab.tab_type == tab_type)
    }

    /// Select a specific tab by index
    /// If the index is out of bounds, this method does nothing
    pub fn select_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab_index = index;
        }
    }

    /// Switch to the next tab (stops at the last tab, does not wrap around)
    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() && self.active_tab_index < self.tabs.len() - 1 {
            self.active_tab_index += 1;
        }
    }

    /// Switch to the previous tab (stops at the first tab, does not wrap around)
    pub fn prev_tab(&mut self) {
        if self.active_tab_index > 0 {
            self.active_tab_index -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper function to create a test event with a specific timestamp
    fn create_test_event(timestamp: u64) -> Event {
        let keys = Keys::generate();
        EventBuilder::text_note(format!("test note {timestamp}"))
            .custom_created_at(Timestamp::from(timestamp))
            .sign_with_keys(&keys)
            .expect("Failed to sign event")
    }

    /// Helper function to insert a test event into the timeline
    fn insert_test_event(state: &mut TimelineState, timestamp: u64) {
        let event = create_test_event(timestamp);
        let event_id = event.id;
        let created_at = event.created_at;

        // Store in centralized storage
        state
            .notes
            .entry(event_id)
            .or_insert_with(|| TextNote::new(event.clone()));

        // Create SortableEventId and insert into tab using add_note which updates inner
        let sortable_id = SortableEventId::new(event_id, created_at);
        let tab = state.active_tab_mut();
        tab.add_note(sortable_id);
    }

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
        state.select(0);
        assert_eq!(state.selected_note(), None);
    }

    #[test]
    fn test_select() {
        let mut state = TimelineState::default();

        // Add some notes
        insert_test_event(&mut state, 1000);
        insert_test_event(&mut state, 2000);
        insert_test_event(&mut state, 3000);

        // Select a valid index
        state.select(1);
        assert_eq!(state.selected_index(), Some(1));

        // Select another valid index
        state.select(2);
        assert_eq!(state.selected_index(), Some(2));

        // Select an invalid index should deselect
        state.select(10);
        assert_eq!(state.selected_index(), None);
    }

    #[test]
    fn test_select_first() {
        let mut state = TimelineState::default();

        // select_first on empty timeline should do nothing
        state.select_first();
        assert_eq!(state.selected_index(), None);

        // Add notes and select first
        insert_test_event(&mut state, 1000);
        insert_test_event(&mut state, 2000);
        state.select_first();
        assert_eq!(state.selected_index(), Some(0));
    }

    #[test]
    fn test_select_last() {
        let mut state = TimelineState::default();

        // select_last on empty timeline should do nothing
        state.select_last();
        assert_eq!(state.selected_index(), None);

        // Add notes and select last
        insert_test_event(&mut state, 1000);
        insert_test_event(&mut state, 2000);
        insert_test_event(&mut state, 3000);
        state.select_last();
        assert_eq!(state.selected_index(), Some(2));
    }

    #[test]
    fn test_deselect() {
        let mut state = TimelineState::default();
        insert_test_event(&mut state, 1000);
        state.select(0);
        assert_eq!(state.selected_index(), Some(0));

        state.deselect();
        assert_eq!(state.selected_index(), None);
    }

    #[test]
    fn test_scroll_up() {
        let mut state = TimelineState::default();

        // scroll_up on empty timeline should do nothing
        state.scroll_up();
        assert_eq!(state.selected_index(), None);

        // Add notes
        insert_test_event(&mut state, 1000);
        insert_test_event(&mut state, 2000);
        insert_test_event(&mut state, 3000);

        // scroll_up with no selection should select first
        state.scroll_up();
        assert_eq!(state.selected_index(), Some(0));

        // scroll_up at the top should stay at the top
        state.scroll_up();
        assert_eq!(state.selected_index(), Some(0));

        // Move to middle and scroll up
        state.select(2);
        state.scroll_up();
        assert_eq!(state.selected_index(), Some(1));
    }

    #[test]
    fn test_scroll_down() {
        let mut state = TimelineState::default();

        // scroll_down on empty timeline should do nothing
        state.scroll_down();
        assert_eq!(state.selected_index(), None);

        // Add notes
        insert_test_event(&mut state, 1000);
        insert_test_event(&mut state, 2000);
        insert_test_event(&mut state, 3000);

        // scroll_down with no selection should select first
        state.scroll_down();
        assert_eq!(state.selected_index(), Some(0));

        // scroll_down should move down
        state.scroll_down();
        assert_eq!(state.selected_index(), Some(1));

        state.scroll_down();
        assert_eq!(state.selected_index(), Some(2));

        // scroll_down at the bottom should stay at the bottom
        state.scroll_down();
        assert_eq!(state.selected_index(), Some(2));
    }

    #[test]
    fn test_scroll_navigation_sequence() {
        let mut state = TimelineState::default();

        // Add notes
        insert_test_event(&mut state, 1000);
        insert_test_event(&mut state, 2000);
        insert_test_event(&mut state, 3000);

        // Start with no selection
        assert_eq!(state.selected_index(), None);

        // First scroll down selects first item
        state.scroll_down();
        assert_eq!(state.selected_index(), Some(0));

        // Continue scrolling down
        state.scroll_down();
        assert_eq!(state.selected_index(), Some(1));

        // Scroll up
        state.scroll_up();
        assert_eq!(state.selected_index(), Some(0));

        // Scroll up at top stays at top
        state.scroll_up();
        assert_eq!(state.selected_index(), Some(0));
    }

    #[test]
    fn test_selected_note_with_data() {
        let mut state = TimelineState::default();

        // Add notes with known content
        let event1 = create_test_event(1000);
        let event2 = create_test_event(2000);
        let event1_id = event1.id;
        let event2_id = event2.id;

        // Store in centralized storage
        state.notes.insert(event1_id, TextNote::new(event1.clone()));
        state.notes.insert(event2_id, TextNote::new(event2.clone()));

        // Create SortableEventIds and insert into tab using add_note
        let sortable1 = SortableEventId::new(event1_id, event1.created_at);
        let sortable2 = SortableEventId::new(event2_id, event2.created_at);

        let tab = state.active_tab_mut();
        tab.add_note(sortable1);
        tab.add_note(sortable2);

        // Select first note
        state.select(0);
        let selected = state.selected_note().expect("should exist");
        // ReverseSortedSet sorts in reverse order, so index 0 is the newest (2000)
        assert_eq!(selected.id(), event2_id);

        // Select second note
        state.select(1);
        let selected = state.selected_note().expect("should exist");
        assert_eq!(selected.id(), event1_id);

        // Deselect
        state.deselect();
        assert!(state.selected_note().is_none());
    }

    #[test]
    fn test_add_reaction_with_valid_target() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();

        // Create a target note
        let target_note = EventBuilder::text_note("target note")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;
        let target_id = target_note.id;

        state.add_note_to_tab(target_note.clone(), &TimelineTabType::Home);

        // Create a reaction to the target note
        let reaction = EventBuilder::reaction(&target_note, "👍").sign_with_keys(&keys)?;

        // Add the reaction
        let result = state.add_reaction(reaction);

        // Should return the target event ID
        assert_eq!(result, Some(target_id));

        // The reaction should be stored in the reactions map
        let note = state.notes.get(&target_id).expect("target note must exist");
        assert_eq!(note.reactions_count(), 1);

        Ok(())
    }

    #[test]
    fn test_add_multiple_reactions_to_same_event() -> Result<()> {
        let mut state = TimelineState::default();
        let keys1 = Keys::generate();
        let keys2 = Keys::generate();

        // Create a target note
        let target_note = EventBuilder::text_note("popular note").sign_with_keys(&keys1)?;
        let target_id = target_note.id;

        state.add_note_to_tab(target_note.clone(), &TimelineTabType::Home);

        // Create multiple reactions from different users
        let reaction1 = EventBuilder::reaction(&target_note, "👍").sign_with_keys(&keys1)?;
        let reaction2 = EventBuilder::reaction(&target_note, "🔥").sign_with_keys(&keys2)?;

        state.add_reaction(reaction1);
        state.add_reaction(reaction2);

        // Both reactions should be stored
        let note = state.notes.get(&target_id).expect("target note must exist");
        assert_eq!(note.reactions_count(), 2);

        Ok(())
    }

    #[test]
    fn test_add_repost_with_valid_target() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();

        // Create a target note
        let target_note = EventBuilder::text_note("target note")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;
        let target_id = target_note.id;

        state.add_note_to_tab(target_note.clone(), &TimelineTabType::Home);

        // Create a repost of the target note
        let repost = EventBuilder::repost(&target_note, None).sign_with_keys(&keys)?;

        // Add the repost
        let result = state.add_repost(repost);

        // Should return the target event ID
        assert_eq!(result, Some(target_id));

        // The repost should be stored in the reposts map
        let note = state.notes.get(&target_id).expect("target note must exist");
        assert_eq!(note.reposts_count(), 1);

        Ok(())
    }

    #[test]
    fn test_add_zap_receipt_with_valid_target() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();

        // Create a target note
        let target_note = EventBuilder::text_note("zappable note")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;
        let target_id = target_note.id;

        state.add_note_to_tab(target_note, &TimelineTabType::Home);

        // Create a zap receipt (Kind 9735) with an 'e' tag pointing to the target
        let zap_receipt = EventBuilder::new(Kind::from(9735), "zap receipt")
            .tag(Tag::event(target_id))
            .sign_with_keys(&keys)?;

        // Add the zap receipt
        let result = state.add_zap_receipt(zap_receipt);

        // Should return the target event ID
        assert_eq!(result, Some(target_id));

        // The zap receipt should be stored in the zap_receipts map
        let note = state.notes.get(&target_id).expect("target note must exist");
        assert_eq!(note.zap_amount(), 0);

        Ok(())
    }

    #[test]
    fn test_reactions_reposts_zaps_independence() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();

        // Create a target note
        let target_note = EventBuilder::text_note("popular note").sign_with_keys(&keys)?;
        let target_id = target_note.id;

        state.add_note_to_tab(target_note.clone(), &TimelineTabType::Home);

        // Add a reaction
        let reaction = EventBuilder::reaction(&target_note, "👍").sign_with_keys(&keys)?;
        state.add_reaction(reaction);

        // Add a repost
        let repost = EventBuilder::repost(&target_note, None).sign_with_keys(&keys)?;
        state.add_repost(repost);

        // Add a zap receipt
        let zap = EventBuilder::new(Kind::from(9735), "zap")
            .tag(Tag::event(target_id))
            .sign_with_keys(&keys)?;
        state.add_zap_receipt(zap);

        // All three should be stored independently
        let note = state.notes.get(&target_id).expect("target note must exist");
        assert_eq!(note.reactions_count(), 1);
        assert_eq!(note.reposts_count(), 1);
        assert_eq!(note.zap_amount(), 0);

        Ok(())
    }

    #[test]
    fn test_oldest_timestamp_tracking() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();

        // Initially no oldest timestamp
        assert_eq!(state.oldest_timestamp(), None);

        // Add first note
        let event1 = EventBuilder::text_note("note 1")
            .custom_created_at(Timestamp::from(2000))
            .sign_with_keys(&keys)?;
        state.add_note_to_tab(event1, &TimelineTabType::Home);
        assert_eq!(state.oldest_timestamp(), Some(Timestamp::from(2000)));

        // Add older note
        let event2 = EventBuilder::text_note("note 2")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;
        state.add_note_to_tab(event2, &TimelineTabType::Home);
        assert_eq!(state.oldest_timestamp(), Some(Timestamp::from(1000)));

        // Add newer note (should not change oldest)
        let event3 = EventBuilder::text_note("note 3")
            .custom_created_at(Timestamp::from(3000))
            .sign_with_keys(&keys)?;
        state.add_note_to_tab(event3, &TimelineTabType::Home);
        assert_eq!(state.oldest_timestamp(), Some(Timestamp::from(1000)));

        Ok(())
    }

    #[test]
    fn test_is_at_bottom() -> Result<()> {
        let mut state = TimelineState::default();

        // Empty timeline - not at bottom
        assert!(!state.is_at_bottom());

        let keys = Keys::generate();

        // Add notes
        let event1 = EventBuilder::text_note("note 1")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;
        state.add_note_to_tab(event1, &TimelineTabType::Home);

        let event2 = EventBuilder::text_note("note 2")
            .custom_created_at(Timestamp::from(2000))
            .sign_with_keys(&keys)?;
        state.add_note_to_tab(event2, &TimelineTabType::Home);

        let event3 = EventBuilder::text_note("note 3")
            .custom_created_at(Timestamp::from(3000))
            .sign_with_keys(&keys)?;
        state.add_note_to_tab(event3, &TimelineTabType::Home);

        // No selection - not at bottom
        assert!(!state.is_at_bottom());

        // Select first - not at bottom
        state.select_first();
        assert!(!state.is_at_bottom());

        // Select last - at bottom
        state.select_last();
        assert!(state.is_at_bottom());

        // Select middle - not at bottom
        state.select(1);
        assert!(!state.is_at_bottom());

        Ok(())
    }

    #[test]
    fn test_scroll_down_at_bottom() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();

        // Add notes
        for i in 1..=3 {
            let event = EventBuilder::text_note(format!("note {i}"))
                .custom_created_at(Timestamp::from(i * 1000))
                .sign_with_keys(&keys)?;
            state.add_note_to_tab(event, &TimelineTabType::Home);
        }

        // Select last
        state.select_last();
        assert!(state.is_at_bottom());

        // Try to scroll down (should stay at bottom)
        state.scroll_down();
        assert!(state.is_at_bottom());

        Ok(())
    }

    #[test]
    fn test_loading_more_tracking() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();

        // Add initial notes
        let event1 = EventBuilder::text_note("note 1")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;
        state.add_note_to_tab(event1, &TimelineTabType::Home);

        let event2 = EventBuilder::text_note("note 2")
            .custom_created_at(Timestamp::from(2000))
            .sign_with_keys(&keys)?;
        state.add_note_to_tab(event2, &TimelineTabType::Home);

        // Not loading initially
        assert!(!state.is_loading_more());

        // Start loading more
        state.start_loading_more();
        assert!(state.is_loading_more());
        // Check that loading started by verifying is_loading_more returns true
        // (we can't access pagination directly anymore)

        // Add a newer event (should not complete loading)
        let event3 = EventBuilder::text_note("note 3")
            .custom_created_at(Timestamp::from(3000))
            .sign_with_keys(&keys)?;
        let (_, loading_completed) = state.add_note_to_tab(event3, &TimelineTabType::Home);
        assert!(!loading_completed);
        assert!(state.is_loading_more());

        // Add an older event (should complete loading)
        let event0 = EventBuilder::text_note("note 0")
            .custom_created_at(Timestamp::from(500))
            .sign_with_keys(&keys)?;
        let (was_inserted, loading_completed) =
            state.add_note_to_tab(event0, &TimelineTabType::Home);
        assert!(was_inserted);
        assert!(loading_completed);
        assert!(!state.is_loading_more());

        Ok(())
    }

    #[test]
    fn test_loading_completion_with_exact_boundary() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();

        // Add initial note
        let event1 = EventBuilder::text_note("note 1")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;
        state.add_note_to_tab(event1, &TimelineTabType::Home);

        // Start loading more (loading_more_since = 1000)
        state.start_loading_more();

        // Add event with same timestamp (should NOT complete loading)
        let event_same = EventBuilder::text_note("note same")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;
        let (_, loading_completed) = state.add_note_to_tab(event_same, &TimelineTabType::Home);
        assert!(!loading_completed);
        assert!(state.is_loading_more());

        // Add event with older timestamp (should complete loading)
        let event_older = EventBuilder::text_note("note older")
            .custom_created_at(Timestamp::from(999))
            .sign_with_keys(&keys)?;
        let (_, loading_completed) = state.add_note_to_tab(event_older, &TimelineTabType::Home);
        assert!(loading_completed);
        assert!(!state.is_loading_more());

        Ok(())
    }

    #[test]
    fn test_loading_more_without_events() -> Result<()> {
        let mut state = TimelineState::default();

        // Empty timeline
        assert!(!state.is_loading_more());

        // Try to start loading more (should set to None since oldest_timestamp is None)
        state.start_loading_more();
        assert!(!state.is_loading_more());

        Ok(())
    }

    #[test]
    fn test_add_tab_success() {
        let mut state = TimelineState::default();

        // Initial state: only Home tab
        assert_eq!(state.tab_count(), 1);
        assert_eq!(state.active_tab_index(), 0);

        // Add a new UserTimeline tab
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let tab_type = TimelineTabType::UserTimeline { pubkey };

        let result = state.add_tab(tab_type.clone());
        assert_eq!(result, Ok(1)); // New tab should be at index 1

        // Verify tab was added
        assert_eq!(state.tab_count(), 2);
        assert_eq!(state.tabs()[1].tab_type, tab_type);
    }

    #[test]
    fn test_add_tab_duplicate_rejection() {
        let mut state = TimelineState::default();

        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let tab_type = TimelineTabType::UserTimeline { pubkey };

        // Add the tab first time - should succeed
        let result1 = state.add_tab(tab_type.clone());
        assert_eq!(result1, Ok(1));

        // Try to add the same tab again - should fail
        let result2 = state.add_tab(tab_type);
        assert_eq!(
            result2,
            Err("Tab with this type already exists".to_string())
        );

        // Tab count should remain 2
        assert_eq!(state.tab_count(), 2);
    }

    #[test]
    fn test_add_tab_home_duplicate() {
        let mut state = TimelineState::default();

        // Try to add another Home tab - should fail
        let result = state.add_tab(TimelineTabType::Home);
        assert_eq!(result, Err("Tab with this type already exists".to_string()));

        // Tab count should remain 1
        assert_eq!(state.tab_count(), 1);
    }

    #[test]
    fn test_add_multiple_user_timeline_tabs() {
        let mut state = TimelineState::default();

        // Add multiple UserTimeline tabs with different pubkeys
        let pubkey1 = Keys::generate().public_key();
        let pubkey2 = Keys::generate().public_key();
        let pubkey3 = Keys::generate().public_key();

        let result1 = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey1 });
        let result2 = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey2 });
        let result3 = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey3 });

        assert!(result1.is_ok());
        assert!(result2.is_ok());
        assert!(result3.is_ok());

        assert_eq!(state.tab_count(), 4); // Home + 3 user timelines
    }

    #[test]
    fn test_remove_tab_success() {
        let mut state = TimelineState::default();

        // Add a UserTimeline tab
        let pubkey = Keys::generate().public_key();
        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey });
        assert_eq!(state.tab_count(), 2);

        // Remove the UserTimeline tab
        let result = state.remove_tab(1);
        assert!(result.is_ok());
        assert_eq!(state.tab_count(), 1);
    }

    #[test]
    fn test_remove_tab_home_rejection() {
        let mut state = TimelineState::default();

        // Try to remove the Home tab - should fail
        let result = state.remove_tab(0);
        assert_eq!(result, Err("Cannot remove the Home tab".to_string()));

        // Tab count should remain 1
        assert_eq!(state.tab_count(), 1);
    }

    #[test]
    fn test_remove_tab_out_of_bounds() {
        let mut state = TimelineState::default();

        // Try to remove a non-existent tab
        let result = state.remove_tab(5);
        assert_eq!(result, Err("Tab index out of bounds".to_string()));

        // Tab count should remain 1
        assert_eq!(state.tab_count(), 1);
    }

    #[test]
    fn test_remove_tab_adjusts_active_index_when_before() {
        let mut state = TimelineState::default();

        // Add two UserTimeline tabs
        let pubkey1 = Keys::generate().public_key();
        let pubkey2 = Keys::generate().public_key();
        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey1 });
        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey2 });

        // Tabs: [Home(0), User1(1), User2(2)]
        assert_eq!(state.tab_count(), 3);

        // Select the third tab
        state.select_tab(2);
        assert_eq!(state.active_tab_index(), 2);

        // Remove the second tab (index 1) - should adjust active index
        let result = state.remove_tab(1);
        assert!(result.is_ok());

        // Active tab should now be at index 1 (previously at index 2)
        assert_eq!(state.active_tab_index(), 1);
        assert_eq!(state.tab_count(), 2);
    }

    #[test]
    fn test_remove_tab_adjusts_active_index_when_same() {
        let mut state = TimelineState::default();

        // Add two UserTimeline tabs
        let pubkey1 = Keys::generate().public_key();
        let pubkey2 = Keys::generate().public_key();
        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey1 });
        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey2 });

        // Tabs: [Home(0), User1(1), User2(2)]
        state.select_tab(1);
        assert_eq!(state.active_tab_index(), 1);

        // Remove the active tab (index 1)
        let result = state.remove_tab(1);
        assert!(result.is_ok());

        // Active tab should remain at index 1 (now points to what was User2)
        assert_eq!(state.active_tab_index(), 1);
        assert_eq!(state.tab_count(), 2);
    }

    #[test]
    fn test_remove_tab_adjusts_active_index_when_last() {
        let mut state = TimelineState::default();

        // Add two UserTimeline tabs
        let pubkey1 = Keys::generate().public_key();
        let pubkey2 = Keys::generate().public_key();
        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey1 });
        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey2 });

        // Tabs: [Home(0), User1(1), User2(2)]
        state.select_tab(2);
        assert_eq!(state.active_tab_index(), 2);

        // Remove the last tab (index 2)
        let result = state.remove_tab(2);
        assert!(result.is_ok());

        // Active tab should move to index 1 (the new last tab)
        assert_eq!(state.active_tab_index(), 1);
        assert_eq!(state.tab_count(), 2);
    }

    #[test]
    fn test_remove_tab_does_not_adjust_active_index_when_after() {
        let mut state = TimelineState::default();

        // Add two UserTimeline tabs
        let pubkey1 = Keys::generate().public_key();
        let pubkey2 = Keys::generate().public_key();
        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey1 });
        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey2 });

        // Tabs: [Home(0), User1(1), User2(2)]
        state.select_tab(0);
        assert_eq!(state.active_tab_index(), 0);

        // Remove a tab after the active one
        let result = state.remove_tab(2);
        assert!(result.is_ok());

        // Active tab should remain at index 0
        assert_eq!(state.active_tab_index(), 0);
        assert_eq!(state.tab_count(), 2);
    }

    #[test]
    fn test_find_tab_by_type_home() {
        let state = TimelineState::default();

        let result = state.find_tab_by_type(&TimelineTabType::Home);
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_find_tab_by_type_user_timeline() {
        let mut state = TimelineState::default();

        let pubkey = Keys::generate().public_key();
        let tab_type = TimelineTabType::UserTimeline { pubkey };

        // Not found initially
        assert_eq!(state.find_tab_by_type(&tab_type), None);

        // Add the tab
        let _ = state.add_tab(tab_type.clone());

        // Now it should be found
        assert_eq!(state.find_tab_by_type(&tab_type), Some(1));
    }

    #[test]
    fn test_find_tab_by_type_multiple_tabs() {
        let mut state = TimelineState::default();

        let pubkey1 = Keys::generate().public_key();
        let pubkey2 = Keys::generate().public_key();
        let pubkey3 = Keys::generate().public_key();

        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey1 });
        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey2 });
        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey3 });

        // Find each tab
        assert_eq!(
            state.find_tab_by_type(&TimelineTabType::UserTimeline { pubkey: pubkey1 }),
            Some(1)
        );
        assert_eq!(
            state.find_tab_by_type(&TimelineTabType::UserTimeline { pubkey: pubkey2 }),
            Some(2)
        );
        assert_eq!(
            state.find_tab_by_type(&TimelineTabType::UserTimeline { pubkey: pubkey3 }),
            Some(3)
        );

        // Non-existent pubkey should return None
        let pubkey_nonexistent = Keys::generate().public_key();
        assert_eq!(
            state.find_tab_by_type(&TimelineTabType::UserTimeline {
                pubkey: pubkey_nonexistent
            }),
            None
        );
    }

    #[test]
    fn test_next_tab_with_single_tab() {
        let mut state = TimelineState::default();

        // With only one tab, next_tab should stay at 0 (no wrap around)
        assert_eq!(state.active_tab_index(), 0);
        state.next_tab();
        assert_eq!(state.active_tab_index(), 0);
    }

    #[test]
    fn test_prev_tab_with_single_tab() {
        let mut state = TimelineState::default();

        // With only one tab, prev_tab should stay at 0 (no wrap around)
        assert_eq!(state.active_tab_index(), 0);
        state.prev_tab();
        assert_eq!(state.active_tab_index(), 0);
    }

    #[test]
    fn test_next_tab_with_multiple_tabs() {
        let mut state = TimelineState::default();

        // Add two more tabs (total 3 tabs: Home, User1, User2)
        let pubkey1 = Keys::generate().public_key();
        let pubkey2 = Keys::generate().public_key();

        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey1 });
        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey2 });

        // Verify we have 3 tabs
        assert_eq!(state.tab_count(), 3);

        // Start at tab 0
        state.select_tab(0);
        assert_eq!(state.active_tab_index(), 0);

        // Next tab -> tab 1
        state.next_tab();
        assert_eq!(state.active_tab_index(), 1);

        // Next tab -> tab 2
        state.next_tab();
        assert_eq!(state.active_tab_index(), 2);

        // Next tab at the end -> should stay at tab 2 (no wrap around)
        state.next_tab();
        assert_eq!(state.active_tab_index(), 2);
    }

    #[test]
    fn test_prev_tab_with_multiple_tabs() {
        let mut state = TimelineState::default();

        // Add two more tabs (total 3 tabs)
        let pubkey1 = Keys::generate().public_key();
        let pubkey2 = Keys::generate().public_key();

        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey1 });
        let _ = state.add_tab(TimelineTabType::UserTimeline { pubkey: pubkey2 });

        // Start at tab 0
        state.select_tab(0);
        assert_eq!(state.active_tab_index(), 0);

        // Prev tab at the start -> should stay at tab 0 (no wrap around)
        state.prev_tab();
        assert_eq!(state.active_tab_index(), 0);

        // Move to tab 2
        state.select_tab(2);
        assert_eq!(state.active_tab_index(), 2);

        // Prev tab -> tab 1
        state.prev_tab();
        assert_eq!(state.active_tab_index(), 1);

        // Prev tab -> tab 0
        state.prev_tab();
        assert_eq!(state.active_tab_index(), 0);

        // Prev tab at the start -> should stay at tab 0
        state.prev_tab();
        assert_eq!(state.active_tab_index(), 0);
    }

    #[test]
    fn test_add_note_to_tab_home() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();

        let event = EventBuilder::text_note("test note for home")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;

        // Add note to Home tab
        let (was_inserted, _) = state.add_note_to_tab(event, &TimelineTabType::Home);
        assert!(was_inserted);

        // Verify it's in the Home tab
        state.select_tab(0);
        assert_eq!(state.len(), 1);

        Ok(())
    }

    #[test]
    fn test_add_note_to_tab_user_timeline() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();
        let pubkey = keys.public_key();

        // Add a UserTimeline tab
        let tab_type = TimelineTabType::UserTimeline { pubkey };
        let _ = state.add_tab(tab_type.clone());

        // Create and add a note to the user timeline
        let event = EventBuilder::text_note("test note for user timeline")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;

        let (was_inserted, _) = state.add_note_to_tab(event, &tab_type);
        assert!(was_inserted);

        // Verify it's in the user timeline tab
        state.select_tab(1); // UserTimeline is at index 1
        assert_eq!(state.len(), 1);

        // Verify Home tab is still empty
        state.select_tab(0);
        assert_eq!(state.len(), 0);

        Ok(())
    }

    #[test]
    fn test_add_note_to_tab_nonexistent() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();
        let pubkey = keys.public_key();

        // Try to add a note to a tab that doesn't exist
        let event = EventBuilder::text_note("test note")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;

        let tab_type = TimelineTabType::UserTimeline { pubkey };
        let (was_inserted, _) = state.add_note_to_tab(event, &tab_type);

        // Should return false since tab doesn't exist
        assert!(!was_inserted);

        Ok(())
    }

    #[test]
    fn test_add_note_to_tab_multiple_tabs_independence() -> Result<()> {
        let mut state = TimelineState::default();
        let keys1 = Keys::generate();
        let keys2 = Keys::generate();

        let pubkey1 = keys1.public_key();
        let pubkey2 = keys2.public_key();

        // Add two UserTimeline tabs
        let tab_type1 = TimelineTabType::UserTimeline { pubkey: pubkey1 };
        let tab_type2 = TimelineTabType::UserTimeline { pubkey: pubkey2 };

        let _ = state.add_tab(tab_type1.clone());
        let _ = state.add_tab(tab_type2.clone());

        // Add notes to different tabs
        let event1 = EventBuilder::text_note("note for user 1")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys1)?;

        let event2 = EventBuilder::text_note("note for user 2")
            .custom_created_at(Timestamp::from(2000))
            .sign_with_keys(&keys2)?;

        let event_home = EventBuilder::text_note("note for home")
            .custom_created_at(Timestamp::from(3000))
            .sign_with_keys(&keys1)?;

        state.add_note_to_tab(event1, &tab_type1);
        state.add_note_to_tab(event2, &tab_type2);
        state.add_note_to_tab(event_home, &TimelineTabType::Home);

        // Verify each tab has only its own notes
        state.select_tab(0); // Home
        assert_eq!(state.len(), 1);

        state.select_tab(1); // User 1
        assert_eq!(state.len(), 1);

        state.select_tab(2); // User 2
        assert_eq!(state.len(), 1);

        Ok(())
    }

    #[test]
    fn test_add_note_to_tab_adjusts_selection_when_active() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();
        let pubkey = keys.public_key();

        // Add a UserTimeline tab
        let tab_type = TimelineTabType::UserTimeline { pubkey };
        let _ = state.add_tab(tab_type.clone());

        // Switch to the user timeline tab
        state.select_tab(1);

        // Add initial notes
        let event1 = EventBuilder::text_note("note 1")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;
        state.add_note_to_tab(event1, &tab_type);

        let event2 = EventBuilder::text_note("note 2")
            .custom_created_at(Timestamp::from(2000))
            .sign_with_keys(&keys)?;
        state.add_note_to_tab(event2, &tab_type);

        // Select the second note (index 1, older note)
        state.select(1);
        assert_eq!(state.selected_index(), Some(1));

        // Add a newer note (will be inserted at index 0)
        let event3 = EventBuilder::text_note("note 3")
            .custom_created_at(Timestamp::from(3000))
            .sign_with_keys(&keys)?;
        state.add_note_to_tab(event3, &tab_type);

        // Selection should be adjusted to index 2
        assert_eq!(state.selected_index(), Some(2));

        Ok(())
    }

    #[test]
    fn test_add_note_to_tab_does_not_adjust_selection_when_inactive() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();
        let pubkey = keys.public_key();

        // Add a UserTimeline tab
        let tab_type = TimelineTabType::UserTimeline { pubkey };
        let _ = state.add_tab(tab_type.clone());

        // Switch to the user timeline tab
        state.select_tab(1);

        // Add initial notes
        let event1 = EventBuilder::text_note("note 1")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;
        state.add_note_to_tab(event1, &tab_type);

        let event2 = EventBuilder::text_note("note 2")
            .custom_created_at(Timestamp::from(2000))
            .sign_with_keys(&keys)?;
        state.add_note_to_tab(event2, &tab_type);

        // Select the second note
        state.select(1);
        assert_eq!(state.selected_index(), Some(1));

        // Switch to Home tab
        state.select_tab(0);

        // Add a newer note to user timeline (while Home tab is active)
        let event3 = EventBuilder::text_note("note 3")
            .custom_created_at(Timestamp::from(3000))
            .sign_with_keys(&keys)?;
        state.add_note_to_tab(event3, &tab_type);

        // Switch back to user timeline
        state.select_tab(1);

        // Selection should NOT be adjusted (still at index 1, but now points to a different note)
        assert_eq!(state.selected_index(), Some(1));

        Ok(())
    }
}
