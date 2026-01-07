//! Home input component
//!
//! Handles text input for composing new posts.

use crossterm::event::KeyEvent;
use ratatui::{prelude::*, widgets::*};
use tui_textarea::TextArea;

use crate::core::state::AppState;

/// Home input component
///
/// Displays and manages the text input area for composing posts.
/// This is a stateless component that syncs with AppState's textarea state.
#[derive(Debug)]
pub struct HomeInputComponent<'a> {
    /// Internal TextArea widget for rendering
    /// This is synced with AppState before rendering
    textarea: TextArea<'a>,
    /// Last synced content (for dirty checking)
    last_synced_content: String,
    /// Last synced cursor position (line, column)
    last_synced_cursor: (usize, usize),
}

impl<'a> HomeInputComponent<'a> {
    /// Create a new input component
    pub fn new() -> Self {
        Self {
            textarea: TextArea::default(),
            last_synced_content: String::new(),
            last_synced_cursor: (0, 0),
        }
    }

    /// Render the input area
    ///
    /// This syncs the internal TextArea with AppState and renders it.
    pub fn view(&mut self, state: &AppState, frame: &mut Frame, area: Rect) {
        if !state.ui.is_composing() {
            return;
        }

        // Clear the input area
        frame.render_widget(Clear, area);

        // Set block based on reply state
        let block = if let Some(reply_to) = &state.ui.reply_to {
            let name = self.get_reply_target_name(state, reply_to);
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Replying to {name}: Press ESC to close"))
        } else {
            Block::default()
                .borders(Borders::ALL)
                .title("New note: Press ESC to close")
        };

        self.textarea.set_block(block);
        frame.render_widget(&self.textarea, area);
    }

    /// Get the name of the reply target
    fn get_reply_target_name(&self, state: &AppState, reply_to: &nostr_sdk::Event) -> String {
        // Get author profile from the reply target event
        state
            .user
            .profiles
            .get(&reply_to.pubkey)
            .map(|profile| profile.name())
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Check if currently composing
    pub fn is_composing(state: &AppState) -> bool {
        state.ui.is_composing()
    }

    /// Get the current content
    pub fn content(state: &AppState) -> &str {
        &state.ui.textarea.content
    }

    /// Check if replying to a note
    pub fn is_replying(state: &AppState) -> bool {
        state.ui.reply_to.is_some()
    }

    /// Process key input directly on the internal TextArea
    ///
    /// This method updates the TextArea state immediately without going through
    /// the State → TextArea → State round-trip, improving input responsiveness.
    pub fn process_input(&mut self, key: KeyEvent) {
        use crossterm::event::Event;
        self.textarea.input(Event::Key(key));

        // Update cached state to keep sync logic working
        self.last_synced_content = self.textarea.lines().join("\n");
        let (line, col) = self.textarea.cursor();
        self.last_synced_cursor = (line, col);
    }

    /// Get the current content from the TextArea
    ///
    /// This should be called when you need to submit or save the content.
    pub fn get_content(&self) -> String {
        self.textarea.lines().join("\n")
    }

    /// Get the current cursor position
    pub fn get_cursor(&self) -> (usize, usize) {
        self.textarea.cursor()
    }

    /// Clear the TextArea content
    pub fn clear(&mut self) {
        self.textarea.select_all();
        self.textarea.delete_str(usize::MAX);
        self.last_synced_content.clear();
        self.last_synced_cursor = (0, 0);
    }
}

impl<'a> Default for HomeInputComponent<'a> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::*;

    fn create_test_state() -> AppState {
        let keys = Keys::generate();
        AppState::new(keys.public_key())
    }

    #[test]
    fn test_input_component_creation() {
        let _input = HomeInputComponent::new();
        // Component should be creatable
    }

    #[test]
    fn test_is_composing() {
        use crate::core::state::ui::UiMode;

        let mut state = create_test_state();
        assert!(!HomeInputComponent::is_composing(&state));

        // Manually set composing state
        state.ui.current_mode = UiMode::Composing;
        assert!(HomeInputComponent::is_composing(&state));
    }

    #[test]
    fn test_content() {
        let mut state = create_test_state();
        assert_eq!(HomeInputComponent::content(&state), "");

        state.ui.textarea.content = "test content".to_string();
        assert_eq!(HomeInputComponent::content(&state), "test content");
    }

    #[test]
    fn test_is_replying() -> Result<()> {
        let mut state = create_test_state();
        assert!(!HomeInputComponent::is_replying(&state));

        let keys = Keys::generate();
        let event = EventBuilder::text_note("test").sign_with_keys(&keys)?;
        state.ui.reply_to = Some(event);
        assert!(HomeInputComponent::is_replying(&state));

        Ok(())
    }
}
