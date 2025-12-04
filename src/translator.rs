use crossterm::event::{KeyCode, KeyModifiers};
use nostr_sdk::prelude::*;

use crate::{msg::Msg, raw_msg::RawMsg, state::AppState};

/// Translates raw external events into domain messages
/// This function is pure and contains no side effects
pub fn translate_raw_to_domain(raw: RawMsg, state: &AppState) -> Vec<Msg> {
    match raw {
        // System events - direct mapping
        RawMsg::Quit => vec![Msg::Quit],
        RawMsg::Suspend => vec![Msg::Suspend],
        RawMsg::Resume => vec![Msg::Resume],
        RawMsg::Resize(width, height) => vec![Msg::Resize(width, height)],

        // User input - translate based on context and key bindings
        RawMsg::Key(key) => translate_key_event(key, state),

        // Network events - translate based on event type
        RawMsg::ReceiveEvent(event) => translate_nostr_event(event),

        // FPS updates
        RawMsg::AppFpsUpdate(fps) => vec![Msg::UpdateAppFps(fps)],
        RawMsg::RenderFpsUpdate(fps) => vec![Msg::UpdateRenderFps(fps)],

        // System events
        RawMsg::Error(error) => vec![Msg::ShowError(error)],

        // Ignore frequent system events in domain layer
        RawMsg::Tick | RawMsg::Render => vec![],
    }
}

/// Translates keyboard input to domain events based on current application state
fn translate_key_event(key: crossterm::event::KeyEvent, state: &AppState) -> Vec<Msg> {
    use crossterm::event::KeyEvent;

    // Handle global key bindings first
    match key {
        KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => return vec![Msg::Quit],

        KeyEvent {
            code: KeyCode::Char('z'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => return vec![Msg::Suspend],

        _ => {}
    }

    // Context-sensitive key bindings
    if state.ui.show_input {
        translate_input_mode_keys(key)
    } else {
        translate_normal_mode_keys(key, state)
    }
}

/// Key bindings when input is active
fn translate_input_mode_keys(key: crossterm::event::KeyEvent) -> Vec<Msg> {
    use crossterm::event::KeyEvent;

    match key {
        KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            ..
        } => vec![Msg::SubmitNote],

        KeyEvent {
            code: KeyCode::Esc, ..
        } => vec![Msg::CancelInput],

        // All other keys should update input content
        // We pass the key event to be processed by ElmHomeInput
        _ => vec![Msg::ProcessInputKey(key)],
    }
}

/// Key bindings when in normal navigation mode
fn translate_normal_mode_keys(key: crossterm::event::KeyEvent, state: &AppState) -> Vec<Msg> {
    use crossterm::event::KeyEvent;

    match key {
        // Vim-style navigation
        KeyEvent {
            code: KeyCode::Char('j'),
            modifiers: KeyModifiers::NONE,
            ..
        }
        | KeyEvent {
            code: KeyCode::Down,
            ..
        } => vec![Msg::ScrollDown],

        KeyEvent {
            code: KeyCode::Char('k'),
            modifiers: KeyModifiers::NONE,
            ..
        }
        | KeyEvent {
            code: KeyCode::Up, ..
        } => vec![Msg::ScrollUp],

        KeyEvent {
            code: KeyCode::Char('g'),
            modifiers: KeyModifiers::NONE,
            ..
        } => vec![Msg::ScrollToTop],

        KeyEvent {
            code: KeyCode::Char('G'),
            modifiers: KeyModifiers::SHIFT,
            ..
        } => vec![Msg::ScrollToBottom],

        // Post interactions
        KeyEvent {
            code: KeyCode::Char('n'),
            modifiers: KeyModifiers::NONE,
            ..
        } => vec![Msg::ShowNewNote],

        KeyEvent {
            code: KeyCode::Char('r'),
            modifiers: KeyModifiers::NONE,
            ..
        } => {
            // Reply to selected note
            if let Some(selected_note) = state.selected_note() {
                vec![Msg::ShowReply(selected_note.clone())]
            } else {
                vec![]
            }
        }

        KeyEvent {
            code: KeyCode::Char('l'),
            modifiers: KeyModifiers::NONE,
            ..
        } => {
            // Like/react to selected note
            if let Some(selected_note) = state.selected_note() {
                vec![Msg::SendReaction(selected_note.clone())]
            } else {
                vec![]
            }
        }

        KeyEvent {
            code: KeyCode::Char('t'),
            modifiers: KeyModifiers::NONE,
            ..
        } => {
            // Repost selected note
            if let Some(selected_note) = state.selected_note() {
                vec![Msg::SendRepost(selected_note.clone())]
            } else {
                vec![]
            }
        }

        // UI toggles
        KeyEvent {
            code: KeyCode::Char('i'),
            modifiers: KeyModifiers::NONE,
            ..
        } => vec![Msg::ToggleInput],

        KeyEvent {
            code: KeyCode::Esc, ..
        } => vec![Msg::ClearStatusMessage],

        _ => vec![], // Unknown keys are ignored
    }
}

/// Translates Nostr events into domain events
fn translate_nostr_event(event: Event) -> Vec<Msg> {
    match event.kind {
        Kind::Metadata => {
            // Parse metadata and update profile
            if let Ok(metadata) = Metadata::from_json(event.content.clone()) {
                let profile = crate::nostr::Profile::new(event.pubkey, event.created_at, metadata);
                vec![Msg::UpdateProfile(event.pubkey, profile)]
            } else {
                vec![Msg::ShowError(
                    "Failed to parse profile metadata".to_string(),
                )]
            }
        }

        Kind::TextNote => vec![Msg::AddNote(event)],

        Kind::Reaction => vec![Msg::AddReaction(event)],

        Kind::Repost => vec![Msg::AddRepost(event)],

        Kind::ZapReceipt => vec![Msg::AddZapReceipt(event)],

        _ => {
            // Unknown event types are logged but not processed
            vec![Msg::UpdateStatusMessage(format!(
                "Received unknown event type: {}",
                event.kind
            ))]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
    fn test_translate_system_events() {
        let state = create_test_state();

        let result = translate_raw_to_domain(RawMsg::Quit, &state);
        assert_eq!(result, vec![Msg::Quit]);

        let result = translate_raw_to_domain(RawMsg::Suspend, &state);
        assert_eq!(result, vec![Msg::Suspend]);

        let result = translate_raw_to_domain(RawMsg::Resize(100, 50), &state);
        assert_eq!(result, vec![Msg::Resize(100, 50)]);
    }

    #[test]
    fn test_translate_navigation_keys() {
        let state = create_test_state();

        // Test vim-style navigation
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::ScrollDown]);

        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::ScrollUp]);

        // Test arrow keys
        let key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::ScrollDown]);

        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::ScrollUp]);
    }

    #[test]
    fn test_translate_global_keys() {
        let state = create_test_state();

        // Test Ctrl+C
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::Quit]);

        // Test Ctrl+Z
        let key = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::Suspend]);
    }

    #[test]
    fn test_translate_input_mode_keys() {
        let mut state = create_test_state();
        state.ui.show_input = true;

        // Test Enter in input mode
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::SubmitNote]);

        // Test Escape in input mode
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::CancelInput]);
    }

    #[test]
    fn test_translate_post_interaction_keys() {
        let mut state = create_test_state();
        let event = create_test_event();

        // Add event to timeline and select it
        let sortable = crate::nostr::SortableEvent::new(event.clone());
        state
            .timeline
            .notes
            .find_or_insert(std::cmp::Reverse(sortable));
        state.timeline.selected_index = Some(0);

        // Test like key
        let key = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::SendReaction(event.clone())]);

        // Test reply key
        let key = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::ShowReply(event.clone())]);
    }

    #[test]
    fn test_translate_nostr_events() {
        let _state = create_test_state();

        // Test text note
        let event = create_test_event();
        let result = translate_raw_to_domain(RawMsg::ReceiveEvent(event.clone()), &_state);
        assert_eq!(result, vec![Msg::AddNote(event)]);

        // Test metadata event
        let keys = Keys::generate();
        let metadata = Metadata::new().name("Test User");
        let metadata_event = EventBuilder::metadata(&metadata)
            .sign_with_keys(&keys)
            .unwrap();

        let result = translate_raw_to_domain(RawMsg::ReceiveEvent(metadata_event.clone()), &_state);
        assert_eq!(result.len(), 1);
        match &result[0] {
            Msg::UpdateProfile(pubkey, _) => {
                assert_eq!(*pubkey, keys.public_key());
            }
            _ => panic!("Expected UpdateProfile message"),
        }
    }

    #[test]
    fn test_translate_frequent_events_ignored() {
        let state = create_test_state();

        let result = translate_raw_to_domain(RawMsg::Tick, &state);
        assert!(result.is_empty());

        let result = translate_raw_to_domain(RawMsg::Render, &state);
        assert!(result.is_empty());
    }

    #[test]
    fn test_translate_unknown_keys_ignored() {
        let state = create_test_state();

        let key = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert!(result.is_empty());
    }
}
