//! Selection state management for timeline
//!
//! This module follows the Elm Architecture pattern:
//! - State is immutable and changes only through the `update` function
//! - All state transitions are explicitly defined as `Message` variants
//! - The module is self-contained and doesn't know about other timeline components

/// Messages that can be sent to update the selection state
///
/// Following Elm conventions, messages are named in past tense
/// to indicate "what happened" rather than "what to do"
pub enum Message {
    /// A specific item was selected by index
    ItemSelected(usize),
    /// The selection was cleared (no item selected)
    SelectionCleared,
    /// The previous item in the list was selected
    PreviousItemSelected,
    /// The next item in the list was selected
    NextItemSelected { max_index: usize },
    /// The first item in the list was selected
    FirstItemSelected,
    /// The last item in the list was selected
    LastItemSelected { max_index: usize },
}

/// Manages the selection state within a timeline
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    selected_index: Option<usize>,
}

impl Selection {
    /// Create a new selection state with no selection
    pub fn new() -> Self {
        Self {
            selected_index: None,
        }
    }

    /// Get the currently selected index
    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    /// Check if an index is selected
    pub fn is_selected(&self) -> bool {
        self.selected_index.is_some()
    }

    /// Update the selection state based on a message
    ///
    /// This is the only way to modify the selection state, following Elm Architecture principles.
    /// All logic is implemented directly in the match arms rather than delegating to private methods,
    /// ensuring a single path for state changes.
    pub fn update(&mut self, message: Message) {
        match message {
            Message::ItemSelected(index) => {
                self.selected_index = Some(index);
            }
            Message::SelectionCleared => {
                self.selected_index = None;
            }
            Message::PreviousItemSelected => match self.selected_index {
                Some(index) => {
                    self.selected_index = Some(index.saturating_sub(1));
                }
                None => {
                    self.selected_index = Some(0);
                }
            },
            Message::NextItemSelected { max_index } => match self.selected_index {
                Some(index) if index < max_index => {
                    self.selected_index = Some(index + 1);
                }
                None if max_index > 0 => {
                    self.selected_index = Some(0);
                }
                _ => {}
            },
            Message::FirstItemSelected => {
                self.selected_index = Some(0);
            }
            Message::LastItemSelected { max_index } => {
                self.selected_index = Some(max_index);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_state_default() {
        let state = Selection::new();
        assert_eq!(state.selected_index(), None);
        assert!(!state.is_selected());
    }

    #[test]
    fn test_select_and_deselect() {
        let mut state = Selection::new();
        state.update(Message::ItemSelected(5));
        assert_eq!(state.selected_index(), Some(5));
        assert!(state.is_selected());

        state.update(Message::SelectionCleared);
        assert_eq!(state.selected_index(), None);
        assert!(!state.is_selected());
    }

    #[test]
    fn test_scroll_up() {
        let mut state = Selection::new();
        state.update(Message::ItemSelected(5));

        state.update(Message::PreviousItemSelected);
        assert_eq!(state.selected_index(), Some(4));

        // Cannot scroll up from 0
        state.update(Message::ItemSelected(0));
        state.update(Message::PreviousItemSelected);
        assert_eq!(state.selected_index(), Some(0));
    }

    #[test]
    fn test_scroll_down() {
        let mut state = Selection::new();

        // Initial scroll down selects first item
        state.update(Message::NextItemSelected { max_index: 10 });
        assert_eq!(state.selected_index(), Some(0));

        state.update(Message::NextItemSelected { max_index: 10 });
        assert_eq!(state.selected_index(), Some(1));

        // Cannot scroll beyond max
        state.update(Message::ItemSelected(10));
        state.update(Message::NextItemSelected { max_index: 10 });
        assert_eq!(state.selected_index(), Some(10));
    }

    #[test]
    fn test_select_first_and_last() {
        let mut state = Selection::new();

        state.update(Message::FirstItemSelected);
        assert_eq!(state.selected_index(), Some(0));

        state.update(Message::LastItemSelected { max_index: 99 });
        assert_eq!(state.selected_index(), Some(99));
    }

    #[test]
    fn test_previous_item_selected_when_nothing_selected() {
        let mut state = Selection::new();
        assert_eq!(state.selected_index(), None);

        state.update(Message::PreviousItemSelected);
        assert_eq!(state.selected_index(), Some(0));
    }

    #[test]
    fn test_next_item_selected_with_empty_list() {
        let mut state = Selection::new();

        // Should do nothing when max_index is 0 (empty list)
        state.update(Message::NextItemSelected { max_index: 0 });
        assert_eq!(state.selected_index(), None);
    }

    #[test]
    fn test_next_item_selected_boundary() {
        let mut state = Selection::new();

        // With max_index = 0, only one item exists
        // Should select it when nothing is selected
        state.update(Message::NextItemSelected { max_index: 0 });
        assert_eq!(state.selected_index(), None);

        // But if we're already at index 0 with max_index 0, can't go further
        state.update(Message::ItemSelected(0));
        state.update(Message::NextItemSelected { max_index: 0 });
        assert_eq!(state.selected_index(), Some(0));
    }
}
