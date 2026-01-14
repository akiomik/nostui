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
pub struct EditorState {
    reply_to: Option<Event>,
    current_mode: UiMode,
}

impl EditorState {
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
        let state = EditorState {
            current_mode: UiMode::Composing,
            ..Default::default()
        };

        assert!(state.is_composing());
        assert!(!state.is_normal());
    }

    #[test]
    fn test_is_normal() {
        let state = EditorState::default();

        assert!(state.is_normal());
        assert!(!state.is_composing());
    }

    #[test]
    fn test_is_reply_none() {
        let state = EditorState::default();

        assert!(!state.is_reply());
    }

    #[test]
    fn test_is_reply_some() {
        let event = create_test_event();
        let state = EditorState {
            reply_to: Some(event),
            ..Default::default()
        };

        assert!(state.is_reply());
    }

    #[test]
    fn test_start_composing() {
        let mut state = EditorState::default();
        state.start_composing();

        assert!(state.is_composing());
        assert!(!state.is_reply());
    }

    #[test]
    fn test_start_reply() {
        let event = create_test_event();
        let mut state = EditorState::default();
        state.start_reply(event);

        assert!(state.is_composing());
        assert!(state.is_reply());
    }

    #[test]
    fn test_cancel_composing() {
        let mut state = EditorState::default();
        state.cancel_composing();

        assert!(!state.is_composing());
        assert!(!state.is_reply());
    }
}
