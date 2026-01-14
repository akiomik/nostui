//! Selection state management for timeline

/// Manages the selection state within a timeline
#[derive(Debug, Clone, Default)]
pub struct SelectionState {
    selected_index: Option<usize>,
}

impl SelectionState {
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

    /// Select a specific index
    pub fn select(&mut self, index: usize) {
        self.selected_index = Some(index);
    }

    /// Deselect the current selection
    pub fn deselect(&mut self) {
        self.selected_index = None;
    }

    /// Scroll up (decrement selection)
    pub fn scroll_up(&mut self) {
        if let Some(index) = self.selected_index {
            if index > 0 {
                self.selected_index = Some(index - 1);
            }
        }
    }

    /// Scroll down (increment selection)
    pub fn scroll_down(&mut self, max_index: usize) {
        match self.selected_index {
            Some(index) if index < max_index => {
                self.selected_index = Some(index + 1);
            }
            None if max_index > 0 => {
                self.selected_index = Some(0);
            }
            _ => {}
        }
    }

    /// Select the first item
    pub fn select_first(&mut self) {
        self.selected_index = Some(0);
    }

    /// Select the last item
    pub fn select_last(&mut self, max_index: usize) {
        self.selected_index = Some(max_index);
    }

    /// Check if an index is selected
    pub fn is_selected(&self) -> bool {
        self.selected_index.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_state_default() {
        let state = SelectionState::new();
        assert_eq!(state.selected_index(), None);
        assert!(!state.is_selected());
    }

    #[test]
    fn test_select_and_deselect() {
        let mut state = SelectionState::new();
        state.select(5);
        assert_eq!(state.selected_index(), Some(5));
        assert!(state.is_selected());

        state.deselect();
        assert_eq!(state.selected_index(), None);
        assert!(!state.is_selected());
    }

    #[test]
    fn test_scroll_up() {
        let mut state = SelectionState::new();
        state.select(5);

        state.scroll_up();
        assert_eq!(state.selected_index(), Some(4));

        // Cannot scroll up from 0
        state.select(0);
        state.scroll_up();
        assert_eq!(state.selected_index(), Some(0));
    }

    #[test]
    fn test_scroll_down() {
        let mut state = SelectionState::new();

        // Initial scroll down selects first item
        state.scroll_down(10);
        assert_eq!(state.selected_index(), Some(0));

        state.scroll_down(10);
        assert_eq!(state.selected_index(), Some(1));

        // Cannot scroll beyond max
        state.select(10);
        state.scroll_down(10);
        assert_eq!(state.selected_index(), Some(10));
    }

    #[test]
    fn test_select_first_and_last() {
        let mut state = SelectionState::new();

        state.select_first();
        assert_eq!(state.selected_index(), Some(0));

        state.select_last(99);
        assert_eq!(state.selected_index(), Some(99));
    }
}
