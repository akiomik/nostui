use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nostr_sdk::prelude::*;
use nostui::{
    core::msg::Msg, core::raw_msg::RawMsg, core::state::AppState,
    core::translator::translate_raw_to_domain, core::update::update,
    test_helpers::TextAreaTestHelper,
};

/// Test Hybrid Approach behavior
/// Verifies that TextArea delegation works without breaking special keys
#[test]
fn test_hybrid_special_keys_preserved() {
    let mut state = AppState::new(Keys::generate().public_key());
    state.ui.show_input = true;

    // Ctrl+P should still submit
    let ctrl_p = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
    let result = translate_raw_to_domain(RawMsg::Key(ctrl_p), &state);
    assert_eq!(result, vec![Msg::SubmitNote]);

    // Esc should still cancel
    let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let result = translate_raw_to_domain(RawMsg::Key(esc), &state);
    assert_eq!(result, vec![Msg::CancelInput]);
}

#[test]
fn test_hybrid_textarea_delegation() {
    let mut state = AppState::new(Keys::generate().public_key());
    state.ui.show_input = true;

    // Regular keys should delegate to ProcessTextAreaInput
    let char_key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    let result = translate_raw_to_domain(RawMsg::Key(char_key), &state);
    assert_eq!(result, vec![Msg::ProcessTextAreaInput(char_key)]);

    // Ctrl keys (non-special) should delegate to ProcessTextAreaInput
    let ctrl_w = KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL);
    let result = translate_raw_to_domain(RawMsg::Key(ctrl_w), &state);
    assert_eq!(result, vec![Msg::ProcessTextAreaInput(ctrl_w)]);

    // Enter should delegate to ProcessTextAreaInput
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let result = translate_raw_to_domain(RawMsg::Key(enter), &state);
    assert_eq!(result, vec![Msg::ProcessTextAreaInput(enter)]);
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
fn test_hybrid_terminal_keybinds() {
    let mut helper = TextAreaTestHelper::in_input_mode_with_content("hello world");

    // Terminal keybinds should work through TextArea delegation
    helper.press_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));

    // Content should be changed by TextArea's Ctrl+W handling
    // Note: We don't know exactly what the content will be, but it should be different
    let content_after = helper.content();
    assert_ne!(content_after, "hello world"); // Should be different
    helper.assert_input_active(); // Should still be in input mode
}

#[test]
fn test_hybrid_no_circular_updates() {
    let mut state = AppState::new(Keys::generate().public_key());
    state.ui.show_input = true;
    state.ui.input_content = "test".to_string();

    // Process a character through the hybrid approach
    let char_key = KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE);
    let (new_state, cmds) = update(Msg::ProcessTextAreaInput(char_key), state);

    // Should not generate additional commands (no circular updates)
    assert!(cmds.is_empty());

    // Content should be updated
    assert_ne!(new_state.ui.input_content, "test"); // Should be different
}

#[test]
fn test_hybrid_input_mode_only() {
    let mut state = AppState::new(Keys::generate().public_key());
    state.ui.show_input = false; // Not in input mode

    // ProcessTextAreaInput should do nothing when not in input mode
    let char_key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    let (new_state, cmds) = update(Msg::ProcessTextAreaInput(char_key), state.clone());

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
