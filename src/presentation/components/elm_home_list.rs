use color_eyre::eyre::Result;
use ratatui::{prelude::*, widgets::*};
use tui_widget_list::{ListBuilder, ListView};

use crate::{
    core::state::AppState, infrastructure::tui::Frame,
    presentation::components::elm_home_data::ElmHomeData,
};

/// Elm-architecture compatible list component for Home timeline
/// This component handles pure UI operations: scrolling, selection, and list rendering
/// No internal state management - all UI state comes from AppState
#[derive(Debug, Clone)]
pub struct ElmHomeList {
    data: ElmHomeData,
}

impl ElmHomeList {
    pub fn new() -> Self {
        Self {
            data: ElmHomeData::new(),
        }
    }

    /// Render timeline list from AppState
    /// Pure function that renders the scrollable timeline
    pub fn draw(&self, state: &AppState, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let padding = Padding::new(1, 1, 1, 3);

        // Generate timeline items using ElmHomeData
        let items = self.data.generate_timeline_items(state, area, padding);
        let item_count = items.len();

        if item_count == 0 {
            // Render empty state
            let empty_block = Block::default()
                .title("Timeline")
                .padding(padding)
                .borders(Borders::ALL);
            let empty_text = Paragraph::new("No notes to display")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);

            let inner = empty_block.inner(area);
            f.render_widget(empty_block, area);
            f.render_widget(empty_text, inner);
            return Ok(());
        }

        // Create list builder with items
        let builder = ListBuilder::new(move |context| {
            let mut item = items[context.index].clone();
            item.0.highlight = context.is_selected;
            (item.0, item.1)
        });

        // Create list state from AppState
        let mut list_state = self.create_list_state_from_app_state(state);

        let list = ListView::new(builder, item_count)
            .block(Block::default().title("Timeline").padding(padding))
            .style(Style::default().fg(Color::White));

        f.render_stateful_widget(list, area, &mut list_state);

        Ok(())
    }

    /// Convert AppState UI selection to tui_widget_list::ListState
    /// Pure function that transforms Elm state to widget state
    fn create_list_state_from_app_state(&self, state: &AppState) -> tui_widget_list::ListState {
        let mut list_state = tui_widget_list::ListState::default();
        list_state.select(state.timeline.selected_index);
        list_state
    }

    /// Calculate valid scroll position for timeline
    /// Pure function that ensures scroll position is within bounds
    pub fn calculate_valid_scroll_position(
        current_index: Option<usize>,
        timeline_len: usize,
    ) -> Option<usize> {
        match current_index {
            None => None,
            Some(_index) if timeline_len == 0 => None,
            Some(index) if index >= timeline_len => {
                if timeline_len > 0 {
                    Some(timeline_len - 1)
                } else {
                    None
                }
            }
            Some(index) => Some(index),
        }
    }

    /// Get the next scroll position for scrolling up
    /// Pure function for scroll up operation
    pub fn scroll_up_position(current_index: Option<usize>, timeline_len: usize) -> Option<usize> {
        if timeline_len == 0 {
            return None;
        }

        match current_index {
            None => Some(0),        // Start from top if nothing selected
            Some(0) => Some(0),     // Already at top
            Some(i) => Some(i - 1), // Move up one position
        }
    }

    /// Get the next scroll position for scrolling down
    /// Pure function for scroll down operation
    pub fn scroll_down_position(
        current_index: Option<usize>,
        timeline_len: usize,
    ) -> Option<usize> {
        if timeline_len == 0 {
            return None;
        }

        let max_index = timeline_len - 1;
        match current_index {
            None => Some(0),                              // Start from top if nothing selected
            Some(i) if i >= max_index => Some(max_index), // Already at bottom
            Some(i) => Some(i + 1),                       // Move down one position
        }
    }

    /// Get position for scroll to top
    /// Pure function for scroll to top operation
    pub fn scroll_to_top_position(timeline_len: usize) -> Option<usize> {
        if timeline_len > 0 {
            Some(0)
        } else {
            None
        }
    }

    /// Get position for scroll to bottom
    /// Pure function for scroll to bottom operation
    pub fn scroll_to_bottom_position(timeline_len: usize) -> Option<usize> {
        if timeline_len > 0 {
            Some(timeline_len - 1)
        } else {
            None
        }
    }

    /// Check if timeline is scrollable
    /// Pure function to determine if scrolling operations are valid
    pub fn is_scrollable(state: &AppState) -> bool {
        !state.ui.show_input && !state.timeline.notes.is_empty()
    }

    /// Get currently selected item information
    /// Pure function that extracts selection info for UI display
    pub fn get_selection_info(state: &AppState) -> SelectionInfo {
        let timeline_len = state.timeline.notes.len();
        let selected_index = state.timeline.selected_index;

        SelectionInfo {
            selected_index,
            timeline_length: timeline_len,
            has_selection: selected_index.is_some(),
            is_at_top: selected_index == Some(0),
            is_at_bottom: timeline_len > 0 && selected_index == Some(timeline_len - 1),
        }
    }
}

impl Default for ElmHomeList {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about current selection state
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionInfo {
    pub selected_index: Option<usize>,
    pub timeline_length: usize,
    pub has_selection: bool,
    pub is_at_top: bool,
    pub is_at_bottom: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::*;

    fn create_test_state_with_timeline(note_count: usize) -> AppState {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        // Add test notes
        for i in 0..note_count {
            let event = EventBuilder::text_note(format!("Test note {}", i))
                .sign_with_keys(&keys)
                .unwrap();

            let sortable = crate::domain::nostr::SortableEvent::new(event);
            state
                .timeline
                .notes
                .find_or_insert(std::cmp::Reverse(sortable));
        }

        state
    }

    #[test]
    fn test_elm_home_list_creation() {
        let list = ElmHomeList::new();
        let default_list = ElmHomeList::default();

        // Should be equivalent (stateless)
        assert_eq!(format!("{:?}", list), format!("{:?}", default_list));
    }

    #[test]
    fn test_calculate_valid_scroll_position() {
        // Empty timeline
        assert_eq!(
            ElmHomeList::calculate_valid_scroll_position(Some(0), 0),
            None
        );
        assert_eq!(ElmHomeList::calculate_valid_scroll_position(None, 0), None);

        // Valid positions
        assert_eq!(
            ElmHomeList::calculate_valid_scroll_position(Some(0), 5),
            Some(0)
        );
        assert_eq!(
            ElmHomeList::calculate_valid_scroll_position(Some(4), 5),
            Some(4)
        );
        assert_eq!(ElmHomeList::calculate_valid_scroll_position(None, 5), None);

        // Out of bounds
        assert_eq!(
            ElmHomeList::calculate_valid_scroll_position(Some(10), 5),
            Some(4)
        );
        assert_eq!(
            ElmHomeList::calculate_valid_scroll_position(Some(5), 5),
            Some(4)
        );
    }

    #[test]
    fn test_scroll_up_position() {
        // Empty timeline
        assert_eq!(ElmHomeList::scroll_up_position(None, 0), None);

        // Normal cases
        assert_eq!(ElmHomeList::scroll_up_position(None, 5), Some(0));
        assert_eq!(ElmHomeList::scroll_up_position(Some(0), 5), Some(0));
        assert_eq!(ElmHomeList::scroll_up_position(Some(3), 5), Some(2));
    }

    #[test]
    fn test_scroll_down_position() {
        // Empty timeline
        assert_eq!(ElmHomeList::scroll_down_position(None, 0), None);

        // Normal cases
        assert_eq!(ElmHomeList::scroll_down_position(None, 5), Some(0));
        assert_eq!(ElmHomeList::scroll_down_position(Some(0), 5), Some(1));
        assert_eq!(ElmHomeList::scroll_down_position(Some(4), 5), Some(4)); // At bottom
    }

    #[test]
    fn test_scroll_to_positions() {
        // Empty timeline
        assert_eq!(ElmHomeList::scroll_to_top_position(0), None);
        assert_eq!(ElmHomeList::scroll_to_bottom_position(0), None);

        // Normal timeline
        assert_eq!(ElmHomeList::scroll_to_top_position(5), Some(0));
        assert_eq!(ElmHomeList::scroll_to_bottom_position(5), Some(4));
    }

    #[test]
    fn test_is_scrollable() {
        let mut state = create_test_state_with_timeline(5);

        // Normal state - scrollable
        assert!(ElmHomeList::is_scrollable(&state));

        // Input shown - not scrollable
        state.ui.show_input = true;
        assert!(!ElmHomeList::is_scrollable(&state));

        // Empty timeline - not scrollable
        state.ui.show_input = false;
        state.timeline.notes.clear();
        assert!(!ElmHomeList::is_scrollable(&state));
    }

    #[test]
    fn test_get_selection_info() {
        let mut state = create_test_state_with_timeline(5);

        // No selection
        let info = ElmHomeList::get_selection_info(&state);
        assert_eq!(info.selected_index, None);
        assert_eq!(info.timeline_length, 5);
        assert!(!info.has_selection);
        assert!(!info.is_at_top);
        assert!(!info.is_at_bottom);

        // Select first item
        state.timeline.selected_index = Some(0);
        let info = ElmHomeList::get_selection_info(&state);
        assert_eq!(info.selected_index, Some(0));
        assert!(info.has_selection);
        assert!(info.is_at_top);
        assert!(!info.is_at_bottom);

        // Select last item
        state.timeline.selected_index = Some(4);
        let info = ElmHomeList::get_selection_info(&state);
        assert_eq!(info.selected_index, Some(4));
        assert!(info.has_selection);
        assert!(!info.is_at_top);
        assert!(info.is_at_bottom);

        // Select middle item
        state.timeline.selected_index = Some(2);
        let info = ElmHomeList::get_selection_info(&state);
        assert!(!info.is_at_top);
        assert!(!info.is_at_bottom);
    }

    #[test]
    fn test_empty_timeline_selection_info() {
        let state = create_test_state_with_timeline(0);
        let info = ElmHomeList::get_selection_info(&state);

        assert_eq!(info.timeline_length, 0);
        assert!(!info.has_selection);
        assert!(!info.is_at_top);
        assert!(!info.is_at_bottom);
    }
}
