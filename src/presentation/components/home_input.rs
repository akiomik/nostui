use color_eyre::eyre::Result;
use crossterm::event::{Event, KeyEvent};
use ratatui::{prelude::*, widgets::*};
use tui_textarea::{CursorMove, TextArea};

use crate::{
    core::state::{AppState, UiState},
    domain::{
        text::shorten_hex,
        ui::{CursorPosition, TextSelection},
    },
    infrastructure::tui::Frame,
};

/// Complete state representation of a TextArea component
/// This struct encapsulates all mutable state that needs to be
/// preserved across TextArea recreation in the stateless approach
#[derive(Debug, Clone, PartialEq)]
pub struct TextAreaState {
    /// The complete text content
    pub content: String,
    /// Current cursor position within the text
    pub cursor_position: CursorPosition,
    /// Active text selection range, if any
    pub selection: Option<TextSelection>,
}

impl TextAreaState {
    /// Create new TextAreaState
    pub fn new(
        content: String,
        cursor_position: CursorPosition,
        selection: Option<TextSelection>,
    ) -> Self {
        Self {
            content,
            cursor_position,
            selection,
        }
    }

    /// Create TextAreaState from AppState's UI state
    pub fn from_ui_state(ui_state: &UiState) -> Self {
        Self::new(
            ui_state.input_content.clone(),
            ui_state.cursor_position,
            ui_state.selection.clone(),
        )
    }

    /// Apply this TextAreaState to AppState's UI state
    pub fn apply_to_ui_state(&self, ui_state: &mut UiState) {
        ui_state.input_content = self.content.clone();
        ui_state.cursor_position = self.cursor_position;
        ui_state.selection = self.selection.clone();
    }

    /// Create empty TextAreaState
    pub fn empty() -> Self {
        Self::new(String::new(), CursorPosition { line: 0, column: 0 }, None)
    }
}

/// Elm-architecture compatible input component for Home timeline
/// This component handles pure input operations: text input, reply management, submission
/// TextArea state is managed externally in AppState
#[derive(Debug)]
pub struct HomeInput<'a> {
    // We need to maintain a TextArea for rendering, but sync it with AppState
    textarea: TextArea<'a>,
}

impl<'a> HomeInput<'a> {
    pub fn new() -> Self {
        let textarea = TextArea::default();

        // Ensure proper key bindings are set for navigation
        // TextArea should have default key bindings, but let's make sure

        Self { textarea }
    }

    /// Render input area from AppState
    /// Synchronizes internal TextArea with AppState and renders
    pub fn draw(&mut self, state: &AppState, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        if !state.ui.is_composing() {
            return Ok(());
        }

        // Sync TextArea content with AppState
        self.sync_textarea_with_state(state);

        // No side effects in draw: do not consume or mutate input state here.
        // TODO(architecture): pending_navigation_key is legacy. Route all navigation keys via Translator→update
        // and remove this field in a follow-up cleanup.

        // Calculate input area like the original implementation (home.rs:265-270)
        let mut input_area = area;
        input_area.height = input_area.height.saturating_sub(2); // Add some margin like original
        f.render_widget(Clear, input_area);

        // Set block based on reply state
        let block = if let Some(ref reply_to) = state.ui.reply_to {
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
        f.render_widget(&self.textarea, input_area);

        Ok(())
    }

    /// Synchronize internal TextArea with AppState content and cursor position
    /// This ensures the TextArea always reflects the current state
    pub fn sync_textarea_with_state(&mut self, state: &AppState) {
        let current_content = self.textarea.lines().join("\n");
        let current_cursor = self.get_cursor_position();

        // Update content if it differs
        if current_content != state.ui.input_content {
            // Clear current content and replace with state content
            // This is necessary to keep TextArea in sync with AppState
            self.textarea.select_all();
            self.textarea.delete_str(usize::MAX);

            // Set new content
            if !state.ui.input_content.is_empty() {
                self.textarea.insert_str(&state.ui.input_content);
                log::debug!(
                    "HomeInput::sync_textarea_with_state: Inserted content, cursor now at: {:?}",
                    self.textarea.cursor()
                );
            }
        }

        // Update cursor position if it differs
        // Re-enabled after pending_keys approach fixed the cursor sync issue
        if current_cursor != state.ui.cursor_position {
            self.set_cursor_position(&state.ui.cursor_position);
            log::debug!(
                "HomeInput::sync_textarea_with_state: Updated cursor to {:?}",
                state.ui.cursor_position
            );
        }

        // Update selection if present
        self.set_selection(&state.ui.selection);

        if current_content == state.ui.input_content && current_cursor == state.ui.cursor_position {
            log::debug!(
                "HomeInput::sync_textarea_with_state: Content and cursor are the same, no update needed"
            );
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

    /// Process raw key input and convert to content update with cursor position
    /// This is the bridge between TextArea input and Elm state management
    pub fn process_key_input(&mut self, key: KeyEvent) -> Option<String> {
        // Let TextArea handle ALL key inputs (including Enter, arrows, Ctrl+A, etc.)
        // TextArea has built-in support for navigation and editing
        self.textarea.input(Event::Key(key));

        let new_content = self.textarea.lines().join("\n");

        // Return the new content for AppState update
        Some(new_content)
    }

    /// Process raw key input and return complete TextArea state
    /// Enhanced version that provides complete state information
    pub fn process_key_input_with_cursor(&mut self, key: KeyEvent) -> TextAreaState {
        // Let TextArea handle the key input
        self.textarea.input(Event::Key(key));

        let new_content = self.textarea.lines().join("\n");
        let cursor_pos = self.get_cursor_position();
        let selection = self.get_selection();

        TextAreaState::new(new_content, cursor_pos, selection)
    }

    /// Get current cursor position from TextArea
    pub fn get_cursor_position(&self) -> CursorPosition {
        let (row, col) = self.textarea.cursor();
        CursorPosition {
            line: row,
            column: col,
        }
    }

    /// Get current selection from TextArea
    pub fn get_selection(&self) -> Option<TextSelection> {
        Self::extract_selection(&self.textarea)
    }

    /// Set cursor position in TextArea from AppState
    pub fn set_cursor_position(&mut self, pos: &CursorPosition) {
        // TextArea's move_cursor method allows setting cursor position
        self.textarea.move_cursor(tui_textarea::CursorMove::Jump(
            pos.line as u16,
            pos.column as u16,
        ));
    }

    /// Apply selection to TextArea from AppState
    pub fn set_selection(&mut self, selection: &Option<TextSelection>) {
        if let Some(selection) = selection {
            Self::restore_selection(&mut self.textarea, selection);
        } else {
            // Cancel selection if None
            self.textarea.cancel_selection();
        }
    }

    /// Calculate if submit is possible
    /// Pure function to determine if current state allows submission
    pub fn can_submit(state: &AppState) -> bool {
        state.ui.can_submit_input()
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
        state.ui.is_composing()
    }

    /// Get input mode description
    /// Pure function that returns user-friendly description of current input mode
    pub fn get_input_mode_description(state: &AppState) -> String {
        if !state.ui.is_composing() {
            "Navigation mode".to_string()
        } else if state.ui.reply_to.is_some() {
            "Reply mode".to_string()
        } else {
            "Compose mode".to_string()
        }
    }

    /// Process pending keys from AppState using stateless TextArea
    /// Returns new TextAreaState without modifying the input state
    pub fn process_pending_keys(state: &mut AppState) -> TextAreaState {
        // Create temporary TextArea for processing
        let mut textarea = TextArea::default();

        // Restore TextArea state from AppState
        Self::restore_textarea_from_state(&mut textarea, state);

        // Apply all pending keys sequentially to preserve state continuity
        for key in state.ui.pending_input_keys.drain(..) {
            textarea.input(Event::Key(key));
        }

        // Extract final state and return (pure function)
        let content = textarea.lines().join("\n");
        let cursor = Self::extract_cursor_position(&textarea);
        let selection = Self::extract_selection(&textarea);

        TextAreaState::new(content, cursor, selection)
    }

    /// Restore TextArea state from AppState
    fn restore_textarea_from_state(textarea: &mut TextArea, state: &AppState) {
        // Restore content if present
        if !state.ui.input_content.is_empty() {
            textarea.insert_str(&state.ui.input_content);
        }

        // Restore cursor position directly from AppState (single source of truth)
        // AppState cursor position should always be valid as it's maintained by the update cycle
        textarea.move_cursor(CursorMove::Jump(
            state.ui.cursor_position.line as u16,
            state.ui.cursor_position.column as u16,
        ));

        // Restore selection range if present
        if let Some(selection) = &state.ui.selection {
            Self::restore_selection(textarea, selection);
        }
    }

    /// Extract cursor position from TextArea
    fn extract_cursor_position(textarea: &tui_textarea::TextArea) -> CursorPosition {
        let (row, col) = textarea.cursor();
        CursorPosition {
            line: row,
            column: col,
        }
    }

    /// Extract selection from TextArea
    fn extract_selection(textarea: &tui_textarea::TextArea) -> Option<TextSelection> {
        textarea
            .selection_range()
            .map(
                |((start_row, start_col), (end_row, end_col))| TextSelection {
                    start: CursorPosition {
                        line: start_row,
                        column: start_col,
                    },
                    end: CursorPosition {
                        line: end_row,
                        column: end_col,
                    },
                },
            )
    }

    /// Restore selection range to TextArea from AppState
    fn restore_selection(textarea: &mut TextArea, selection: &TextSelection) {
        // First, position cursor at selection start
        textarea.move_cursor(CursorMove::Jump(
            selection.start.line as u16,
            selection.start.column as u16,
        ));

        // Start selection
        textarea.start_selection();

        // Move cursor to selection end
        textarea.move_cursor(CursorMove::Jump(
            selection.end.line as u16,
            selection.end.column as u16,
        ));
    }
}

impl<'a> Default for HomeInput<'a> {
    fn default() -> Self {
        Self::new()
    }
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
    use crate::core::state::ui::UiMode;

    use super::*;
    use nostr_sdk::prelude::{Event as NostrEvent, *};

    fn create_test_event() -> NostrEvent {
        let keys = Keys::generate();
        EventBuilder::text_note("Test content")
            .sign_with_keys(&keys)
            .unwrap()
    }

    #[test]
    fn test_home_input_creation() {
        let input = HomeInput::new();
        let default_input = HomeInput::default();

        // Should be creatable
        assert!(input.textarea.is_empty());
        assert!(default_input.textarea.is_empty());
    }

    #[test]
    fn test_can_submit() {
        let mut state = AppState::new(Keys::generate().public_key());

        // Cannot submit when input not shown
        assert!(!HomeInput::can_submit(&state));

        // Cannot submit when input shown but empty
        state.ui.current_mode = UiMode::Composing;
        assert!(!HomeInput::can_submit(&state));

        // Cannot submit with only whitespace
        state.ui.input_content = "   \n  \t  ".to_string();
        assert!(!HomeInput::can_submit(&state));

        // Can submit with actual content
        state.ui.input_content = "Hello, Nostr!".to_string();
        assert!(HomeInput::can_submit(&state));
    }

    #[test]
    fn test_get_input_stats() {
        let mut state = AppState::new(Keys::generate().public_key());

        // Empty content
        let stats = HomeInput::get_input_stats(&state);
        assert_eq!(stats.char_count, 0);
        assert_eq!(stats.line_count, 1); // At least 1 line
        assert_eq!(stats.word_count, 0);
        assert!(stats.is_empty);

        // Simple content
        state.ui.input_content = "Hello, world!".to_string();
        let stats = HomeInput::get_input_stats(&state);
        assert_eq!(stats.char_count, 13);
        assert_eq!(stats.line_count, 1);
        assert_eq!(stats.word_count, 2);
        assert!(!stats.is_empty);

        // Multi-line content
        state.ui.input_content = "Line 1\nLine 2\nLine 3".to_string();
        let stats = HomeInput::get_input_stats(&state);
        assert_eq!(stats.line_count, 3);
        assert_eq!(stats.word_count, 6);

        // Whitespace only
        state.ui.input_content = "   \n  \t  ".to_string();
        let stats = HomeInput::get_input_stats(&state);
        assert!(stats.is_empty); // Trimmed empty
        assert!(stats.char_count > 0); // But has characters
    }

    #[test]
    fn test_is_input_active() {
        let mut state = AppState::new(Keys::generate().public_key());

        // Initially not active
        assert!(!HomeInput::is_input_active(&state));

        // Active when input shown
        state.ui.current_mode = UiMode::Composing;
        assert!(HomeInput::is_input_active(&state));

        // Not active when hidden again
        state.ui.current_mode = UiMode::Normal;
        assert!(!HomeInput::is_input_active(&state));
    }

    #[test]
    fn test_get_input_mode_description() {
        let mut state = AppState::new(Keys::generate().public_key());

        // Navigation mode
        assert_eq!(
            HomeInput::get_input_mode_description(&state),
            "Navigation mode"
        );

        // Compose mode
        state.ui.current_mode = UiMode::Composing;
        assert_eq!(
            HomeInput::get_input_mode_description(&state),
            "Compose mode"
        );

        // Reply mode
        state.ui.reply_to = Some(create_test_event());
        assert_eq!(HomeInput::get_input_mode_description(&state), "Reply mode");
    }

    #[test]
    fn test_input_stats_edge_cases() {
        let mut state = AppState::new(Keys::generate().public_key());

        // Unicode content
        state.ui.input_content = "こんにちは世界！".to_string();
        let stats = HomeInput::get_input_stats(&state);
        assert_eq!(stats.char_count, 8); // Unicode characters
        assert!(!stats.is_empty);

        // Emoji content
        state.ui.input_content = "🚀🌟💫".to_string();
        let stats = HomeInput::get_input_stats(&state);
        assert_eq!(stats.char_count, 3); // Emoji count
        assert_eq!(stats.word_count, 1); // Emojis as one word
    }
}
