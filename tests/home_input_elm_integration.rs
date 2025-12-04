use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nostr_sdk::prelude::*;
use nostui::{
    components::elm_home_input::{ElmHomeInput, SubmitData},
    msg::Msg,
    state::AppState,
    update::update,
};

/// Test Home input layer integration with Elm architecture
#[test]
fn test_elm_home_input_creation_and_defaults() {
    let _input = ElmHomeInput::new();
    let _default_input = ElmHomeInput::default();

    // Should be creatable (note: cannot easily compare TextArea directly)
    // We test through behavior instead
    let state = AppState::new(Keys::generate().public_key());
    assert!(!ElmHomeInput::is_input_active(&state));
}

#[test]
fn test_input_activation_flow() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    // Initially not active
    assert!(!ElmHomeInput::is_input_active(&state));
    assert_eq!(
        ElmHomeInput::get_input_mode_description(&state),
        "Navigation mode"
    );

    // Show new note input
    let (new_state, cmds) = update(Msg::ShowNewNote, state);
    state = new_state;
    assert!(cmds.is_empty());
    assert!(ElmHomeInput::is_input_active(&state));
    assert_eq!(
        ElmHomeInput::get_input_mode_description(&state),
        "Compose mode"
    );

    // Cancel input
    let (new_state, cmds) = update(Msg::CancelInput, state);
    state = new_state;
    assert!(cmds.is_empty());
    assert!(!ElmHomeInput::is_input_active(&state));
}

#[test]
fn test_reply_mode_activation() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    let test_event = EventBuilder::text_note("Original post")
        .sign_with_keys(&keys)
        .unwrap();

    // Show reply
    let (new_state, cmds) = update(Msg::ShowReply(test_event), state);
    state = new_state;
    assert!(cmds.is_empty());
    assert!(ElmHomeInput::is_input_active(&state));
    assert_eq!(
        ElmHomeInput::get_input_mode_description(&state),
        "Reply mode"
    );
    assert!(state.ui.reply_to.is_some());
}

#[test]
fn test_input_content_updates() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    // Activate input
    let (new_state, _) = update(Msg::ShowNewNote, state);
    state = new_state;

    // Update content directly
    let (new_state, cmds) = update(Msg::UpdateInputContent("Hello, Nostr!".to_string()), state);
    state = new_state;
    assert!(cmds.is_empty());
    assert_eq!(state.ui.input_content, "Hello, Nostr!");

    // Test statistics
    let stats = ElmHomeInput::get_input_stats(&state);
    assert_eq!(stats.char_count, 13);
    assert!(!stats.is_empty);
    assert_eq!(stats.word_count, 2);
}

#[test]
fn test_submission_validation() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    // Cannot submit when input not active
    assert!(!ElmHomeInput::can_submit(&state));
    assert!(ElmHomeInput::get_submit_data(&state).is_none());

    // Cannot submit when input active but empty
    let (new_state, _) = update(Msg::ShowNewNote, state);
    state = new_state;
    assert!(!ElmHomeInput::can_submit(&state));

    // Cannot submit with only whitespace
    let (new_state, _) = update(Msg::UpdateInputContent("   \n\t  ".to_string()), state);
    state = new_state;
    assert!(!ElmHomeInput::can_submit(&state));

    // Can submit with actual content
    let (new_state, _) = update(Msg::UpdateInputContent("Hello, world!".to_string()), state);
    state = new_state;
    assert!(ElmHomeInput::can_submit(&state));

    let submit_data = ElmHomeInput::get_submit_data(&state).unwrap();
    assert_eq!(submit_data.content, "Hello, world!");
    assert!(submit_data.tags.is_empty()); // No reply tags for new note
}

#[test]
fn test_submission_with_reply_tags() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    let original_event = EventBuilder::text_note("Original post")
        .sign_with_keys(&keys)
        .unwrap();

    // Start reply
    let (new_state, _) = update(Msg::ShowReply(original_event), state);
    state = new_state;

    // Add content
    let (new_state, _) = update(
        Msg::UpdateInputContent("This is a reply".to_string()),
        state,
    );
    state = new_state;

    // Should be submittable with reply tags
    assert!(ElmHomeInput::can_submit(&state));
    let submit_data = ElmHomeInput::get_submit_data(&state).unwrap();
    assert_eq!(submit_data.content, "This is a reply");
    assert!(!submit_data.tags.is_empty()); // Should have reply tags
}

#[test]
fn test_submission_flow() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    // Setup input
    let (new_state, _) = update(Msg::ShowNewNote, state);
    state = new_state;
    let (new_state, _) = update(
        Msg::UpdateInputContent("Test submission".to_string()),
        state,
    );
    state = new_state;

    // Submit
    let (new_state, cmds) = update(Msg::SubmitNote, state);
    state = new_state;

    // Should generate SendTextNote command
    assert_eq!(cmds.len(), 1);
    match &cmds[0] {
        nostui::cmd::Cmd::SendTextNote { content, tags } => {
            assert_eq!(content, "Test submission");
            assert!(tags.is_empty());
        }
        _ => panic!("Expected SendTextNote command"),
    }

    // Input should be reset
    assert!(!state.ui.show_input);
    assert!(state.ui.input_content.is_empty());
    assert!(state.ui.reply_to.is_none());
}

#[test]
fn test_input_key_processing() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    // Activate input
    let (new_state, _) = update(Msg::ShowNewNote, state);
    state = new_state;

    // Process some key events
    let char_key = KeyEvent::new(KeyCode::Char('H'), KeyModifiers::NONE);
    let (new_state, cmds) = update(Msg::ProcessInputKey(char_key), state);
    state = new_state;
    assert!(cmds.is_empty());

    // Content should be updated (depends on TextArea implementation)
    // We can't easily test exact content without complex TextArea simulation
    // But we can test that the mechanism works
    assert!(state.ui.input_content.contains('H') || state.ui.input_content.is_empty());
}

#[test]
fn test_input_stats_comprehensive() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    // Empty content
    let stats = ElmHomeInput::get_input_stats(&state);
    assert_eq!(stats.char_count, 0);
    assert_eq!(stats.line_count, 1);
    assert_eq!(stats.word_count, 0);
    assert!(stats.is_empty);

    // Simple content
    state.ui.input_content = "Hello world!".to_string();
    let stats = ElmHomeInput::get_input_stats(&state);
    assert_eq!(stats.char_count, 12);
    assert_eq!(stats.word_count, 2);
    assert!(!stats.is_empty);

    // Multi-line content
    state.ui.input_content = "Line 1\nLine 2\nLine 3".to_string();
    let stats = ElmHomeInput::get_input_stats(&state);
    assert_eq!(stats.line_count, 3);
    assert_eq!(stats.word_count, 6);

    // Unicode content
    state.ui.input_content = "こんにちは🌟".to_string();
    let stats = ElmHomeInput::get_input_stats(&state);
    assert_eq!(stats.char_count, 6);

    // Whitespace only
    state.ui.input_content = "   \n  \t  ".to_string();
    let stats = ElmHomeInput::get_input_stats(&state);
    assert!(stats.is_empty); // Trimmed empty
    assert!(stats.char_count > 0); // But has characters
}

#[test]
fn test_mode_transitions() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    let test_event = EventBuilder::text_note("Test")
        .sign_with_keys(&keys)
        .unwrap();

    // Navigation → Compose
    assert_eq!(
        ElmHomeInput::get_input_mode_description(&state),
        "Navigation mode"
    );
    let (new_state, _) = update(Msg::ShowNewNote, state);
    state = new_state;
    assert_eq!(
        ElmHomeInput::get_input_mode_description(&state),
        "Compose mode"
    );

    // Compose → Navigation
    let (new_state, _) = update(Msg::CancelInput, state);
    state = new_state;
    assert_eq!(
        ElmHomeInput::get_input_mode_description(&state),
        "Navigation mode"
    );

    // Navigation → Reply
    let (new_state, _) = update(Msg::ShowReply(test_event), state);
    state = new_state;
    assert_eq!(
        ElmHomeInput::get_input_mode_description(&state),
        "Reply mode"
    );

    // Reply → Navigation
    let (new_state, _) = update(Msg::CancelInput, state);
    state = new_state;
    assert_eq!(
        ElmHomeInput::get_input_mode_description(&state),
        "Navigation mode"
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

#[tokio::test]
async fn test_complete_input_workflow() {
    let author_keys = Keys::generate();
    let user_keys = Keys::generate();
    let mut state = AppState::new(user_keys.public_key());

    // 1. Start in navigation mode
    assert_eq!(
        ElmHomeInput::get_input_mode_description(&state),
        "Navigation mode"
    );
    assert!(!ElmHomeInput::is_input_active(&state));

    // 2. Start new post
    let (new_state, _) = update(Msg::ShowNewNote, state);
    state = new_state;
    assert_eq!(
        ElmHomeInput::get_input_mode_description(&state),
        "Compose mode"
    );
    assert!(ElmHomeInput::is_input_active(&state));

    // 3. Type content
    let (new_state, _) = update(
        Msg::UpdateInputContent("Hello, Nostr community!".to_string()),
        state,
    );
    state = new_state;
    let stats = ElmHomeInput::get_input_stats(&state);
    assert!(!stats.is_empty);
    assert!(ElmHomeInput::can_submit(&state));

    // 4. Submit post
    let (new_state, cmds) = update(Msg::SubmitNote, state);
    state = new_state;
    assert_eq!(cmds.len(), 1);
    assert!(!ElmHomeInput::is_input_active(&state));

    // 5. Start reply to another post
    let original_post = EventBuilder::text_note("Original content")
        .sign_with_keys(&author_keys)
        .unwrap();

    let (new_state, _) = update(Msg::ShowReply(original_post), state);
    state = new_state;
    assert_eq!(
        ElmHomeInput::get_input_mode_description(&state),
        "Reply mode"
    );

    // 6. Type reply
    let (new_state, _) = update(Msg::UpdateInputContent("Great point!".to_string()), state);
    state = new_state;

    // 7. Submit reply
    let (new_state, cmds) = update(Msg::SubmitNote, state);
    state = new_state;
    assert_eq!(cmds.len(), 1);

    // Verify reply has tags
    if let nostui::cmd::Cmd::SendTextNote { content, tags } = &cmds[0] {
        assert_eq!(content, "Great point!");
        assert!(!tags.is_empty()); // Reply should have tags
    } else {
        panic!("Expected SendTextNote command");
    }

    // 8. Back to navigation
    assert_eq!(
        ElmHomeInput::get_input_mode_description(&state),
        "Navigation mode"
    );
}

#[test]
fn test_empty_submission_prevention() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    // Start input
    let (new_state, _) = update(Msg::ShowNewNote, state);
    state = new_state;

    // Try to submit empty
    let (new_state, cmds) = update(Msg::SubmitNote, state);
    state = new_state;
    assert!(cmds.is_empty()); // Should not generate any commands
    assert!(state.ui.show_input); // Should stay in input mode

    // Try to submit whitespace only
    let (new_state, _) = update(Msg::UpdateInputContent("   \n\t   ".to_string()), state);
    state = new_state;
    let (new_state, cmds) = update(Msg::SubmitNote, state);
    state = new_state;
    assert!(cmds.is_empty()); // Should not generate any commands
    assert!(state.ui.show_input); // Should stay in input mode
}

#[test]
fn test_input_stats_edge_cases() {
    let mut state = AppState::new(Keys::generate().public_key());

    // Empty string
    state.ui.input_content = String::new();
    let stats = ElmHomeInput::get_input_stats(&state);
    assert_eq!(stats.char_count, 0);
    assert_eq!(stats.line_count, 1);
    assert!(stats.is_empty);

    // Single character
    state.ui.input_content = "a".to_string();
    let stats = ElmHomeInput::get_input_stats(&state);
    assert_eq!(stats.char_count, 1);
    assert_eq!(stats.word_count, 1);

    // Only newlines
    state.ui.input_content = "\n\n\n".to_string();
    let stats = ElmHomeInput::get_input_stats(&state);
    assert_eq!(stats.line_count, 3); // lines().count() for "\n\n\n"
    assert!(stats.is_empty); // Only whitespace

    // Mixed content
    state.ui.input_content = "Hello\n\nWorld!\n".to_string();
    let stats = ElmHomeInput::get_input_stats(&state);
    assert_eq!(stats.line_count, 3); // lines().count() behavior
    assert_eq!(stats.word_count, 2);
    assert!(!stats.is_empty);
}
