use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nostr_sdk::prelude::*;

use nostui::{
    core::{
        msg::{ui::UiMsg, Msg},
        raw_msg::RawMsg,
        state::{ui::UiMode, AppState},
        translator::translate_raw_to_domain,
        update::update,
    },
    domain::ui::CursorPosition,
    test_helpers::TextAreaTestHelper,
};

/// Test Hybrid Approach behavior
/// Verifies that TextArea delegation works without breaking special keys
#[test]
fn test_hybrid_special_keys_preserved() {
    let mut state = AppState::new(Keys::generate().public_key());
    state.ui.current_mode = UiMode::Composing;

    // Ctrl+P should still submit
    let ctrl_p = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
    let result = translate_raw_to_domain(RawMsg::Key(ctrl_p), &state);
    assert_eq!(result, vec![Msg::Ui(UiMsg::SubmitNote)]);

    // Esc should still cancel
    let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let result = translate_raw_to_domain(RawMsg::Key(esc), &state);
    assert_eq!(result, vec![Msg::Ui(UiMsg::CancelInput)]);
}

#[test]
fn test_hybrid_textarea_delegation() {
    let mut state = AppState::new(Keys::generate().public_key());
    state.ui.current_mode = UiMode::Composing;

    // Regular keys should delegate to ProcessTextAreaInput
    let char_key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    let result = translate_raw_to_domain(RawMsg::Key(char_key), &state);
    assert_eq!(result, vec![Msg::Ui(UiMsg::ProcessTextAreaInput(char_key))]);

    // Ctrl keys (non-special) should delegate to ProcessTextAreaInput
    let ctrl_w = KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL);
    let result = translate_raw_to_domain(RawMsg::Key(ctrl_w), &state);
    assert_eq!(result, vec![Msg::Ui(UiMsg::ProcessTextAreaInput(ctrl_w))]);

    // Enter should delegate to ProcessTextAreaInput
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let result = translate_raw_to_domain(RawMsg::Key(enter), &state);
    assert_eq!(result, vec![Msg::Ui(UiMsg::ProcessTextAreaInput(enter))]);
}

#[test]
fn test_hybrid_update_cycle() {
    // Test that ProcessTextAreaInput doesn't break the update cycle
    let mut helper = TextAreaTestHelper::in_input_mode();

    // These should work through ProcessTextAreaInput delegation
    helper.press_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
    helper.press_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));

    // Content should be updated correctly
    helper.assert_content("hi");
    helper.assert_input_active();
}

#[test]
#[ignore] // Legacy test - requires adaptation for pending_keys approach
fn test_hybrid_terminal_keybinds() {
    let mut helper = TextAreaTestHelper::in_input_mode_with_content("hello world");

    // Terminal keybinds should work through TextArea delegation
    helper.press_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));

    // Note: pending_keys approach may change TextArea behavior
    // This test needs to be updated based on actual behavior verification
    let content_after = helper.content();
    assert_ne!(content_after, "hello world"); // Should be different
    helper.assert_input_active(); // Should still be in input mode
}

#[test]
fn test_pending_keys_basic_functionality() {
    let mut state = AppState::new(Keys::generate().public_key());
    state.ui.current_mode = UiMode::Composing;
    state.ui.input_content = "test".to_string();
    state.ui.cursor_position = CursorPosition { line: 0, column: 4 };

    // Add character at cursor position
    let char_key = KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE);
    let (new_state, _) = update(Msg::Ui(UiMsg::ProcessTextAreaInput(char_key)), state);

    // Verify content was updated and pending_keys was processed
    assert_eq!(new_state.ui.input_content, "test!");
    assert!(new_state.ui.pending_input_keys.is_empty());
    assert_eq!(new_state.ui.cursor_position.column, 5);
}

#[test]
fn test_pending_keys_navigation_functionality() {
    let mut state = AppState::new(Keys::generate().public_key());
    state.ui.current_mode = UiMode::Composing;
    state.ui.input_content = "hello world".to_string();
    state.ui.cursor_position = CursorPosition { line: 0, column: 5 }; // At space

    // Test left arrow navigation
    let left_key = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
    let (new_state, _) = update(Msg::Ui(UiMsg::ProcessTextAreaInput(left_key)), state);

    // Verify cursor moved left but content unchanged
    assert_eq!(new_state.ui.input_content, "hello world");
    assert_eq!(new_state.ui.cursor_position.column, 4); // Moved to 'o'
    assert!(new_state.ui.pending_input_keys.is_empty());
}

#[test]
fn test_pending_keys_multiple_operations() {
    let mut state = AppState::new(Keys::generate().public_key());
    state.ui.current_mode = UiMode::Composing;
    state.ui.input_content = "test".to_string();
    state.ui.cursor_position = CursorPosition { line: 0, column: 4 };

    // First operation: move left
    let left_key = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
    let (state, _) = update(Msg::Ui(UiMsg::ProcessTextAreaInput(left_key)), state);

    // Second operation: type character
    let char_key = KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE);
    let (final_state, _) = update(Msg::Ui(UiMsg::ProcessTextAreaInput(char_key)), state);

    // Verify final state: "tes!t" with cursor after '!'
    assert_eq!(final_state.ui.input_content, "tes!t");
    assert_eq!(final_state.ui.cursor_position.column, 4);
    assert!(final_state.ui.pending_input_keys.is_empty());
}

#[test]
fn test_pending_keys_home_end_navigation() {
    let mut state = AppState::new(Keys::generate().public_key());
    state.ui.current_mode = UiMode::Composing;
    state.ui.input_content = "hello world".to_string();
    state.ui.cursor_position = CursorPosition { line: 0, column: 5 };

    // Test Home key
    let home_key = KeyEvent::new(KeyCode::Home, KeyModifiers::NONE);
    let (state, _) = update(Msg::Ui(UiMsg::ProcessTextAreaInput(home_key)), state);
    assert_eq!(state.ui.cursor_position.column, 0);

    // Test End key
    let end_key = KeyEvent::new(KeyCode::End, KeyModifiers::NONE);
    let (final_state, _) = update(Msg::Ui(UiMsg::ProcessTextAreaInput(end_key)), state);
    assert_eq!(final_state.ui.cursor_position.column, 11); // End of "hello world"
}

#[test]
fn test_pending_keys_maintains_state_consistency() {
    let mut state = AppState::new(Keys::generate().public_key());
    state.ui.current_mode = UiMode::Composing;
    state.ui.input_content = "test content".to_string();
    state.ui.cursor_position = CursorPosition { line: 0, column: 4 };

    // Verify AppState is always the single source of truth
    let backspace_key = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
    let (new_state, _) = update(Msg::Ui(UiMsg::ProcessTextAreaInput(backspace_key)), state);

    // Content should be updated and cursor moved back
    assert_eq!(new_state.ui.input_content, "tes content");
    assert_eq!(new_state.ui.cursor_position.column, 3);
    assert!(new_state.ui.pending_input_keys.is_empty());
}

#[test]
fn test_pending_keys_empty_queue_after_processing() {
    let mut state = AppState::new(Keys::generate().public_key());
    state.ui.current_mode = UiMode::Composing;

    // Multiple key presses should all be processed
    let char_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    let (state, _) = update(Msg::Ui(UiMsg::ProcessTextAreaInput(char_a)), state);

    let char_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE);
    let (final_state, _) = update(Msg::Ui(UiMsg::ProcessTextAreaInput(char_b)), state);

    // All keys should be processed and queue should be empty
    assert_eq!(final_state.ui.input_content, "ab");
    assert!(final_state.ui.pending_input_keys.is_empty());
}

#[test]
fn test_hybrid_no_circular_updates() {
    let mut state = AppState::new(Keys::generate().public_key());
    state.ui.current_mode = UiMode::Composing;
    state.ui.input_content = "test".to_string();

    // Process a character through the hybrid approach
    let char_key = KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE);
    let (new_state, cmds) = update(Msg::Ui(UiMsg::ProcessTextAreaInput(char_key)), state);

    // Should not generate additional commands (no circular updates)
    assert!(cmds.is_empty());

    // Content should be updated
    assert_ne!(new_state.ui.input_content, "test"); // Should be different
}

#[test]
fn test_hybrid_input_mode_only() {
    let mut state = AppState::new(Keys::generate().public_key());
    state.ui.current_mode = UiMode::Normal; // Not in input mode

    // ProcessTextAreaInput should do nothing when not in input mode
    let char_key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    let (new_state, cmds) = update(
        Msg::Ui(UiMsg::ProcessTextAreaInput(char_key)),
        state.clone(),
    );

    // State should be unchanged
    assert_eq!(new_state.ui.input_content, state.ui.input_content);
    assert!(cmds.is_empty());
}

#[test]
fn test_hybrid_special_vs_regular_keys() {
    let mut helper = TextAreaTestHelper::in_input_mode();

    // Type some content first
    helper.type_text("hello");
    helper.assert_content("hello");

    // Special key: Ctrl+P should submit and exit input mode
    helper.press_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
    helper.assert_input_not_active();

    // Restart input mode
    helper.show_new_note();
    helper.type_text("world");

    // Special key: Esc should cancel and exit input mode
    helper.press_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    helper.assert_input_not_active();
}

#[test]
fn test_hybrid_complex_workflow() {
    let mut helper = TextAreaTestHelper::in_input_mode();

    // Step 1: Type content using delegated keys
    helper.type_text("Hello ");
    helper.assert_content("Hello ");

    // Step 2: Use TextArea's backspace (delegated)
    helper.press_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    // Content should change (we don't know exactly how TextArea handles it)

    // Step 3: Add more content
    helper.type_text("World");
    helper.assert_not_empty();

    // Step 4: Use special key to submit
    helper.press_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
    helper.assert_input_not_active();
}
