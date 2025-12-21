use nostr_sdk::prelude::*;
use sorted_vec::ReverseSortedSet;
use std::{cmp::Reverse, collections::HashMap};

use crate::{
    core::{cmd::Cmd, msg::timeline::TimelineMsg},
    domain::{collections::EventSet, nostr::SortableEvent},
};

/// Timeline-related state
#[derive(Debug, Clone)]
pub struct TimelineState {
    pub notes: ReverseSortedSet<SortableEvent>,
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
    /// Timeline-specific update function
    /// Returns: Generated commands
    pub fn update(&mut self, msg: TimelineMsg) -> Vec<Cmd> {
        match msg {
            // Scroll operations
            TimelineMsg::ScrollUp => {
                if !self.notes.is_empty() {
                    let new_index = match self.selected_index {
                        Some(i) if i > 0 => Some(i - 1),
                        Some(_) => Some(0),
                        None => Some(0),
                    };
                    self.selected_index = new_index;
                }
                vec![]
            }

            TimelineMsg::ScrollDown => {
                if !self.notes.is_empty() {
                    let max_index = self.notes.len().saturating_sub(1);
                    let new_index = match self.selected_index {
                        Some(i) if i < max_index => Some(i + 1),
                        Some(_) => Some(max_index),
                        None => Some(0),
                    };
                    self.selected_index = new_index;
                }
                vec![]
            }

            TimelineMsg::ScrollToTop => {
                if !self.notes.is_empty() {
                    self.selected_index = Some(0);
                }
                vec![]
            }

            TimelineMsg::ScrollToBottom => {
                if !self.notes.is_empty() {
                    self.selected_index = Some(self.notes.len().saturating_sub(1));
                }
                vec![]
            }

            // Selection operations
            TimelineMsg::SelectNote(index) => {
                self.selected_index = Some(index);
                vec![]
            }

            TimelineMsg::DeselectNote => {
                self.selected_index = None;
                vec![]
            }

            // Nostr event additions
            TimelineMsg::AddNote(event) => {
                let sortable_event = SortableEvent::new(event);
                let note = Reverse(sortable_event);

                self.notes.find_or_insert(note);

                // Adjust selection position (new note was added)
                if let Some(selected) = self.selected_index {
                    self.selected_index = Some(selected + 1);
                }
                vec![]
            }

            TimelineMsg::AddReaction(reaction) => {
                if let Some(event_id) = extract_last_event_id(&reaction) {
                    self.reactions.entry(event_id).or_default().insert(reaction);
                }
                vec![]
            }

            TimelineMsg::AddRepost(repost) => {
                if let Some(event_id) = extract_last_event_id(&repost) {
                    self.reposts.entry(event_id).or_default().insert(repost);
                }
                vec![]
            }

            TimelineMsg::AddZapReceipt(zap_receipt) => {
                if let Some(event_id) = extract_last_event_id(&zap_receipt) {
                    self.zap_receipts
                        .entry(event_id)
                        .or_default()
                        .insert(zap_receipt);
                }
                vec![]
            }
        }
    }

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

/// Helper function to extract event_id from the last e tag of an event
fn extract_last_event_id(event: &Event) -> Option<EventId> {
    use nostr_sdk::nostr::{Alphabet, SingleLetterTag, TagKind, TagStandard};

    event
        .tags
        .iter()
        .filter(|tag| tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E)))
        .next_back()
        .and_then(|tag| {
            if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized() {
                Some(*event_id)
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_event() -> Result<Event> {
        create_test_event_with_content("test content")
    }

    fn create_test_event_with_content(content: &str) -> Result<Event> {
        let keys = Keys::generate();
        EventBuilder::text_note(content)
            .sign_with_keys(&keys)
            .map_err(|e| e.into())
    }

    // Unit tests for scroll operations
    #[test]
    fn test_scroll_up_unit() -> Result<()> {
        let mut timeline = TimelineState::default();

        // Empty timeline - no change
        let cmds = timeline.update(TimelineMsg::ScrollUp);
        assert!(timeline.selected_index.is_none());
        assert!(cmds.is_empty());

        // Add some notes
        timeline
            .notes
            .find_or_insert(Reverse(SortableEvent::new(create_test_event()?)));
        timeline
            .notes
            .find_or_insert(Reverse(SortableEvent::new(create_test_event()?)));
        timeline
            .notes
            .find_or_insert(Reverse(SortableEvent::new(create_test_event()?)));
        timeline.selected_index = Some(2);

        let cmds = timeline.update(TimelineMsg::ScrollUp);
        assert_eq!(timeline.selected_index, Some(1));
        assert!(cmds.is_empty());

        // At top - stays at 0
        timeline.selected_index = Some(0);
        let cmds = timeline.update(TimelineMsg::ScrollUp);
        assert_eq!(timeline.selected_index, Some(0));
        assert!(cmds.is_empty());

        Ok(())
    }

    #[test]
    fn test_scroll_down_unit() -> Result<()> {
        let mut timeline = TimelineState::default();

        // Add some notes
        timeline
            .notes
            .find_or_insert(Reverse(SortableEvent::new(create_test_event()?)));
        timeline
            .notes
            .find_or_insert(Reverse(SortableEvent::new(create_test_event()?)));
        timeline
            .notes
            .find_or_insert(Reverse(SortableEvent::new(create_test_event()?)));
        timeline.selected_index = Some(0);

        let cmds = timeline.update(TimelineMsg::ScrollDown);
        assert_eq!(timeline.selected_index, Some(1));
        assert!(cmds.is_empty());

        // At bottom - stays at max
        timeline.selected_index = Some(2);
        let cmds = timeline.update(TimelineMsg::ScrollDown);
        assert_eq!(timeline.selected_index, Some(2));
        assert!(cmds.is_empty());

        Ok(())
    }

    #[test]
    fn test_scroll_to_positions_unit() -> Result<()> {
        let mut timeline = TimelineState::default();

        // Add some notes
        timeline
            .notes
            .find_or_insert(Reverse(SortableEvent::new(create_test_event()?)));
        timeline
            .notes
            .find_or_insert(Reverse(SortableEvent::new(create_test_event()?)));
        timeline
            .notes
            .find_or_insert(Reverse(SortableEvent::new(create_test_event()?)));

        // Scroll to top
        timeline.selected_index = Some(1);
        let cmds = timeline.update(TimelineMsg::ScrollToTop);
        assert_eq!(timeline.selected_index, Some(0));
        assert!(cmds.is_empty());

        // Scroll to bottom
        let cmds = timeline.update(TimelineMsg::ScrollToBottom);
        assert_eq!(timeline.selected_index, Some(2));
        assert!(cmds.is_empty());

        Ok(())
    }

    // Unit tests for note selection
    #[test]
    fn test_note_selection_unit() {
        let mut timeline = TimelineState::default();

        // Select note
        let cmds = timeline.update(TimelineMsg::SelectNote(5));
        assert_eq!(timeline.selected_index, Some(5));
        assert!(cmds.is_empty());

        // Deselect note
        let cmds = timeline.update(TimelineMsg::DeselectNote);
        assert_eq!(timeline.selected_index, None);
        assert!(cmds.is_empty());
    }

    // Unit tests for Nostr event additions
    #[test]
    fn test_add_note_unit() -> Result<()> {
        let mut timeline = TimelineState::default();
        assert_eq!(timeline.len(), 0);

        let event = create_test_event()?;
        let cmds = timeline.update(TimelineMsg::AddNote(event));

        assert_eq!(timeline.len(), 1);
        assert!(cmds.is_empty());

        // Selection adjustment when note is added
        timeline.selected_index = Some(0);
        let event2 = create_test_event()?;
        let cmds = timeline.update(TimelineMsg::AddNote(event2));

        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline.selected_index, Some(1)); // Adjusted
        assert!(cmds.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_reactions_unit() -> Result<()> {
        let mut timeline = TimelineState::default();

        // Create event and reaction
        let target_event = create_test_event()?;
        let target_id = target_event.id;

        let reaction =
            EventBuilder::reaction(&target_event, "👍").sign_with_keys(&Keys::generate())?;

        let cmds = timeline.update(TimelineMsg::AddReaction(reaction.clone()));

        assert!(timeline.reactions.contains_key(&target_id));
        assert!(timeline.reactions[&target_id].contains(&reaction.id));
        assert!(cmds.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_reposts_unit() -> Result<()> {
        let mut timeline = TimelineState::default();

        let target_event = create_test_event()?;
        let target_id = target_event.id;

        let repost = EventBuilder::repost(&target_event, None).sign_with_keys(&Keys::generate())?;

        let cmds = timeline.update(TimelineMsg::AddRepost(repost.clone()));

        assert!(timeline.reposts.contains_key(&target_id));
        assert!(timeline.reposts[&target_id].contains(&repost.id));
        assert!(cmds.is_empty());

        Ok(())
    }

    // Integration tests
    #[test]
    fn test_timeline_complete_flow_unit() -> Result<()> {
        let mut timeline = TimelineState::default();

        // 1. Add multiple notes
        let event1 = create_test_event_with_content("First note")?;
        let event2 = create_test_event_with_content("Second note")?;
        let event3 = create_test_event_with_content("Third note")?;

        timeline.update(TimelineMsg::AddNote(event1.clone()));
        timeline.update(TimelineMsg::AddNote(event2));
        timeline.update(TimelineMsg::AddNote(event3));

        assert_eq!(timeline.len(), 3);

        // 2. Navigate timeline
        timeline.update(TimelineMsg::SelectNote(0));
        assert_eq!(timeline.selected_index, Some(0));

        timeline.update(TimelineMsg::ScrollDown);
        assert_eq!(timeline.selected_index, Some(1));

        timeline.update(TimelineMsg::ScrollToBottom);
        assert_eq!(timeline.selected_index, Some(2));

        // 3. Add engagement
        let reaction = EventBuilder::reaction(&event1, "👍").sign_with_keys(&Keys::generate())?;
        timeline.update(TimelineMsg::AddReaction(reaction));

        assert!(!timeline.reactions.is_empty());

        // 4. Deselect
        timeline.update(TimelineMsg::DeselectNote);
        assert_eq!(timeline.selected_index, None);

        Ok(())
    }

    // Helper methods tests
    #[test]
    fn test_helper_methods_unit() -> Result<()> {
        let mut timeline = TimelineState::default();

        assert!(timeline.is_empty());
        assert_eq!(timeline.len(), 0);
        assert!(timeline.selected_note().is_none());

        let event = create_test_event()?;
        timeline.update(TimelineMsg::AddNote(event.clone()));
        timeline.update(TimelineMsg::SelectNote(0));

        assert!(!timeline.is_empty());
        assert_eq!(timeline.len(), 1);

        let selected = timeline.selected_note();
        assert!(matches!(selected, Some(s) if s.id == event.id));

        Ok(())
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
}
