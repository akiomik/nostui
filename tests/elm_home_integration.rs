use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nostr_sdk::prelude::*;
use nostui::{
    core::msg::Msg,
    core::raw_msg::RawMsg,
    core::state::AppState,
    core::translator::translate_raw_to_domain,
    presentation::components::elm_home::{ElmHome, HomeAction},
};

/// Integration tests for ElmHome component
fn create_test_state() -> AppState {
    AppState::new(Keys::generate().public_key())
}

fn create_test_event() -> Event {
    let keys = Keys::generate();
    EventBuilder::text_note("test content")
        .sign_with_keys(&keys)
        .unwrap()
}

#[test]
fn test_elm_home_complete_workflow() -> Result<()> {
    let home = ElmHome::new();
    let mut state = create_test_state();

    // Initial state
    assert!(home
        .get_available_actions(&state)
        .contains(&HomeAction::ShowNewNote));
    assert!(!home.can_interact(&state));
    assert!(!home.can_submit_input(&state));

    // Add some timeline content
    let event = create_test_event();
    let sortable = nostui::domain::nostr::SortableEvent::new(event.clone());
    state
        .timeline
        .notes
        .find_or_insert(std::cmp::Reverse(sortable));
    state.timeline.selected_index = Some(0);

    // Now can interact
    assert!(home.can_interact(&state));
    let actions = home.get_available_actions(&state);
    assert!(actions.contains(&HomeAction::SendReaction));
    assert!(actions.contains(&HomeAction::ShowReply));
    assert!(actions.contains(&HomeAction::SendRepost));

    // Test status info
    let status = home.get_status_info(&state);
    assert_eq!(status.timeline_count, 1);
    assert_eq!(status.selected_index, Some(0));
    assert!(!status.input_mode);
    assert!(status.can_interact);

    Ok(())
}

#[test]
fn test_elm_home_input_workflow() -> Result<()> {
    let home = ElmHome::new();
    let mut state = create_test_state();

    // Start new note
    state.ui.show_input = true;
    state.ui.input_content = "Hello, Nostr!".to_string();

    // Check input mode status
    let status = home.get_status_info(&state);
    assert!(status.input_mode);
    assert!(!status.reply_mode);

    // Check available actions in input mode
    let actions = home.get_available_actions(&state);
    assert!(actions.contains(&HomeAction::SubmitNote));
    assert!(actions.contains(&HomeAction::CancelInput));
    assert!(!actions.contains(&HomeAction::SendReaction));

    // Test help text
    let help = home.get_help_text(&state);
    assert!(help.contains("Enter"));
    assert!(help.contains("Send note"));

    Ok(())
}

#[test]
fn test_elm_home_reply_workflow() -> Result<()> {
    let home = ElmHome::new();
    let mut state = create_test_state();
    let target_event = create_test_event();

    // Setup reply mode
    state.ui.show_input = true;
    state.ui.reply_to = Some(target_event.clone());
    state.ui.input_content = "Great point!".to_string();

    // Check reply mode status
    let status = home.get_status_info(&state);
    assert!(status.input_mode);
    assert!(status.reply_mode);

    // Test help text for reply
    let help = home.get_help_text(&state);
    assert!(help.contains("Send reply"));

    Ok(())
}

#[test]
fn test_elm_home_key_processing() -> Result<()> {
    let mut home = ElmHome::new();
    let mut state = create_test_state();

    // Add event to timeline
    let event = create_test_event();
    let sortable = nostui::domain::nostr::SortableEvent::new(event.clone());
    state
        .timeline
        .notes
        .find_or_insert(std::cmp::Reverse(sortable));
    state.timeline.selected_index = Some(0);

    // Test navigation keys (should be handled by list component)
    let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
    let msgs = home.process_key(key, &state);
    // Navigation is handled by translator.rs, component returns empty
    assert!(msgs.is_empty() || msgs.iter().all(|msg| matches!(msg, Msg::ScrollDown)));

    // Test input mode key processing
    state.ui.show_input = true;
    state.ui.input_content = "test".to_string();
    let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    let msgs = home.process_key(key, &state);
    // Currently returns empty since key processing is handled by translator
    assert!(msgs.is_empty());

    Ok(())
}

#[test]
fn test_elm_home_advanced_interaction_validation() -> Result<()> {
    let home = ElmHome::new();
    let mut state = create_test_state();

    // Test interaction with empty timeline
    assert!(!home.can_interact(&state));
    assert!(home.get_selected_note(&state).is_none());

    // Test interaction validation with timeline
    let event = create_test_event();
    let sortable = nostui::domain::nostr::SortableEvent::new(event.clone());
    state
        .timeline
        .notes
        .find_or_insert(std::cmp::Reverse(sortable));
    state.timeline.selected_index = Some(0);

    assert!(home.can_interact(&state));
    assert!(home.get_selected_note(&state).is_some());

    // Test interaction blocked by input mode
    state.ui.show_input = true;
    assert!(!home.can_interact(&state));

    Ok(())
}

#[test]
fn test_elm_home_translator_integration() -> Result<()> {
    let mut state = create_test_state();
    let event = create_test_event();

    // Setup timeline with event
    let sortable = nostui::domain::nostr::SortableEvent::new(event.clone());
    state
        .timeline
        .notes
        .find_or_insert(std::cmp::Reverse(sortable));
    state.timeline.selected_index = Some(0);

    // Test like key translation
    let key = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE);
    let msgs = translate_raw_to_domain(RawMsg::Key(key), &state);
    assert_eq!(msgs.len(), 1);
    match &msgs[0] {
        Msg::SendReaction(reaction_event) => {
            assert_eq!(reaction_event.id, event.id);
        }
        _ => panic!("Expected SendReaction message"),
    }

    // Test reply key translation
    let key = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
    let msgs = translate_raw_to_domain(RawMsg::Key(key), &state);
    assert!(!msgs.is_empty());
    assert!(msgs.iter().any(|msg| matches!(msg, Msg::ShowReply(_))));

    // Test repost key translation
    let key = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE);
    let msgs = translate_raw_to_domain(RawMsg::Key(key), &state);
    assert_eq!(msgs.len(), 1);
    match &msgs[0] {
        Msg::SendRepost(repost_event) => {
            assert_eq!(repost_event.id, event.id);
        }
        _ => panic!("Expected SendRepost message"),
    }

    Ok(())
}

#[test]
fn test_elm_home_validation_edge_cases() -> Result<()> {
    let mut state = create_test_state();

    // Test own note repost prevention
    let keys = Keys::generate();
    state.user.current_user_pubkey = keys.public_key();

    let mut own_event = create_test_event();
    own_event.pubkey = keys.public_key(); // Make it user's own event

    let sortable = nostui::domain::nostr::SortableEvent::new(own_event.clone());
    state
        .timeline
        .notes
        .find_or_insert(std::cmp::Reverse(sortable));
    state.timeline.selected_index = Some(0);

    // Attempt to repost own note
    let key = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE);
    let msgs = translate_raw_to_domain(RawMsg::Key(key), &state);
    assert_eq!(msgs.len(), 1);
    match &msgs[0] {
        Msg::UpdateStatusMessage(msg) => {
            assert!(msg.contains("Cannot repost your own note"));
        }
        _ => panic!("Expected status message about own note repost"),
    }

    Ok(())
}

#[test]
fn test_elm_home_help_text_contextual() -> Result<()> {
    let home = ElmHome::new();
    let mut state = create_test_state();

    // Empty timeline help
    let help = home.get_help_text(&state);
    assert!(help.contains("New note"));
    assert!(help.contains("first note"));

    // With timeline but no selection
    let event = create_test_event();
    let sortable = nostui::domain::nostr::SortableEvent::new(event);
    state
        .timeline
        .notes
        .find_or_insert(std::cmp::Reverse(sortable));

    let help = home.get_help_text(&state);
    assert!(help.contains("Navigate"));

    // With timeline and selection
    state.timeline.selected_index = Some(0);
    let help = home.get_help_text(&state);
    assert!(help.contains("Like"));
    assert!(help.contains("Reply"));
    assert!(help.contains("Repost"));

    // Input mode
    state.ui.show_input = true;
    let help = home.get_help_text(&state);
    assert!(help.contains("Send note"));
    assert!(help.contains("Cancel"));

    // Reply mode
    state.ui.reply_to = Some(create_test_event());
    let help = home.get_help_text(&state);
    assert!(help.contains("Send reply"));

    Ok(())
}

#[test]
fn test_elm_home_component_reset() -> Result<()> {
    let mut home = ElmHome::new();
    let state = create_test_state();

    // Verify initial state
    let initial_status = home.get_status_info(&state);

    // Simulate some usage (this doesn't modify the component much, but demonstrates the interface)
    let _actions = home.get_available_actions(&state);
    let _help = home.get_help_text(&state);

    // Reset component
    home.reset();

    // Verify state is reset (status should be the same for empty state)
    let reset_status = home.get_status_info(&state);
    assert_eq!(initial_status.timeline_count, reset_status.timeline_count);
    assert_eq!(initial_status.selected_index, reset_status.selected_index);
    assert_eq!(initial_status.input_mode, reset_status.input_mode);

    Ok(())
}
