use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nostr_sdk::prelude::*;
use nostui::components::elm_home_input::ElmHomeInput;
use nostui::state::AppState;

/// Specific test for the input area height calculation bug
/// This test would have caught the original bug where height became 0
#[test]
fn test_input_area_height_calculation_not_zero() -> Result<()> {
    let mut input = ElmHomeInput::new();
    // Create a dummy public key for testing
    let dummy_pubkey =
        PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
    let mut state = AppState::new(dummy_pubkey);

    // Enable input mode
    state.ui.show_input = true;
    state.ui.input_content = "test content".to_string();

    // Create a mock frame and area to test rendering
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let backend = TestBackend::new(140, 32); // Size from the bug logs
    let mut terminal = Terminal::new(backend)?;

    terminal.draw(|f| {
        // This is the area that was problematic in the bug:
        // From logs: Rect { x: 0, y: 29, width: 140, height: 3 }
        let problematic_area = ratatui::layout::Rect {
            x: 0,
            y: 29,
            width: 140,
            height: 3, // This was the issue - too small
        };

        // The old calculation would have resulted in height: 0
        // height /= 2;          // 3 / 2 = 1
        // y = height;           // y = 1
        // height -= 2;          // 1 - 2 = 0 (saturating_sub)

        // With our fix, we should use the area directly without complex calculation
        let result = input.draw(&state, f, problematic_area);

        // The draw should succeed without panicking
        assert!(
            result.is_ok(),
            "Input draw should succeed even with small area"
        );

        // Test with a proper sized area
        let proper_area = ratatui::layout::Rect {
            x: 0,
            y: 15,
            width: 140,
            height: 17, // Half of 32 + 2 margin
        };

        let result = input.draw(&state, f, proper_area);
        assert!(result.is_ok(), "Input draw should succeed with proper area");
    })?;

    Ok(())
}

/// Test for the specific sync issue that caused character overwriting
#[test]
fn test_textarea_sync_preserves_content() -> Result<()> {
    let mut input = ElmHomeInput::new();
    // Create a dummy public key for testing
    let dummy_pubkey =
        PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
    let mut state = AppState::new(dummy_pubkey);

    state.ui.show_input = true;

    // Test the progression that would have failed before the fix

    // 1. Initial state - empty
    state.ui.input_content = String::new();
    input.sync_textarea_with_state(&state);

    // 2. Type 'h'
    let h_key = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE);
    if let Some(content) = input.process_key_input(h_key) {
        state.ui.input_content = content;
        assert_eq!(state.ui.input_content, "h");
    }

    // 3. Sync again (this would have reset cursor position in buggy version)
    input.sync_textarea_with_state(&state);

    // 4. Type 'e' - should append, not overwrite
    let e_key = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE);
    if let Some(content) = input.process_key_input(e_key) {
        state.ui.input_content = content;
        assert_eq!(
            state.ui.input_content, "he",
            "Characters should append, not overwrite"
        );
    }

    // 5. Continue typing to ensure the bug is fully fixed
    let l_key = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE);
    if let Some(content) = input.process_key_input(l_key) {
        state.ui.input_content = content;
        assert_eq!(state.ui.input_content, "hel");
    }

    let l2_key = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE);
    if let Some(content) = input.process_key_input(l2_key) {
        state.ui.input_content = content;
        assert_eq!(state.ui.input_content, "hell");
    }

    let o_key = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE);
    if let Some(content) = input.process_key_input(o_key) {
        state.ui.input_content = content;
        assert_eq!(state.ui.input_content, "hello");
    }

    Ok(())
}

/// Test for navigation key handling that was broken
#[test]
fn test_navigation_key_storage_and_processing() -> Result<()> {
    let mut input = ElmHomeInput::new();
    // Create a dummy public key for testing
    let dummy_pubkey =
        PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
    let mut state = AppState::new(dummy_pubkey);

    state.ui.show_input = true;
    state.ui.input_content = "Hello World".to_string();

    // Test that navigation keys are handled differently from content keys
    let left_key = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);

    // Navigation key should be stored for later processing
    input.process_navigation_key(left_key);

    // When we sync and render, the navigation key should be processed
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend)?;

    terminal.draw(|f| {
        let area = ratatui::layout::Rect {
            x: 0,
            y: 10,
            width: 80,
            height: 10,
        };

        // This should process the pending navigation key
        let result = input.draw(&state, f, area);
        assert!(result.is_ok(), "Draw with navigation key should succeed");
    })?;

    Ok(())
}

/// Test for newline handling that was problematic
#[test]
fn test_newline_preservation() -> Result<()> {
    let mut input = ElmHomeInput::new();
    // Create a dummy public key for testing
    let dummy_pubkey =
        PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
    let mut state = AppState::new(dummy_pubkey);

    state.ui.show_input = true;

    // Type some text
    state.ui.input_content = "Hello".to_string();
    input.sync_textarea_with_state(&state);

    // Press Enter
    let enter_key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    if let Some(content) = input.process_key_input(enter_key) {
        state.ui.input_content = content;
        assert!(
            state.ui.input_content.contains('\n'),
            "Content should contain newline after Enter: '{}'",
            state.ui.input_content
        );
    }

    // Type more text
    let w_key = KeyEvent::new(KeyCode::Char('W'), KeyModifiers::NONE);
    if let Some(content) = input.process_key_input(w_key) {
        state.ui.input_content = content;
        assert!(
            state.ui.input_content.contains("Hello"),
            "Original content before newline should be preserved"
        );
        assert!(
            state.ui.input_content.contains('\n'),
            "Newline should be preserved"
        );
        assert!(
            state.ui.input_content.ends_with('W'),
            "New character should be appended after newline"
        );
    }

    Ok(())
}

/// Test for Ctrl key combinations that were broken
#[test]
fn test_ctrl_key_combinations() -> Result<()> {
    let mut input = ElmHomeInput::new();
    // Create a dummy public key for testing
    let dummy_pubkey =
        PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
    let mut state = AppState::new(dummy_pubkey);

    state.ui.show_input = true;
    state.ui.input_content = "Hello World Test".to_string();

    // First sync the input with the state
    input.sync_textarea_with_state(&state);

    // Test Ctrl+A (should be handled as navigation)
    let ctrl_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
    input.process_navigation_key(ctrl_a);

    // Test Ctrl+E (should be handled as navigation)
    let ctrl_e = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL);
    input.process_navigation_key(ctrl_e);

    // Test that these don't interfere with normal text input
    let char_key = KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE);
    if let Some(content) = input.process_key_input(char_key) {
        state.ui.input_content = content;
        assert!(
            state.ui.input_content.contains("Hello World Test"),
            "Original content should be preserved with navigation keys, got: '{}'",
            state.ui.input_content
        );
    }

    Ok(())
}
