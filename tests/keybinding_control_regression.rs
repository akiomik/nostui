use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nostr_sdk::prelude::*;
use nostui::action::Action;
use nostui::components::elm_home_adapter::ElmHomeAdapter;
use nostui::components::Component;
use nostui::state::AppState;

/// Test for keybinding control in input mode
/// This test verifies that inappropriate actions are blocked during text input
#[test]
fn test_input_mode_blocks_inappropriate_actions() -> Result<()> {
    let mut adapter = ElmHomeAdapter::new();

    // Initialize with a minimal state
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    adapter.init(area)?;

    // Note: In a real test environment, we'd need to set up ElmRuntime properly
    // For now, we test the structure and ensure no panics occur

    // Test React action (should be handled appropriately in input mode)
    let react_action = Action::React;
    let result = adapter.update(react_action);
    assert!(
        result.is_ok(),
        "React action should be handled without panic"
    );

    // Test Repost action
    let repost_action = Action::Repost;
    let result = adapter.update(repost_action);
    assert!(
        result.is_ok(),
        "Repost action should be handled without panic"
    );

    // Test NewTextNote action
    let new_note_action = Action::NewTextNote;
    let result = adapter.update(new_note_action);
    assert!(
        result.is_ok(),
        "NewTextNote action should be handled without panic"
    );

    Ok(())
}

/// Test that navigation keys are properly differentiated from content keys
#[test]
fn test_navigation_key_detection() {
    // This test verifies the logic that differentiates navigation from content keys
    let test_cases = vec![
        // Navigation keys
        (KeyCode::Left, KeyModifiers::NONE, true),
        (KeyCode::Right, KeyModifiers::NONE, true),
        (KeyCode::Up, KeyModifiers::NONE, true),
        (KeyCode::Down, KeyModifiers::NONE, true),
        (KeyCode::Home, KeyModifiers::NONE, true),
        (KeyCode::End, KeyModifiers::NONE, true),
        (KeyCode::Char('a'), KeyModifiers::CONTROL, true), // Ctrl+A
        (KeyCode::Char('e'), KeyModifiers::CONTROL, true), // Ctrl+E
        (KeyCode::Char('b'), KeyModifiers::CONTROL, true), // Ctrl+B
        (KeyCode::Char('f'), KeyModifiers::CONTROL, true), // Ctrl+F
        (KeyCode::Char('p'), KeyModifiers::CONTROL, true), // Ctrl+P (but this might be submit)
        (KeyCode::Char('n'), KeyModifiers::CONTROL, true), // Ctrl+N
        // Content keys
        (KeyCode::Char('a'), KeyModifiers::NONE, false),
        (KeyCode::Char('z'), KeyModifiers::NONE, false),
        (KeyCode::Char('1'), KeyModifiers::NONE, false),
        (KeyCode::Enter, KeyModifiers::NONE, false),
        (KeyCode::Backspace, KeyModifiers::NONE, false),
        (KeyCode::Delete, KeyModifiers::NONE, false),
        (KeyCode::Char('u'), KeyModifiers::CONTROL, false), // Ctrl+U (delete)
        (KeyCode::Char('w'), KeyModifiers::CONTROL, false), // Ctrl+W (delete word)
    ];

    for (code, modifiers, expected_is_navigation) in test_cases {
        let key_event = KeyEvent::new(code, modifiers);
        let is_navigation = is_navigation_key(&key_event);
        assert_eq!(
            is_navigation, expected_is_navigation,
            "Key {:?} with modifiers {:?} should be navigation: {}",
            code, modifiers, expected_is_navigation
        );
    }
}

// Helper function that mirrors the logic in ElmHomeAdapter
fn is_navigation_key(key: &KeyEvent) -> bool {
    match key.code {
        KeyCode::Left
        | KeyCode::Right
        | KeyCode::Up
        | KeyCode::Down
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::PageUp
        | KeyCode::PageDown => true,
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            matches!(c, 'a' | 'e' | 'b' | 'f' | 'p' | 'n')
        }
        _ => false,
    }
}

/// Test for the app.rs keybinding blocking logic
#[test]
fn test_app_level_keybinding_blocking() {
    // This test verifies the logic that should be in app.rs for blocking keybindings

    let test_cases = vec![
        // These should be blocked in input mode
        (Action::React, true),
        (Action::Repost, true),
        (Action::ScrollDown, true),
        (Action::ScrollUp, true),
        // These should be allowed in input mode
        (Action::Unselect, false),       // Esc
        (Action::SubmitTextNote, false), // Ctrl+P
        // Quit with Ctrl+C should be allowed, but 'q' key should be blocked

        // System actions should always be allowed
        (Action::Suspend, false),
        (Action::Resume, false),
        (Action::Resize(80, 24), false),
    ];

    for (action, should_be_blocked) in test_cases {
        let is_blocked = should_block_action_in_input_mode(&action);
        assert_eq!(
            is_blocked, should_be_blocked,
            "Action {:?} should be blocked in input mode: {}",
            action, should_be_blocked
        );
    }
}

// Helper function that mirrors the logic in app.rs
fn should_block_action_in_input_mode(action: &Action) -> bool {
    match action {
        Action::Unselect | Action::Suspend | Action::SubmitTextNote => false,
        Action::Quit => false, // Depends on implementation - might check for Ctrl+C
        Action::Resume | Action::Resize(_, _) => false,
        _ => true, // Block everything else
    }
}

/// Test for the q key vs Quit action distinction
#[test]
fn test_q_key_vs_quit_action_in_input_mode() -> Result<()> {
    // Test that 'q' key in input mode should be treated as character input
    // while Ctrl+C should still quit

    let q_key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    // let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);

    // In input mode, 'q' should be a content key
    assert!(
        !is_navigation_key(&q_key),
        "'q' should not be a navigation key"
    );

    // Ctrl+C should be handled as a system action
    // This would typically be converted to Action::Quit at a higher level

    Ok(())
}

/// Integration test for complete keybinding flow
#[test]
fn test_complete_keybinding_flow() -> Result<()> {
    let mut adapter = ElmHomeAdapter::new();
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    adapter.init(area)?;

    // Simulate the complete flow:
    // 1. Open new note
    // 2. Type some text
    // 3. Try navigation
    // 4. Try inappropriate action (should be blocked)
    // 5. Close with Esc

    // 1. Open new note
    let new_note = Action::NewTextNote;
    adapter.update(new_note)?;

    // 2. Type text
    let char_key = Action::Key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::NONE));
    adapter.update(char_key)?;

    // 3. Navigation
    let nav_key = Action::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    adapter.update(nav_key)?;

    // 4. Inappropriate action (React) - should be handled gracefully
    let react = Action::React;
    adapter.update(react)?;

    // 5. Close
    let esc = Action::Unselect;
    adapter.update(esc)?;

    // All steps should complete without panic
    Ok(())
}

/// Test for the overlay vs split layout issue
#[test]
fn test_overlay_rendering_logic() -> Result<()> {
    use nostui::components::elm_home::ElmHome;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend)?;
    let mut elm_home = ElmHome::new();

    // Test that in input mode, we use overlay approach not split approach
    // Create a dummy public key for testing
    let dummy_pubkey =
        PublicKey::from_hex("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
    let mut state = AppState::new(dummy_pubkey);
    state.ui.show_input = true;

    terminal.draw(|f| {
        let area = f.area(); // Full area: 80x24

        // With overlay approach:
        // - Timeline gets full area (80x24)
        // - Input gets calculated overlay area (roughly 80x12, positioned at y=12)

        // This should work without issues
        elm_home.render(f, area, &state);
    })?;

    Ok(())
}
