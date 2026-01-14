use nostr_sdk::prelude::*;

/// High-level UI mode for keybindings and view switching
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiMode {
    #[default]
    Normal,
    Composing,
}

/// UI-related state
#[derive(Debug, Clone, Default)]
pub struct UiState {
    reply_to: Option<Event>,
    current_mode: UiMode,
    /// Current tab index (0-based)
    current_tab_index: usize,
}

impl UiState {
    /// Returns true if the UI is in composing mode
    pub fn is_composing(&self) -> bool {
        self.current_mode == UiMode::Composing
    }

    /// Returns true if the UI is in normal mode
    pub fn is_normal(&self) -> bool {
        self.current_mode == UiMode::Normal
    }

    /// Returns true if currently composing a reply
    pub fn is_reply(&self) -> bool {
        self.reply_to.is_some()
    }

    /// Returns the event being replied to, if any
    pub fn reply_target(&self) -> Option<&Event> {
        self.reply_to.as_ref()
    }

    /// Returns the current UI mode
    pub fn current_mode(&self) -> UiMode {
        self.current_mode
    }

    /// Starts composing a new post (not a reply)
    pub fn start_composing(&mut self) {
        self.current_mode = UiMode::Composing;
        self.reply_to = None;
    }

    /// Starts composing a reply to the given event
    pub fn start_reply(&mut self, to: Event) {
        self.current_mode = UiMode::Composing;
        self.reply_to = Some(to);
    }

    /// Cancels composing and returns to normal mode
    pub fn cancel_composing(&mut self) {
        self.current_mode = UiMode::Normal;
        self.reply_to = None;
    }

    /// Returns the current tab index
    pub fn current_tab_index(&self) -> usize {
        self.current_tab_index
    }

    /// Sets the current tab index
    pub fn set_tab_index(&mut self, index: usize) {
        self.current_tab_index = index;
    }

    /// Select a specific tab by index
    /// Clamps the index to the valid range [0, max_tab_index]
    pub fn select_tab(&mut self, index: usize, max_tab_index: usize) {
        self.current_tab_index = index.min(max_tab_index);
    }

    /// Switch to the next tab (wraps around to 0 if at the end)
    pub fn next_tab(&mut self, max_tab_index: usize) {
        self.current_tab_index = if self.current_tab_index >= max_tab_index {
            0
        } else {
            self.current_tab_index + 1
        };
    }

    /// Switch to the previous tab (wraps around to max if at 0)
    pub fn prev_tab(&mut self, max_tab_index: usize) {
        self.current_tab_index = if self.current_tab_index == 0 {
            max_tab_index
        } else {
            self.current_tab_index - 1
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper function to create a test event
    fn create_test_event() -> Event {
        let keys = Keys::generate();
        EventBuilder::text_note("test note".to_owned())
            .sign_with_keys(&keys)
            .expect("Failed to sign event")
    }

    #[test]
    fn test_is_composing() {
        let state = UiState {
            current_mode: UiMode::Composing,
            ..Default::default()
        };

        assert!(state.is_composing());
        assert!(!state.is_normal());
    }

    #[test]
    fn test_is_normal() {
        let state = UiState::default();

        assert!(state.is_normal());
        assert!(!state.is_composing());
    }

    #[test]
    fn test_is_reply_none() {
        let state = UiState::default();

        assert!(!state.is_reply());
    }

    #[test]
    fn test_is_reply_some() {
        let event = create_test_event();
        let state = UiState {
            reply_to: Some(event),
            ..Default::default()
        };

        assert!(state.is_reply());
    }

    #[test]
    fn test_start_composing() {
        let mut state = UiState::default();
        state.start_composing();

        assert!(state.is_composing());
        assert!(!state.is_reply());
    }

    #[test]
    fn test_start_reply() {
        let event = create_test_event();
        let mut state = UiState::default();
        state.start_reply(event);

        assert!(state.is_composing());
        assert!(state.is_reply());
    }

    #[test]
    fn test_cancel_composing() {
        let mut state = UiState::default();
        state.cancel_composing();

        assert!(!state.is_composing());
        assert!(!state.is_reply());
    }

    #[test]
    fn test_default_tab_index() {
        let state = UiState::default();
        assert_eq!(state.current_tab_index(), 0);
    }

    #[test]
    fn test_set_tab_index() {
        let mut state = UiState::default();
        state.set_tab_index(1);
        assert_eq!(state.current_tab_index(), 1);
    }

    #[test]
    fn test_select_tab() {
        let mut state = UiState::default();

        // Select tab within range
        state.select_tab(0, 2);
        assert_eq!(state.current_tab_index(), 0);

        // Select tab at max
        state.select_tab(2, 2);
        assert_eq!(state.current_tab_index(), 2);

        // Select tab beyond max (should clamp)
        state.select_tab(5, 2);
        assert_eq!(state.current_tab_index(), 2);
    }

    #[test]
    fn test_next_tab() {
        let mut state = UiState::default();

        // Move to next tab
        state.next_tab(2);
        assert_eq!(state.current_tab_index(), 1);

        // Move to next tab again
        state.next_tab(2);
        assert_eq!(state.current_tab_index(), 2);

        // Wrap around to 0
        state.next_tab(2);
        assert_eq!(state.current_tab_index(), 0);
    }

    #[test]
    fn test_prev_tab() {
        let mut state = UiState::default();

        // Wrap around to max from 0
        state.prev_tab(2);
        assert_eq!(state.current_tab_index(), 2);

        // Move to previous tab
        state.prev_tab(2);
        assert_eq!(state.current_tab_index(), 1);

        // Move to previous tab again
        state.prev_tab(2);
        assert_eq!(state.current_tab_index(), 0);
    }

    #[test]
    fn test_next_tab_with_single_tab() {
        let mut state = UiState::default();

        // With only one tab (max_tab_index = 0), should stay at 0
        state.next_tab(0);
        assert_eq!(state.current_tab_index(), 0);
    }

    #[test]
    fn test_prev_tab_with_single_tab() {
        let mut state = UiState::default();

        // With only one tab (max_tab_index = 0), should stay at 0
        state.prev_tab(0);
        assert_eq!(state.current_tab_index(), 0);
    }
}
