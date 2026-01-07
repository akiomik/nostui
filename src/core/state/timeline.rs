use nostr_sdk::prelude::*;
use sorted_vec::ReverseSortedSet;
use std::{cmp::Reverse, collections::HashMap, slice::Iter};

use crate::domain::{collections::EventSet, nostr::EventWrapper};

/// Timeline-related state
#[derive(Debug, Clone)]
pub struct TimelineState {
    notes: ReverseSortedSet<EventWrapper>,
    reactions: HashMap<EventId, EventSet>,
    reposts: HashMap<EventId, EventSet>,
    zap_receipts: HashMap<EventId, EventSet>,
    selected_index: Option<usize>,
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

    pub fn iter(&self) -> Iter<'_, Reverse<EventWrapper>> {
        self.notes.iter()
    }

    /// Get the index of currently selected note
    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    /// Get reactions for the specified event
    pub fn reactions_for(&self, event_id: &EventId) -> EventSet {
        self.reactions
            .get(event_id)
            .cloned()
            .unwrap_or_else(EventSet::new)
    }

    /// Get reposts for the specified event
    pub fn reposts_for(&self, event_id: &EventId) -> EventSet {
        self.reposts
            .get(event_id)
            .cloned()
            .unwrap_or_else(EventSet::new)
    }

    /// Get zap receipts for the specified event
    pub fn zap_receipts_for(&self, event_id: &EventId) -> EventSet {
        self.zap_receipts
            .get(event_id)
            .cloned()
            .unwrap_or_else(EventSet::new)
    }

    /// Add a text note to the timeline
    /// Returns `true` if the event was newly inserted, `false` if it already existed
    /// Automatically adjusts the selected index if a new item is inserted before it
    pub fn add_note(&mut self, event: Event) -> bool {
        let wrapper = EventWrapper::new(event);
        let insert_result = self.notes.find_or_insert(Reverse(wrapper));

        // Adjust selected index if a new item was inserted before it
        // This prevents the selection from shifting when new events arrive
        if let sorted_vec::FindOrInsert::Inserted(inserted_at) = insert_result {
            if let Some(selected) = self.selected_index {
                if inserted_at <= selected {
                    self.selected_index = Some(selected + 1);
                }
            }
            true
        } else {
            false
        }
    }

    /// Add a reaction event to the timeline
    /// Returns the ID of the event being reacted to, or `None` if no valid target event is found
    pub fn add_reaction(&mut self, event: Event) -> Option<EventId> {
        let wrapper = EventWrapper::new(event);
        if let Some(event_id) = wrapper.last_event_id_from_tags() {
            self.reactions
                .entry(event_id)
                .or_default()
                .insert(wrapper.event);
            Some(event_id)
        } else {
            None
        }
    }

    /// Add a repost event to the timeline
    /// Returns the ID of the event being reposted, or `None` if no valid target event is found
    pub fn add_repost(&mut self, event: Event) -> Option<EventId> {
        let wrapper = EventWrapper::new(event);
        if let Some(event_id) = wrapper.last_event_id_from_tags() {
            self.reposts
                .entry(event_id)
                .or_default()
                .insert(wrapper.event);
            Some(event_id)
        } else {
            None
        }
    }

    /// Add a zap receipt event to the timeline
    /// Returns the ID of the event being zapped, or `None` if no valid target event is found
    pub fn add_zap_receipt(&mut self, event: Event) -> Option<EventId> {
        let wrapper = EventWrapper::new(event);
        if let Some(event_id) = wrapper.last_event_id_from_tags() {
            self.zap_receipts
                .entry(event_id)
                .or_default()
                .insert(wrapper.event);
            Some(event_id)
        } else {
            None
        }
    }

    /// Move selection up by one item
    /// If no item is selected, selects the first item
    pub fn scroll_up(&mut self) {
        if let Some(current) = self.selected_index {
            if current > 0 {
                self.selected_index = Some(current - 1);
            }
        } else if !self.is_empty() {
            self.select_first();
        }
    }

    /// Move selection down by one item
    /// If no item is selected, selects the first item
    pub fn scroll_down(&mut self) {
        let max_index = self.len().saturating_sub(1);
        if let Some(current) = self.selected_index {
            if current < max_index {
                self.selected_index = Some(current + 1);
            }
        } else if !self.is_empty() {
            self.select_first();
        }
    }

    /// Get the currently selected note
    pub fn selected_note(&self) -> Option<&Event> {
        self.selected_index
            .and_then(|i| self.notes.get(i))
            .map(|sortable| &sortable.0.event)
    }

    /// Select a note at the specified index
    /// If the index is out of bounds, deselects the current selection
    pub fn select(&mut self, index: usize) {
        if index < self.len() {
            self.selected_index = Some(index);
        } else {
            self.deselect();
        }
    }

    /// Select the first note in the timeline
    pub fn select_first(&mut self) {
        if !self.is_empty() {
            self.selected_index = Some(0);
        }
    }

    /// Select the last note in the timeline
    pub fn select_last(&mut self) {
        if !self.is_empty() {
            let max_index = self.len().saturating_sub(1);
            self.selected_index = Some(max_index);
        }
    }

    /// Clear the current selection
    pub fn deselect(&mut self) {
        self.selected_index = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper function to create a test event with a specific timestamp
    fn create_test_event(timestamp: u64) -> EventWrapper {
        let keys = Keys::generate();
        let event = EventBuilder::text_note(format!("test note {timestamp}"))
            .custom_created_at(Timestamp::from(timestamp))
            .sign_with_keys(&keys)
            .expect("Failed to sign event");
        EventWrapper::new(event)
    }

    /// Helper function to insert a test event into the timeline
    fn insert_test_event(state: &mut TimelineState, timestamp: u64) {
        let _ = state
            .notes
            .find_or_insert(Reverse(create_test_event(timestamp)));
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
        state.selected_index = Some(0);
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
        assert_eq!(state.selected_index, Some(1));

        // Select another valid index
        state.select(2);
        assert_eq!(state.selected_index, Some(2));

        // Select an invalid index should deselect
        state.select(10);
        assert_eq!(state.selected_index, None);
    }

    #[test]
    fn test_select_first() {
        let mut state = TimelineState::default();

        // select_first on empty timeline should do nothing
        state.select_first();
        assert_eq!(state.selected_index, None);

        // Add notes and select first
        insert_test_event(&mut state, 1000);
        insert_test_event(&mut state, 2000);
        state.select_first();
        assert_eq!(state.selected_index, Some(0));
    }

    #[test]
    fn test_select_last() {
        let mut state = TimelineState::default();

        // select_last on empty timeline should do nothing
        state.select_last();
        assert_eq!(state.selected_index, None);

        // Add notes and select last
        insert_test_event(&mut state, 1000);
        insert_test_event(&mut state, 2000);
        insert_test_event(&mut state, 3000);
        state.select_last();
        assert_eq!(state.selected_index, Some(2));
    }

    #[test]
    fn test_deselect() {
        let mut state = TimelineState::default();
        insert_test_event(&mut state, 1000);
        state.select(0);
        assert_eq!(state.selected_index, Some(0));

        state.deselect();
        assert_eq!(state.selected_index, None);
    }

    #[test]
    fn test_scroll_up() {
        let mut state = TimelineState::default();

        // scroll_up on empty timeline should do nothing
        state.scroll_up();
        assert_eq!(state.selected_index, None);

        // Add notes
        insert_test_event(&mut state, 1000);
        insert_test_event(&mut state, 2000);
        insert_test_event(&mut state, 3000);

        // scroll_up with no selection should select first
        state.scroll_up();
        assert_eq!(state.selected_index, Some(0));

        // scroll_up at the top should stay at the top
        state.scroll_up();
        assert_eq!(state.selected_index, Some(0));

        // Move to middle and scroll up
        state.select(2);
        state.scroll_up();
        assert_eq!(state.selected_index, Some(1));
    }

    #[test]
    fn test_scroll_down() {
        let mut state = TimelineState::default();

        // scroll_down on empty timeline should do nothing
        state.scroll_down();
        assert_eq!(state.selected_index, None);

        // Add notes
        insert_test_event(&mut state, 1000);
        insert_test_event(&mut state, 2000);
        insert_test_event(&mut state, 3000);

        // scroll_down with no selection should select first
        state.scroll_down();
        assert_eq!(state.selected_index, Some(0));

        // scroll_down should move down
        state.scroll_down();
        assert_eq!(state.selected_index, Some(1));

        state.scroll_down();
        assert_eq!(state.selected_index, Some(2));

        // scroll_down at the bottom should stay at the bottom
        state.scroll_down();
        assert_eq!(state.selected_index, Some(2));
    }

    #[test]
    fn test_scroll_navigation_sequence() {
        let mut state = TimelineState::default();

        // Add notes
        insert_test_event(&mut state, 1000);
        insert_test_event(&mut state, 2000);
        insert_test_event(&mut state, 3000);

        // Start with no selection
        assert_eq!(state.selected_index, None);

        // First scroll down selects first item
        state.scroll_down();
        assert_eq!(state.selected_index, Some(0));

        // Continue scrolling down
        state.scroll_down();
        assert_eq!(state.selected_index, Some(1));

        // Scroll up
        state.scroll_up();
        assert_eq!(state.selected_index, Some(0));

        // Scroll up at top stays at top
        state.scroll_up();
        assert_eq!(state.selected_index, Some(0));
    }

    #[test]
    fn test_selected_note_with_data() {
        let mut state = TimelineState::default();

        // Add notes with known content
        let event1 = create_test_event(1000);
        let event2 = create_test_event(2000);
        let event1_id = event1.event.id;
        let event2_id = event2.event.id;

        let _ = state.notes.find_or_insert(Reverse(event1));
        let _ = state.notes.find_or_insert(Reverse(event2));

        // Select first note
        state.select(0);
        let selected = state.selected_note().expect("should exist");
        // ReverseSortedSet sorts in reverse order, so index 0 is the newest (2000)
        assert_eq!(selected.id, event2_id);

        // Select second note
        state.select(1);
        let selected = state.selected_note().expect("should exist");
        assert_eq!(selected.id, event1_id);

        // Deselect
        state.deselect();
        assert!(state.selected_note().is_none());
    }

    #[test]
    fn test_add_note_returns_correct_value() -> Result<()> {
        let mut state = TimelineState::default();

        let keys = Keys::generate();
        let event1 = EventBuilder::text_note("test 1")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;

        // First insert should return true
        assert!(state.add_note(event1.clone()));
        assert_eq!(state.len(), 1);

        // Duplicate insert should return false
        assert!(!state.add_note(event1));
        assert_eq!(state.len(), 1);

        Ok(())
    }

    #[test]
    fn test_add_note_without_selection() -> Result<()> {
        let mut state = TimelineState::default();

        let keys = Keys::generate();

        // Add first note
        let event1 = EventBuilder::text_note("test 1")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;
        state.add_note(event1);

        // Selection should remain None
        assert_eq!(state.selected_index, None);

        // Add second note (newer, will be inserted at index 0)
        let event2 = EventBuilder::text_note("test 2")
            .custom_created_at(Timestamp::from(2000))
            .sign_with_keys(&keys)?;
        state.add_note(event2);

        // Selection should still be None
        assert_eq!(state.selected_index, None);
        assert_eq!(state.len(), 2);

        Ok(())
    }

    #[test]
    fn test_add_note_adjusts_selection_when_inserted_before() -> Result<()> {
        let mut state = TimelineState::default();

        let keys = Keys::generate();

        // Add initial notes
        let event1 = EventBuilder::text_note("test 1")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;
        state.add_note(event1);

        let event2 = EventBuilder::text_note("test 2")
            .custom_created_at(Timestamp::from(2000))
            .sign_with_keys(&keys)?;
        state.add_note(event2);

        let event3 = EventBuilder::text_note("test 3")
            .custom_created_at(Timestamp::from(3000))
            .sign_with_keys(&keys)?;
        state.add_note(event3);

        // Timeline: [3000, 2000, 1000] (indices: 0, 1, 2)
        assert_eq!(state.len(), 3);

        // Select the middle item (2000 at index 1)
        state.select(1);
        assert_eq!(state.selected_index, Some(1));

        // Add a newer note (4000) - will be inserted at index 0
        let event4 = EventBuilder::text_note("test 4")
            .custom_created_at(Timestamp::from(4000))
            .sign_with_keys(&keys)?;
        state.add_note(event4);

        // Selection should be adjusted to index 2 to keep pointing to the same note
        // Timeline: [4000, 3000, 2000, 1000] (indices: 0, 1, 2, 3)
        assert_eq!(state.selected_index, Some(2));
        assert_eq!(state.len(), 4);

        Ok(())
    }

    #[test]
    fn test_add_note_does_not_adjust_selection_when_inserted_after() -> Result<()> {
        let mut state = TimelineState::default();

        let keys = Keys::generate();

        // Add initial notes
        let event1 = EventBuilder::text_note("test 1")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;
        state.add_note(event1);

        let event2 = EventBuilder::text_note("test 2")
            .custom_created_at(Timestamp::from(3000))
            .sign_with_keys(&keys)?;
        state.add_note(event2);

        // Timeline: [3000, 1000] (indices: 0, 1)
        assert_eq!(state.len(), 2);

        // Select the first item (3000 at index 0)
        state.select(0);
        assert_eq!(state.selected_index, Some(0));

        // Add an older note (2000) - will be inserted at index 1
        let event3 = EventBuilder::text_note("test 3")
            .custom_created_at(Timestamp::from(2000))
            .sign_with_keys(&keys)?;
        state.add_note(event3);

        // Selection should remain at index 0
        // Timeline: [3000, 2000, 1000] (indices: 0, 1, 2)
        assert_eq!(state.selected_index, Some(0));
        assert_eq!(state.len(), 3);

        Ok(())
    }

    #[test]
    fn test_add_note_edge_case_insert_at_selected_index() -> Result<()> {
        let mut state = TimelineState::default();

        let keys = Keys::generate();

        // Add initial notes
        let event1 = EventBuilder::text_note("test 1")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)?;
        state.add_note(event1);

        let event2 = EventBuilder::text_note("test 2")
            .custom_created_at(Timestamp::from(3000))
            .sign_with_keys(&keys)?;
        state.add_note(event2);

        // Timeline: [3000, 1000] (indices: 0, 1)
        // Select the second item (1000 at index 1)
        state.select(1);
        assert_eq!(state.selected_index, Some(1));

        // Add a note with timestamp 2000 - will be inserted at index 1
        let event3 = EventBuilder::text_note("test 3")
            .custom_created_at(Timestamp::from(2000))
            .sign_with_keys(&keys)?;
        state.add_note(event3);

        // Since inserted_at (1) <= selected (1), selection should be adjusted
        // Timeline: [3000, 2000, 1000] (indices: 0, 1, 2)
        assert_eq!(state.selected_index, Some(2));
        assert_eq!(state.len(), 3);

        Ok(())
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

        state.add_note(target_note.clone());

        // Create a reaction to the target note
        let reaction = EventBuilder::reaction(&target_note, "👍").sign_with_keys(&keys)?;
        let reaction_id = reaction.id;

        // Add the reaction
        let result = state.add_reaction(reaction);

        // Should return the target event ID
        assert_eq!(result, Some(target_id));

        // The reaction should be stored in the reactions map
        assert!(state.reactions.contains_key(&target_id));
        let reactions = state.reactions_for(&target_id);
        assert_eq!(reactions.len(), 1);
        assert!(reactions.contains(&reaction_id));

        Ok(())
    }

    #[test]
    fn test_add_reaction_without_target() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();

        // Create a reaction-like event without an 'e' tag
        let invalid_reaction =
            EventBuilder::text_note("not a valid reaction").sign_with_keys(&keys)?;

        // Add the invalid reaction
        let result = state.add_reaction(invalid_reaction);

        // Should return None
        assert_eq!(result, None);

        // No reactions should be stored
        assert!(state.reactions.is_empty());

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

        state.add_note(target_note.clone());

        // Create multiple reactions from different users
        let reaction1 = EventBuilder::reaction(&target_note, "👍").sign_with_keys(&keys1)?;
        let reaction2 = EventBuilder::reaction(&target_note, "🔥").sign_with_keys(&keys2)?;

        state.add_reaction(reaction1.clone());
        state.add_reaction(reaction2.clone());

        // Both reactions should be stored
        let reactions = state.reactions_for(&target_id);
        assert_eq!(reactions.len(), 2);
        assert!(reactions.contains(&reaction1.id));
        assert!(reactions.contains(&reaction2.id));

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

        state.add_note(target_note.clone());

        // Create a repost of the target note
        let repost = EventBuilder::repost(&target_note, None).sign_with_keys(&keys)?;
        let repost_id = repost.id;

        // Add the repost
        let result = state.add_repost(repost);

        // Should return the target event ID
        assert_eq!(result, Some(target_id));

        // The repost should be stored in the reposts map
        assert!(state.reposts.contains_key(&target_id));
        let reposts = state.reposts_for(&target_id);
        assert_eq!(reposts.len(), 1);
        assert!(reposts.contains(&repost_id));

        Ok(())
    }

    #[test]
    fn test_add_repost_without_target() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();

        // Create a repost-like event without an 'e' tag
        let invalid_repost = EventBuilder::text_note("not a valid repost").sign_with_keys(&keys)?;

        // Add the invalid repost
        let result = state.add_repost(invalid_repost);

        // Should return None
        assert_eq!(result, None);

        // No reposts should be stored
        assert!(state.reposts.is_empty());

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

        state.add_note(target_note);

        // Create a zap receipt (Kind 9735) with an 'e' tag pointing to the target
        let zap_receipt = EventBuilder::new(Kind::from(9735), "zap receipt")
            .tag(Tag::event(target_id))
            .sign_with_keys(&keys)?;
        let zap_id = zap_receipt.id;

        // Add the zap receipt
        let result = state.add_zap_receipt(zap_receipt);

        // Should return the target event ID
        assert_eq!(result, Some(target_id));

        // The zap receipt should be stored in the zap_receipts map
        assert!(state.zap_receipts.contains_key(&target_id));
        let zaps = state.zap_receipts_for(&target_id);
        assert_eq!(zaps.len(), 1);
        assert!(zaps.contains(&zap_id));

        Ok(())
    }

    #[test]
    fn test_add_zap_receipt_without_target() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();

        // Create a zap receipt without an 'e' tag
        let invalid_zap =
            EventBuilder::new(Kind::from(9735), "invalid zap").sign_with_keys(&keys)?;

        // Add the invalid zap receipt
        let result = state.add_zap_receipt(invalid_zap);

        // Should return None
        assert_eq!(result, None);

        // No zap receipts should be stored
        assert!(state.zap_receipts.is_empty());

        Ok(())
    }

    #[test]
    fn test_reactions_reposts_zaps_independence() -> Result<()> {
        let mut state = TimelineState::default();
        let keys = Keys::generate();

        // Create a target note
        let target_note = EventBuilder::text_note("popular note").sign_with_keys(&keys)?;
        let target_id = target_note.id;

        state.add_note(target_note.clone());

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
        assert_eq!(state.reactions_for(&target_id).len(), 1);
        assert_eq!(state.reposts_for(&target_id).len(), 1);
        assert_eq!(state.zap_receipts_for(&target_id).len(), 1);

        Ok(())
    }
}
