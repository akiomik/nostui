use color_eyre::eyre::Result;
use ratatui::{prelude::*, widgets::*};
use tui_textarea::TextArea;

use crate::{state::AppState, text::shorten_hex, tui::Frame};

/// Elm-architecture compatible input component for Home timeline
/// This component handles pure input operations: text input, reply management, submission
/// TextArea state is managed externally in AppState
#[derive(Debug)]
pub struct ElmHomeInput<'a> {
    // We need to maintain a TextArea for rendering, but sync it with AppState
    textarea: TextArea<'a>,
}

impl<'a> ElmHomeInput<'a> {
    pub fn new() -> Self {
        Self {
            textarea: TextArea::default(),
        }
    }

    /// Render input area from AppState
    /// Synchronizes internal TextArea with AppState and renders
    pub fn draw(&mut self, state: &AppState, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        if !state.ui.show_input {
            return Ok(());
        }

        // Sync TextArea content with AppState
        self.sync_textarea_with_state(state);

        // Calculate input area (similar to original implementation)
        let mut input_area = area;
        input_area.height /= 2;
        input_area.y = input_area.height;
        input_area.height = input_area.height.saturating_sub(2);

        f.render_widget(Clear, input_area);

        // Set block based on reply state
        let block = if let Some(ref reply_to) = state.ui.reply_to {
            let name = self.get_reply_target_name(state, reply_to);
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Replying to {}: Press ESC to close", name))
        } else {
            Block::default()
                .borders(Borders::ALL)
                .title("New note: Press ESC to close")
        };

        self.textarea.set_block(block);
        f.render_widget(&self.textarea, input_area);

        Ok(())
    }

    /// Synchronize internal TextArea with AppState content
    /// This ensures the TextArea always reflects the current state
    fn sync_textarea_with_state(&mut self, state: &AppState) {
        let current_content = self.textarea.lines().join("\n");

        // Only update if content differs to avoid unnecessary operations
        if current_content != state.ui.input_content {
            // Clear current content
            self.textarea.select_all();
            self.textarea.delete_str(usize::MAX);

            // Set new content
            if !state.ui.input_content.is_empty() {
                // Split content into lines and insert each line
                let lines: Vec<&str> = state.ui.input_content.lines().collect();
                for (i, line) in lines.iter().enumerate() {
                    if i > 0 {
                        self.textarea.insert_newline();
                    }
                    self.textarea.insert_str(line);
                }
            }
        }
    }

    /// Get display name for reply target
    /// Pure function that extracts name from profile or falls back to shortened pubkey
    fn get_reply_target_name(&self, state: &AppState, reply_to: &nostr_sdk::Event) -> String {
        state
            .user
            .profiles
            .get(&reply_to.pubkey)
            .map(|profile| profile.name())
            .unwrap_or_else(|| shorten_hex(&reply_to.pubkey.to_string()))
    }

    /// Process raw key input and convert to content update
    /// This is the bridge between TextArea input and Elm state management
    pub fn process_key_input(&mut self, key: crossterm::event::KeyEvent) -> Option<String> {
        // Apply key to internal TextArea
        self.textarea.input(crossterm::event::Event::Key(key));

        // Return the new content for AppState update
        Some(self.textarea.lines().join("\n"))
    }

    /// Calculate if submit is possible
    /// Pure function to determine if current state allows submission
    pub fn can_submit(state: &AppState) -> bool {
        state.ui.show_input && !state.ui.input_content.trim().is_empty()
    }

    /// Get submit data for creating a note
    /// Pure function that extracts all data needed for submission
    pub fn get_submit_data(state: &AppState) -> Option<SubmitData> {
        if !Self::can_submit(state) {
            return None;
        }

        let content = state.ui.input_content.clone();
        let tags = if let Some(ref reply_to) = state.ui.reply_to {
            // Use the same reply tag building logic as original
            crate::nostr::nip10::ReplyTagsBuilder::build(reply_to.clone())
        } else {
            vec![]
        };

        Some(SubmitData { content, tags })
    }

    /// Get current input statistics
    /// Pure function for UI display (character count, line count, etc.)
    pub fn get_input_stats(state: &AppState) -> InputStats {
        let content = &state.ui.input_content;
        let char_count = content.chars().count();
        let line_count = content.lines().count().max(1); // At least 1 line
        let word_count = content.split_whitespace().count();
        let is_empty = content.trim().is_empty();

        InputStats {
            char_count,
            line_count,
            word_count,
            is_empty,
        }
    }

    /// Check if input area is active
    /// Pure function to determine if input should capture key events
    pub fn is_input_active(state: &AppState) -> bool {
        state.ui.show_input
    }

    /// Get input mode description
    /// Pure function that returns user-friendly description of current input mode
    pub fn get_input_mode_description(state: &AppState) -> String {
        if !state.ui.show_input {
            "Navigation mode".to_string()
        } else if state.ui.reply_to.is_some() {
            "Reply mode".to_string()
        } else {
            "Compose mode".to_string()
        }
    }
}

impl<'a> Default for ElmHomeInput<'a> {
    fn default() -> Self {
        Self::new()
    }
}

/// Data required for submitting a note
#[derive(Debug, Clone, PartialEq)]
pub struct SubmitData {
    pub content: String,
    pub tags: Vec<nostr_sdk::Tag>,
}

/// Statistics about current input content
#[derive(Debug, Clone, PartialEq)]
pub struct InputStats {
    pub char_count: usize,
    pub line_count: usize,
    pub word_count: usize,
    pub is_empty: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::*;

    fn create_test_state_with_input() -> AppState {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());
        state.ui.show_input = true;
        state.ui.input_content = "Test content".to_string();
        state
    }

    fn create_test_event() -> Event {
        let keys = Keys::generate();
        EventBuilder::text_note("Test content")
            .sign_with_keys(&keys)
            .unwrap()
    }

    #[test]
    fn test_elm_home_input_creation() {
        let input = ElmHomeInput::new();
        let default_input = ElmHomeInput::default();

        // Should be creatable
        assert!(input.textarea.is_empty());
        assert!(default_input.textarea.is_empty());
    }

    #[test]
    fn test_can_submit() {
        let mut state = AppState::new(Keys::generate().public_key());

        // Cannot submit when input not shown
        assert!(!ElmHomeInput::can_submit(&state));

        // Cannot submit when input shown but empty
        state.ui.show_input = true;
        assert!(!ElmHomeInput::can_submit(&state));

        // Cannot submit with only whitespace
        state.ui.input_content = "   \n  \t  ".to_string();
        assert!(!ElmHomeInput::can_submit(&state));

        // Can submit with actual content
        state.ui.input_content = "Hello, Nostr!".to_string();
        assert!(ElmHomeInput::can_submit(&state));
    }

    #[test]
    fn test_get_submit_data() {
        let mut state = create_test_state_with_input();

        // Basic submission (new note)
        state.ui.input_content = "Hello, Nostr!".to_string();
        let submit_data = ElmHomeInput::get_submit_data(&state);
        assert!(submit_data.is_some());
        let data = submit_data.unwrap();
        assert_eq!(data.content, "Hello, Nostr!");
        assert!(data.tags.is_empty()); // No reply tags

        // Reply submission
        state.ui.reply_to = Some(create_test_event());
        let submit_data = ElmHomeInput::get_submit_data(&state);
        assert!(submit_data.is_some());
        let data = submit_data.unwrap();
        assert!(!data.tags.is_empty()); // Should have reply tags

        // Cannot submit when input hidden
        state.ui.show_input = false;
        let submit_data = ElmHomeInput::get_submit_data(&state);
        assert!(submit_data.is_none());
    }

    #[test]
    fn test_get_input_stats() {
        let mut state = AppState::new(Keys::generate().public_key());

        // Empty content
        let stats = ElmHomeInput::get_input_stats(&state);
        assert_eq!(stats.char_count, 0);
        assert_eq!(stats.line_count, 1); // At least 1 line
        assert_eq!(stats.word_count, 0);
        assert!(stats.is_empty);

        // Simple content
        state.ui.input_content = "Hello, world!".to_string();
        let stats = ElmHomeInput::get_input_stats(&state);
        assert_eq!(stats.char_count, 13);
        assert_eq!(stats.line_count, 1);
        assert_eq!(stats.word_count, 2);
        assert!(!stats.is_empty);

        // Multi-line content
        state.ui.input_content = "Line 1\nLine 2\nLine 3".to_string();
        let stats = ElmHomeInput::get_input_stats(&state);
        assert_eq!(stats.line_count, 3);
        assert_eq!(stats.word_count, 6);

        // Whitespace only
        state.ui.input_content = "   \n  \t  ".to_string();
        let stats = ElmHomeInput::get_input_stats(&state);
        assert!(stats.is_empty); // Trimmed empty
        assert!(stats.char_count > 0); // But has characters
    }

    #[test]
    fn test_is_input_active() {
        let mut state = AppState::new(Keys::generate().public_key());

        // Initially not active
        assert!(!ElmHomeInput::is_input_active(&state));

        // Active when input shown
        state.ui.show_input = true;
        assert!(ElmHomeInput::is_input_active(&state));

        // Not active when hidden again
        state.ui.show_input = false;
        assert!(!ElmHomeInput::is_input_active(&state));
    }

    #[test]
    fn test_get_input_mode_description() {
        let mut state = AppState::new(Keys::generate().public_key());

        // Navigation mode
        assert_eq!(
            ElmHomeInput::get_input_mode_description(&state),
            "Navigation mode"
        );

        // Compose mode
        state.ui.show_input = true;
        assert_eq!(
            ElmHomeInput::get_input_mode_description(&state),
            "Compose mode"
        );

        // Reply mode
        state.ui.reply_to = Some(create_test_event());
        assert_eq!(
            ElmHomeInput::get_input_mode_description(&state),
            "Reply mode"
        );
    }

    #[test]
    fn test_submit_data_equality() {
        let data1 = SubmitData {
            content: "Hello".to_string(),
            tags: vec![],
        };
        let data2 = SubmitData {
            content: "Hello".to_string(),
            tags: vec![],
        };
        let data3 = SubmitData {
            content: "World".to_string(),
            tags: vec![],
        };

        assert_eq!(data1, data2);
        assert_ne!(data1, data3);
    }

    #[test]
    fn test_input_stats_edge_cases() {
        let mut state = AppState::new(Keys::generate().public_key());

        // Unicode content
        state.ui.input_content = "こんにちは世界！".to_string();
        let stats = ElmHomeInput::get_input_stats(&state);
        assert_eq!(stats.char_count, 8); // Unicode characters
        assert!(!stats.is_empty);

        // Emoji content
        state.ui.input_content = "🚀🌟💫".to_string();
        let stats = ElmHomeInput::get_input_stats(&state);
        assert_eq!(stats.char_count, 3); // Emoji count
        assert_eq!(stats.word_count, 1); // Emojis as one word
    }
}
