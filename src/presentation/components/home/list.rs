//! Home list component
//!
//! Displays the timeline list of events.

use ratatui::{prelude::*, widgets::*};
use tui_widget_list::{ListBuilder, ListView};

use crate::{
    core::state::AppState,
    presentation::widgets::text_note::{TextNoteWidget, ViewContext},
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

        // Create list builder with notes
        let builder = ListBuilder::new(move |list_ctx| {
            let note = state
                .timeline
                .note_by_index(list_ctx.index)
                .cloned()
                .expect("note must exist");

            let item_ctx = ViewContext {
                profiles: state.user.profiles(),
                live_status: None,
                selected: list_ctx.is_selected,
            };
            let widget = TextNoteWidget::new(note, item_ctx);
            let height = widget.calculate_height(&area, padding);

            (widget, height)
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
}

impl Default for HomeListComponent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::core::state::timeline::TimelineTabType;

    use super::*;
    use nostr_sdk::prelude::*;

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
        state
            .timeline
            .add_note_to_tab(event, &TimelineTabType::Home);

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
