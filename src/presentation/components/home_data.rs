use nostr_sdk::prelude::*;
use ratatui::{prelude::*, widgets::*};

use crate::{
    core::state::AppState, domain::collections::EventSet,
    presentation::widgets::text_note::TextNote,
};

/// Elm-architecture compatible data layer for Home component
/// This component handles pure data operations: note display, profile management, social features
/// No internal state management - all data comes from AppState
#[derive(Debug, Clone)]
pub struct HomeData;

impl HomeData {
    pub fn new() -> Self {
        Self
    }

    /// Generate timeline items from AppState
    /// Pure function that transforms state data into displayable items
    pub fn generate_timeline_items(
        &self,
        state: &AppState,
        area: Rect,
        padding: Padding,
    ) -> Vec<(TextNote, u16)> {
        let mut items = Vec::new();

        for sortable_event in &state.timeline.notes {
            let event = &sortable_event.0.event;
            let text_note = self.create_text_note(event.clone(), state, area, padding);
            let height = text_note.calculate_height();
            items.push((text_note, height));
        }

        items
    }

    /// Create a TextNote widget from event and app state
    /// Pure function that aggregates all related data for display
    pub fn create_text_note(
        &self,
        event: Event,
        state: &AppState,
        area: Rect,
        padding: Padding,
    ) -> TextNote {
        let profile = state.user.profiles.get(&event.pubkey).cloned();
        let reactions = state
            .timeline
            .reactions
            .get(&event.id)
            .cloned()
            .unwrap_or_else(EventSet::new);
        let reposts = state
            .timeline
            .reposts
            .get(&event.id)
            .cloned()
            .unwrap_or_else(EventSet::new);
        let zap_receipts = state
            .timeline
            .zap_receipts
            .get(&event.id)
            .cloned()
            .unwrap_or_else(EventSet::new);

        TextNote::new(
            event,
            profile,
            reactions,
            reposts,
            zap_receipts,
            area,
            padding,
        )
    }

    /// Get note at specific index
    /// Safe accessor that returns None if index is out of bounds
    pub fn get_note_at_index(state: &AppState, index: usize) -> Option<&Event> {
        state
            .timeline
            .notes
            .get(index)
            .map(|sortable| &sortable.0.event)
    }

    /// Get currently selected note
    /// Pure function based on AppState selection
    pub fn get_selected_note(state: &AppState) -> Option<&Event> {
        state
            .timeline
            .selected_index
            .and_then(|index| Self::get_note_at_index(state, index))
    }

    /// Calculate statistics for timeline
    /// Pure function for dashboard/status display
    pub fn calculate_timeline_stats(state: &AppState) -> TimelineStats {
        let total_notes = state.timeline.notes.len();
        let total_profiles = state.user.profiles.len();

        let total_reactions: usize = state
            .timeline
            .reactions
            .values()
            .map(|reactions| reactions.len())
            .sum();

        let total_reposts: usize = state
            .timeline
            .reposts
            .values()
            .map(|reposts| reposts.len())
            .sum();

        let total_zaps: usize = state
            .timeline
            .zap_receipts
            .values()
            .map(|zaps| zaps.len())
            .sum();

        TimelineStats {
            total_notes,
            total_profiles,
            total_reactions,
            total_reposts,
            total_zaps,
        }
    }

    /// Check if we have profile data for a specific pubkey
    /// Pure function for UI conditional rendering
    pub fn has_profile_data(state: &AppState, pubkey: &PublicKey) -> bool {
        state.user.profiles.contains_key(pubkey)
    }

    /// Get all unique authors in timeline
    /// Pure function for UI information (doesn't trigger actions)
    pub fn get_timeline_authors(state: &AppState) -> Vec<PublicKey> {
        let mut authors: std::collections::HashSet<PublicKey> = std::collections::HashSet::new();
        for sortable_event in &state.timeline.notes {
            authors.insert(sortable_event.0.event.pubkey);
        }
        authors.into_iter().collect()
    }

    /// Get social engagement for a specific event
    /// Pure function that aggregates all social metrics
    pub fn get_event_engagement(state: &AppState, event_id: &EventId) -> EventEngagement {
        let reactions_count = state
            .timeline
            .reactions
            .get(event_id)
            .map(|r| r.len())
            .unwrap_or(0);

        let reposts_count = state
            .timeline
            .reposts
            .get(event_id)
            .map(|r| r.len())
            .unwrap_or(0);

        let zaps_count = state
            .timeline
            .zap_receipts
            .get(event_id)
            .map(|r| r.len())
            .unwrap_or(0);

        // Calculate zap amounts if available
        let total_zap_amount = state
            .timeline
            .zap_receipts
            .get(event_id)
            .map(|_zaps| {
                // TODO: Extract zap amounts from receipts
                // This would require parsing zap receipt content
                0
            })
            .unwrap_or(0);

        EventEngagement {
            reactions_count,
            reposts_count,
            zaps_count,
            total_zap_amount,
        }
    }

    /// Check if user can interact with timeline
    /// Pure function based on app state
    pub fn can_interact_with_timeline(state: &AppState) -> bool {
        !state.ui.is_composing() && !state.timeline.notes.is_empty()
    }

    /// Get display name for a public key
    /// Pure function with fallback to shortened key
    pub fn get_display_name(state: &AppState, pubkey: &PublicKey) -> String {
        state
            .user
            .profiles
            .get(pubkey)
            .map(|profile| profile.name())
            .unwrap_or_else(|| {
                crate::presentation::widgets::public_key::PublicKey::new(*pubkey).shortened()
            })
    }
}

impl Default for HomeData {
    fn default() -> Self {
        Self::new()
    }
}

/// Timeline statistics
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineStats {
    pub total_notes: usize,
    pub total_profiles: usize,
    pub total_reactions: usize,
    pub total_reposts: usize,
    pub total_zaps: usize,
}

/// Event engagement metrics
#[derive(Debug, Clone, PartialEq)]
pub struct EventEngagement {
    pub reactions_count: usize,
    pub reposts_count: usize,
    pub zaps_count: usize,
    pub total_zap_amount: u64, // in sats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::nostr::SortableEvent;
    use nostr_sdk::{EventBuilder, Keys, Metadata, Timestamp};
    use std::cmp::Reverse;

    fn create_test_state_with_notes(note_count: usize) -> AppState {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        // Add test notes
        for i in 0..note_count {
            let event = EventBuilder::text_note(format!("Test note {i}"))
                .sign_with_keys(&keys)
                .unwrap();

            let sortable = SortableEvent::new(event);
            state.timeline.notes.find_or_insert(Reverse(sortable));
        }

        // Add test profile
        let metadata = Metadata::new()
            .name("Test User")
            .display_name("Test Display");
        let profile =
            crate::domain::nostr::Profile::new(keys.public_key(), Timestamp::now(), metadata);
        state.user.profiles.insert(keys.public_key(), profile);

        state
    }

    fn create_test_event() -> Event {
        let keys = Keys::generate();
        EventBuilder::text_note("Test content")
            .sign_with_keys(&keys)
            .unwrap()
    }

    #[test]
    fn test_home_data_creation() {
        let home_data = HomeData::new();
        let default_home_data = HomeData;

        // Should be equivalent (stateless)
        assert_eq!(format!("{home_data:?}"), format!("{default_home_data:?}"));
    }

    #[test]
    fn test_generate_timeline_items() {
        let state = create_test_state_with_notes(3);
        let home_data = HomeData::new();

        let area = Rect::new(0, 0, 100, 50);
        let padding = Padding::new(1, 1, 1, 1);

        let items = home_data.generate_timeline_items(&state, area, padding);
        assert_eq!(items.len(), 3);

        // Each item should have a TextNote and height
        for (_text_note, height) in &items {
            assert!(height > &0);
        }
    }

    #[test]
    fn test_get_note_at_index() {
        let state = create_test_state_with_notes(5);

        // Valid index
        let note = HomeData::get_note_at_index(&state, 0);
        assert!(note.is_some());

        // Invalid index
        let note = HomeData::get_note_at_index(&state, 10);
        assert!(note.is_none());
    }

    #[test]
    fn test_get_selected_note() {
        let mut state = create_test_state_with_notes(3);

        // No selection
        let selected = HomeData::get_selected_note(&state);
        assert!(selected.is_none());

        // Valid selection
        state.timeline.selected_index = Some(1);
        let selected = HomeData::get_selected_note(&state);
        assert!(selected.is_some());

        // Invalid selection
        state.timeline.selected_index = Some(10);
        let selected = HomeData::get_selected_note(&state);
        assert!(selected.is_none());
    }

    #[test]
    fn test_calculate_timeline_stats() {
        let mut state = create_test_state_with_notes(5);

        // Add some reactions and reposts
        let event_id = state.timeline.notes.iter().next().unwrap().0.event.id;
        let reaction = create_test_event();
        let repost = create_test_event();

        let mut reaction_set = EventSet::new();
        reaction_set.insert(reaction);
        state.timeline.reactions.insert(event_id, reaction_set);

        let mut repost_set = EventSet::new();
        repost_set.insert(repost);
        state.timeline.reposts.insert(event_id, repost_set);

        let stats = HomeData::calculate_timeline_stats(&state);

        assert_eq!(stats.total_notes, 5);
        assert_eq!(stats.total_profiles, 1);
        assert_eq!(stats.total_reactions, 1);
        assert_eq!(stats.total_reposts, 1);
        assert_eq!(stats.total_zaps, 0);
    }

    #[test]
    fn test_get_event_engagement() {
        let mut state = create_test_state_with_notes(1);
        let event_id = state.timeline.notes.iter().next().unwrap().0.event.id;

        // Initially no engagement
        let engagement = HomeData::get_event_engagement(&state, &event_id);
        assert_eq!(engagement.reactions_count, 0);
        assert_eq!(engagement.reposts_count, 0);
        assert_eq!(engagement.zaps_count, 0);

        // Add some engagement
        let reaction1 = create_test_event();
        let reaction2 = create_test_event();
        let mut reaction_set = EventSet::new();
        reaction_set.insert(reaction1);
        reaction_set.insert(reaction2);
        state.timeline.reactions.insert(event_id, reaction_set);

        let engagement = HomeData::get_event_engagement(&state, &event_id);
        assert_eq!(engagement.reactions_count, 2);
    }

    #[test]
    fn test_can_interact_with_timeline() {
        let mut state = create_test_state_with_notes(1);

        // Can interact when input is not shown and notes exist
        assert!(HomeData::can_interact_with_timeline(&state));

        // Cannot interact when input is shown
        state.ui.current_mode = crate::core::state::ui::UiMode::Composing;
        assert!(!HomeData::can_interact_with_timeline(&state));

        // Cannot interact when no notes (even if input hidden)
        state.ui.current_mode = crate::core::state::ui::UiMode::Normal;
        state.timeline.notes.clear();
        assert!(!HomeData::can_interact_with_timeline(&state));
    }

    #[test]
    fn test_get_display_name() {
        let state = create_test_state_with_notes(1);
        let pubkey = state.user.current_user_pubkey;

        // Should return profile display name
        let display_name = HomeData::get_display_name(&state, &pubkey);
        assert_eq!(display_name, "Test Display");

        // Test with unknown pubkey - should return shortened key
        let unknown_keys = Keys::generate();
        let unknown_name = HomeData::get_display_name(&state, &unknown_keys.public_key());
        assert!(!unknown_name.is_empty());
    }

    #[test]
    fn test_has_profile_data() {
        let state_with_profile = create_test_state_with_notes(1);
        let keys = Keys::generate();
        let state_without_profile = AppState::new(keys.public_key());

        // Should have profile data for test user
        assert!(HomeData::has_profile_data(
            &state_with_profile,
            &state_with_profile.user.current_user_pubkey
        ));

        // Should not have profile data for unknown user
        let unknown_keys = Keys::generate();
        assert!(!HomeData::has_profile_data(
            &state_with_profile,
            &unknown_keys.public_key()
        ));

        // Empty state should not have any profiles
        assert!(!HomeData::has_profile_data(
            &state_without_profile,
            &keys.public_key()
        ));
    }

    #[test]
    fn test_get_timeline_authors() {
        let keys1 = Keys::generate();
        let keys2 = Keys::generate();
        let mut state = AppState::new(keys1.public_key());

        // Initially no authors
        let authors = HomeData::get_timeline_authors(&state);
        assert_eq!(authors.len(), 0);

        // Add notes from different authors
        let event1 = EventBuilder::text_note("Post 1")
            .sign_with_keys(&keys1)
            .unwrap();
        let event2 = EventBuilder::text_note("Post 2")
            .sign_with_keys(&keys2)
            .unwrap();
        let event3 = EventBuilder::text_note("Post 3")
            .sign_with_keys(&keys1)
            .unwrap(); // Same author as event1

        let sortable1 = crate::domain::nostr::SortableEvent::new(event1);
        let sortable2 = crate::domain::nostr::SortableEvent::new(event2);
        let sortable3 = crate::domain::nostr::SortableEvent::new(event3);

        state.timeline.notes.find_or_insert(Reverse(sortable1));
        state.timeline.notes.find_or_insert(Reverse(sortable2));
        state.timeline.notes.find_or_insert(Reverse(sortable3));

        // Should have 2 unique authors
        let authors = HomeData::get_timeline_authors(&state);
        assert_eq!(authors.len(), 2);
        assert!(authors.contains(&keys1.public_key()));
        assert!(authors.contains(&keys2.public_key()));
    }

    #[test]
    fn test_create_text_note() {
        let state = create_test_state_with_notes(1);
        let home_data = HomeData::new();
        let event = state.timeline.notes.iter().next().unwrap().0.event.clone();

        let area = Rect::new(0, 0, 100, 50);
        let padding = Padding::new(1, 1, 1, 1);

        let text_note = home_data.create_text_note(event, &state, area, padding);

        // TextNote should be created successfully
        // Detailed assertions would require TextNote internals access
        assert_eq!(text_note.area, area);
    }
}
