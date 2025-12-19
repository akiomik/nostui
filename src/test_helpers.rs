use crate::core::msg::ui::UiMsg;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nostr_sdk::prelude::*;

use crate::core::state::ui::{SubmitData, UiMode};
use crate::{
    core::msg::Msg, core::raw_msg::RawMsg, core::state::AppState,
    core::translator::translate_raw_to_domain, core::update::update,
    presentation::components::home_input::HomeInput,
};

/// Test helper for TextArea and HomeInput integration testing
/// Provides a fluent API for common test patterns and reduces boilerplate
pub struct TextAreaTestHelper<'a> {
    input: HomeInput<'a>,
    state: AppState,
}

impl<'a> TextAreaTestHelper<'a> {
    /// Create a new test helper with default state
    pub fn new() -> Self {
        let dummy_pubkey = Self::create_test_pubkey();
        let state = AppState::new(dummy_pubkey);
        let input = HomeInput::new();
        Self { input, state }
    }

    /// Create a test helper with specific input content
    pub fn with_content(content: &str) -> Self {
        let mut helper = Self::new();
        helper.set_content(content);
        helper
    }

    /// Create a test helper in input mode with content
    pub fn in_input_mode_with_content(content: &str) -> Self {
        let mut helper = Self::with_content(content);
        helper.activate_input();
        helper
    }

    /// Create a test helper in input mode (empty content)
    pub fn in_input_mode() -> Self {
        let mut helper = Self::new();
        helper.activate_input();
        helper
    }

    /// Set the input content and sync with textarea
    pub fn set_content(&mut self, content: &str) -> &mut Self {
        self.state.ui.textarea.content = content.to_string();
        self.sync_state();
        self
    }

    /// Activate input mode
    pub fn activate_input(&mut self) -> &mut Self {
        self.state.ui.current_mode = UiMode::Composing;
        self.sync_state();
        self
    }

    /// Deactivate input mode
    pub fn deactivate_input(&mut self) -> &mut Self {
        self.state.ui.current_mode = UiMode::Normal;
        self.sync_state();
        self
    }

    /// Type text character by character
    pub fn type_text(&mut self, text: &str) -> &mut Self {
        for ch in text.chars() {
            self.press_char(ch);
        }
        self
    }

    /// Press a character key
    pub fn press_char(&mut self, ch: char) -> &mut Self {
        let key_event = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
        self.press_key(key_event)
    }

    /// Press a key with modifiers
    pub fn press_key(&mut self, key: KeyEvent) -> &mut Self {
        // Use the translator to convert raw key to domain messages
        let messages = translate_raw_to_domain(RawMsg::Key(key), &self.state);

        // Process each message through the update cycle
        for msg in messages {
            let (new_state, _cmds) = update(msg, self.state.clone());
            self.state = new_state;
        }

        self.sync_state();
        self
    }

    /// Press navigation key (Ctrl+A, Ctrl+E, etc.)
    pub fn press_navigation_key(&mut self, key: KeyEvent) -> &mut Self {
        // Route navigation via the normal translator→update path
        let messages = translate_raw_to_domain(RawMsg::Key(key), &self.state);
        for msg in messages {
            let (new_state, _cmds) = update(msg, self.state.clone());
            self.state = new_state;
        }
        self.sync_state();
        self
    }

    /// Press Ctrl+A (go to line start)
    pub fn ctrl_a(&mut self) -> &mut Self {
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        self.press_navigation_key(key)
    }

    /// Press Ctrl+E (go to line end)
    pub fn ctrl_e(&mut self) -> &mut Self {
        let key = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL);
        self.press_navigation_key(key)
    }

    /// Press Enter key
    pub fn press_enter(&mut self) -> &mut Self {
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        self.press_key(key)
    }

    /// Press Backspace key
    pub fn press_backspace(&mut self) -> &mut Self {
        let key = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        self.press_key(key)
    }

    /// Send a message through the update cycle
    pub fn send_message(&mut self, msg: Msg) -> &mut Self {
        let (new_state, _cmds) = update(msg, self.state.clone());
        self.state = new_state;
        self.sync_state();
        self
    }

    /// Show new note input via message
    pub fn show_new_note(&mut self) -> &mut Self {
        self.send_message(Msg::Ui(UiMsg::ShowNewNote))
    }

    /// Cancel input via message
    pub fn cancel_input(&mut self) -> &mut Self {
        self.send_message(Msg::Ui(UiMsg::CancelInput))
    }

    /// Submit input via message
    pub fn submit_input(&mut self) -> &mut Self {
        self.send_message(Msg::Ui(UiMsg::SubmitNote))
    }

    // === Assertion Methods ===

    /// Assert the current input content
    pub fn assert_content(&self, expected: &str) {
        assert_eq!(
            self.state.ui.textarea.content, expected,
            "Expected content '{}', got '{}'",
            expected, self.state.ui.textarea.content
        );
    }

    /// Assert that content contains the given substring
    pub fn assert_content_contains(&self, substring: &str) {
        assert!(
            self.state.ui.textarea.content.contains(substring),
            "Expected content to contain '{}', got '{}'",
            substring,
            self.state.ui.textarea.content
        );
    }

    /// Assert input is active
    pub fn assert_input_active(&self) {
        assert!(
            HomeInput::is_input_active(&self.state),
            "Expected input to be active"
        );
    }

    /// Assert input is not active
    pub fn assert_input_not_active(&self) {
        assert!(
            !HomeInput::is_input_active(&self.state),
            "Expected input to be inactive"
        );
    }

    /// Assert input mode description
    pub fn assert_mode_description(&self, expected: &str) {
        let actual = HomeInput::get_input_mode_description(&self.state);
        assert_eq!(
            actual, expected,
            "Expected mode description '{expected}', got '{actual}'",
        );
    }

    /// Assert input can be submitted
    pub fn assert_can_submit(&self) {
        assert!(
            HomeInput::can_submit(&self.state),
            "Expected input to be submittable"
        );
    }

    /// Assert input cannot be submitted
    pub fn assert_cannot_submit(&self) {
        assert!(
            !HomeInput::can_submit(&self.state),
            "Expected input to not be submittable"
        );
    }

    /// Assert submit data is available and return it
    pub fn assert_submit_data(&self) -> SubmitData {
        self.state
            .ui
            .prepare_submit_data()
            .expect("Expected submit data to be available")
    }

    /// Assert submit data is not available
    pub fn assert_no_submit_data(&self) {
        assert!(
            self.state.ui.prepare_submit_data().is_none(),
            "Expected no submit data to be available"
        );
    }

    /// Assert character count
    pub fn assert_char_count(&self, expected: usize) {
        let stats = HomeInput::get_input_stats(&self.state);
        assert_eq!(
            stats.char_count, expected,
            "Expected char count {}, got {}",
            expected, stats.char_count
        );
    }

    /// Assert line count
    pub fn assert_line_count(&self, expected: usize) {
        let stats = HomeInput::get_input_stats(&self.state);
        assert_eq!(
            stats.line_count, expected,
            "Expected line count {}, got {}",
            expected, stats.line_count
        );
    }

    /// Assert word count
    pub fn assert_word_count(&self, expected: usize) {
        let stats = HomeInput::get_input_stats(&self.state);
        assert_eq!(
            stats.word_count, expected,
            "Expected word count {}, got {}",
            expected, stats.word_count
        );
    }

    /// Assert content is empty
    pub fn assert_empty(&self) {
        let stats = HomeInput::get_input_stats(&self.state);
        assert!(stats.is_empty, "Expected content to be empty");
    }

    /// Assert content is not empty
    pub fn assert_not_empty(&self) {
        let stats = HomeInput::get_input_stats(&self.state);
        assert!(!stats.is_empty, "Expected content to not be empty");
    }

    /// Assert cursor position
    pub fn assert_cursor_position(&self, line: usize, column: usize) {
        let cursor = &self.state.ui.textarea.cursor_position;
        assert_eq!(
            cursor.line, line,
            "Expected cursor row {}, got {}",
            line, cursor.line
        );
        assert_eq!(
            cursor.column, column,
            "Expected cursor col {}, got {}",
            column, cursor.column
        );
    }

    /// Assert cursor at start of document
    pub fn assert_cursor_at_start(&self) {
        self.assert_cursor_position(0, 0);
    }

    /// Assert no selection
    pub fn assert_no_selection(&self) {
        assert!(
            self.state.ui.textarea.selection.is_none(),
            "Expected no selection, but selection is present"
        );
    }

    /// Assert selection exists
    pub fn assert_has_selection(&self) {
        assert!(
            self.state.ui.textarea.selection.is_some(),
            "Expected selection to exist, but none found"
        );
    }

    // === Getter Methods ===

    /// Get the current input content
    pub fn content(&self) -> &str {
        &self.state.ui.textarea.content
    }

    /// Get the current state (for advanced assertions)
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Get the current input component (for advanced assertions)
    pub fn input(&self) -> &HomeInput<'a> {
        &self.input
    }

    /// Get mutable access to input component (for advanced testing)
    pub fn input_mut(&mut self) -> &mut HomeInput<'a> {
        &mut self.input
    }

    // === Private Helper Methods ===

    /// Sync textarea state with app state
    fn sync_state(&mut self) {
        self.input.sync_textarea_with_state(&self.state);
    }

    /// Create a consistent test public key
    fn create_test_pubkey() -> PublicKey {
        // Use a fixed key for consistent testing
        PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001")
            .expect("Valid test public key")
    }
}

impl<'a> Default for TextAreaTestHelper<'a> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_helper_basic_usage() {
        let helper = TextAreaTestHelper::new();
        helper.assert_input_not_active();
        helper.assert_content("");
        helper.assert_mode_description("Navigation mode");
    }

    #[test]
    fn test_helper_with_content() {
        let helper = TextAreaTestHelper::with_content("Hello World");
        helper.assert_content("Hello World");
        helper.assert_char_count(11);
        helper.assert_word_count(2);
        helper.assert_not_empty();
    }

    #[test]
    fn test_helper_input_mode() {
        let helper = TextAreaTestHelper::in_input_mode();
        helper.assert_input_active();
        helper.assert_mode_description("Compose mode");
    }

    #[test]
    fn test_helper_typing() {
        let mut helper = TextAreaTestHelper::in_input_mode();
        helper.type_text("Hello");
        helper.assert_content("Hello");
        helper.press_char(' ');
        helper.type_text("World");
        helper.assert_content("Hello World");
        helper.assert_char_count(11);
    }

    #[test]
    fn test_helper_elm_messages() {
        let mut helper = TextAreaTestHelper::new();
        helper.assert_input_not_active();
        helper.show_new_note();
        helper.assert_input_active();
        helper.cancel_input();
        helper.assert_input_not_active();
    }

    #[test]
    fn test_helper_submit_flow() {
        let helper = TextAreaTestHelper::in_input_mode_with_content("Test post");
        helper.assert_can_submit();
        helper.assert_submit_data(); // Will panic if submit data is not available
    }

    #[test]
    fn test_helper_navigation_keys() {
        let mut helper = TextAreaTestHelper::in_input_mode_with_content("Hello World");
        helper.ctrl_a(); // Go to start
        helper.type_text(">>> "); // Should insert at beginning
        helper.assert_content_contains(">>>"); // Content should contain our prefix
    }
}
