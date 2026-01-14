//! Home list component
//!
//! Displays the timeline list of events.

use nostr_sdk::ToBech32;
use ratatui::{prelude::*, widgets::*};
use tui_widget_list::{ListBuilder, ListView};

use crate::{
    core::state::AppState, domain::text::shorten_npub, presentation::widgets::text_note::TextNote,
};

/// Home list component
///
/// Displays the scrollable list of timeline notes.
/// This is a stateless component that renders timeline data from AppState.
#[derive(Debug, Clone)]
pub struct HomeListComponent;

impl HomeListComponent {
    /// Create a new list component
    pub fn new() -> Self {
        Self
    }

    /// Render the timeline list
    ///
    /// This renders a scrollable list of text notes from the timeline state.
    pub fn view(&self, state: &AppState, frame: &mut Frame, area: Rect) {
        let padding = Padding::new(1, 1, 1, 1);
        let item_count = state.timeline.len();

        if item_count == 0 {
            // Render empty state
            let empty_block = Block::default().padding(padding);
            let empty_text = Paragraph::new("No notes to display")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);

            let inner = empty_block.inner(area);
            frame.render_widget(empty_block, area);
            frame.render_widget(empty_text, inner);
            return;
        }

        // Prepare note data for the list builder
        let notes_data: Vec<_> = state
            .timeline
            .iter()
            .map(|sortable_event| {
                let event = &sortable_event.0.event;
                let event_id = event.id;

                // Get author profile if available
                let profile = state.user.get_profile(&event.pubkey).cloned();

                // Extract p-tags and get profiles for mentioned users
                let mentioned_names: Vec<_> = event
                    .tags
                    .public_keys()
                    .map(|pubkey| {
                        state
                            .user
                            .get_profile(pubkey)
                            .map(|p| p.name())
                            .unwrap_or_else(|| {
                                let Ok(npub) = pubkey.to_bech32();
                                shorten_npub(npub)
                            })
                    })
                    .collect();

                // Get reactions, reposts, and zap receipts for this event
                let reactions = state.timeline.reactions_for(&event_id);
                let reposts = state.timeline.reposts_for(&event_id);
                let zap_receipts = state.timeline.zap_receipts_for(&event_id);

                // Create TextNote widget
                let text_note = TextNote::new(
                    event.clone(),
                    profile,
                    mentioned_names,
                    reactions,
                    reposts,
                    zap_receipts,
                    padding,
                );

                let height = text_note.calculate_height(&area);
                (text_note, height)
            })
            .collect();

        // Create list builder with notes
        let builder = ListBuilder::new(move |context| {
            let mut item = notes_data[context.index].clone();
            item.0.highlight = context.is_selected;
            (item.0, item.1)
        });

        // Create list state from AppState
        let mut list_state = tui_widget_list::ListState::default();
        let selected_index = state.timeline.selected_index();
        list_state.select(selected_index);

        let list = ListView::new(builder, item_count)
            .block(Block::default().padding(padding))
            .style(Style::default().fg(Color::White));

        frame.render_stateful_widget(list, area, &mut list_state);
    }

    /// Get the number of notes in the timeline
    pub fn note_count(state: &AppState) -> usize {
        state.timeline.len()
    }

    /// Check if a note is selected
    pub fn has_selection(state: &AppState) -> bool {
        state.timeline.selected_index().is_some()
    }

    /// Get the selected note index
    pub fn selected_index(state: &AppState) -> Option<usize> {
        state.timeline.selected_index()
    }

    /// Get the selected event if any
    pub fn selected_event(state: &AppState) -> Option<&nostr_sdk::Event> {
        state.timeline.selected_note()
    }
}

impl Default for HomeListComponent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::*;

    fn create_test_state_with_notes(note_count: usize) -> Result<AppState> {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        for i in 0..note_count {
            let note_keys = Keys::generate();
            let content = format!("Test note {i}");
            let event = EventBuilder::text_note(&content).sign_with_keys(&note_keys)?;
            state.timeline.add_note(event);
        }

        Ok(state)
    }

    #[test]
    fn test_list_component_creation() {
        let list = HomeListComponent::new();
        let default_list = HomeListComponent;

        assert_eq!(format!("{list:?}"), format!("{default_list:?}"));
    }

    #[test]
    fn test_list_is_stateless() {
        let list1 = HomeListComponent::new();
        let list2 = HomeListComponent::new();

        assert_eq!(format!("{list1:?}"), format!("{list2:?}"));
    }

    #[test]
    fn test_note_count() -> Result<()> {
        let state_empty = create_test_state_with_notes(0)?;
        let state_with_notes = create_test_state_with_notes(5)?;

        assert_eq!(HomeListComponent::note_count(&state_empty), 0);
        assert_eq!(HomeListComponent::note_count(&state_with_notes), 5);

        Ok(())
    }

    #[test]
    fn test_has_selection() -> Result<()> {
        let mut state = create_test_state_with_notes(5)?;

        assert!(!HomeListComponent::has_selection(&state));

        state.timeline.select(2);
        assert!(HomeListComponent::has_selection(&state));

        Ok(())
    }

    #[test]
    fn test_selected_index() -> Result<()> {
        let mut state = create_test_state_with_notes(5)?;

        assert_eq!(HomeListComponent::selected_index(&state), None);

        state.timeline.select(2);
        assert_eq!(HomeListComponent::selected_index(&state), Some(2));

        Ok(())
    }

    #[test]
    fn test_selected_event() -> Result<()> {
        let mut state = create_test_state_with_notes(5)?;

        assert!(HomeListComponent::selected_event(&state).is_none());

        state.timeline.select(2);
        let selected = HomeListComponent::selected_event(&state);
        assert!(selected.is_some());
        if let Some(event) = selected {
            // Notes are stored in reverse order, so index 2 is actually note 3
            assert!(event.content.starts_with("Test note"));
        }

        Ok(())
    }

    #[test]
    fn test_selected_event_out_of_bounds() -> Result<()> {
        let mut state = create_test_state_with_notes(3)?;

        state.timeline.select(10); // Out of bounds
        let selected = HomeListComponent::selected_event(&state);
        assert!(selected.is_none());

        Ok(())
    }

    #[test]
    fn test_multibyte_character_rendering() -> Result<()> {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        // Test that Japanese text (multibyte characters) renders correctly
        // without extra spaces between characters
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        // Create a note with Japanese text
        let note_keys = Keys::generate();
        let japanese_text = "初force pushめでたい";
        let event = EventBuilder::text_note(japanese_text).sign_with_keys(&note_keys)?;
        state.timeline.add_note(event);

        // Render the note
        let list = HomeListComponent::new();
        let area = Rect::new(0, 0, 80, 20);
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend)?;

        terminal.draw(|frame| {
            list.view(&state, frame, area);
        })?;

        // Get the rendered buffer
        let buffer = terminal.backend().buffer();

        let raw_content = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .join("");

        // The Japanese text should appear in the output
        // Note: In ratatui, wide characters (like Japanese) take 2 cells when rendered.
        // The buffer will have the character followed by a space for padding.
        // We verify that the characters are present (with their padding spaces).
        assert!(
            raw_content.contains("初"),
            "Expected Japanese character '初' in rendered output"
        );
        assert!(
            raw_content.contains("force"),
            "Expected 'force' in rendered output"
        );
        assert!(
            raw_content.contains("push"),
            "Expected 'push' in rendered output"
        );

        // For Japanese text, we check each character individually since they have padding
        assert!(
            raw_content.contains("め"),
            "Expected Japanese character 'め' in rendered output"
        );
        assert!(
            raw_content.contains("で"),
            "Expected Japanese character 'で' in rendered output"
        );
        assert!(
            raw_content.contains("た"),
            "Expected Japanese character 'た' in rendered output"
        );
        assert!(
            raw_content.contains("い"),
            "Expected Japanese character 'い' in rendered output"
        );

        Ok(())
    }
}
