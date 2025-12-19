use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nostr_sdk::prelude::*;

use crate::core::msg::nostr::NostrMsg;
use crate::presentation::config::keybindings::Action;
use crate::{
    core::{
        msg::{system::SystemMsg, timeline::TimelineMsg, ui::UiMsg, Msg},
        raw_msg::RawMsg,
        state::AppState,
    },
    domain::nostr::Profile,
};

/// Translates raw external events into domain messages
/// This function is pure and contains no side effects
pub fn translate_raw_to_domain(raw: RawMsg, state: &AppState) -> Vec<Msg> {
    match raw {
        // System events - direct mapping
        RawMsg::Quit => vec![Msg::System(SystemMsg::Quit)],
        RawMsg::Suspend => vec![Msg::System(SystemMsg::Suspend)],
        RawMsg::Resume => vec![Msg::System(SystemMsg::Resume)],
        RawMsg::Resize(width, height) => vec![Msg::System(SystemMsg::Resize(width, height))],

        // User input - translate based on context and key bindings
        RawMsg::Key(key) => translate_key_event(key, state),

        // Network events - translate based on event type
        RawMsg::ReceiveEvent(event) => translate_nostr_event(event),

        // FPS updates
        RawMsg::AppFpsUpdate(fps) => vec![Msg::System(SystemMsg::UpdateAppFps(fps))],
        RawMsg::RenderFpsUpdate(fps) => vec![Msg::System(SystemMsg::UpdateRenderFps(fps))],

        // System events
        RawMsg::SystemMessage(msg) => vec![Msg::System(SystemMsg::UpdateStatusMessage(msg))],
        RawMsg::Error(error) => vec![Msg::System(SystemMsg::ShowError(error))],

        // Ignore frequent system events in domain layer
        RawMsg::Tick | RawMsg::Render => vec![],
    }
}

/// Translates keyboard input to domain events based on current application state
fn translate_key_event(key: KeyEvent, state: &AppState) -> Vec<Msg> {
    // Handle global key bindings first
    match key {
        KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => return vec![Msg::System(SystemMsg::Quit)],

        KeyEvent {
            code: KeyCode::Char('z'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => return vec![Msg::System(SystemMsg::Suspend)],

        _ => {}
    }

    // Context-sensitive key bindings
    if state.ui.is_composing() {
        translate_input_mode_keys(key, state)
    } else {
        translate_normal_mode_keys(key, state)
    }
}

/// Key bindings when input is active
fn translate_input_mode_keys(key: KeyEvent, state: &AppState) -> Vec<Msg> {
    use crossterm::event::KeyEvent;

    match key {
        KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => vec![Msg::Ui(UiMsg::SubmitNote)],

        KeyEvent {
            code: KeyCode::Esc, ..
        } => {
            // In input mode, always cancel input
            if state.ui.is_composing() {
                vec![Msg::Ui(UiMsg::CancelInput)]
            } else {
                // In normal mode, use keybinding configuration
                translate_normal_mode_keys(key, state)
            }
        }

        // Hybrid Approach: Delegate all other input to TextArea component
        // All non-special keys go to TextArea for processing
        _ => vec![Msg::Ui(UiMsg::ProcessTextAreaInput(key))],
    }
}

/// Key bindings when in normal navigation mode
fn translate_normal_mode_keys(key: KeyEvent, state: &AppState) -> Vec<Msg> {
    // Get keybindings from config state (flat mapping)
    if let Some(action) = state.config.config.keybindings.get(&vec![key]) {
        return translate_action_to_msg(action, state);
    }

    vec![] // No matching keybinding found
}

fn translate_action_to_msg(action: &Action, state: &AppState) -> Vec<Msg> {
    match action {
        Action::ScrollUp => vec![Msg::Timeline(TimelineMsg::ScrollUp)],
        Action::ScrollDown => vec![Msg::Timeline(TimelineMsg::ScrollDown)],
        Action::ScrollToTop => vec![Msg::Timeline(TimelineMsg::ScrollToTop)],
        Action::ScrollToBottom => vec![Msg::Timeline(TimelineMsg::ScrollToBottom)],
        Action::NewTextNote => vec![Msg::Ui(UiMsg::ShowNewNote)],
        Action::ReplyTextNote => translate_reply_key(state),
        Action::React => translate_like_key(state),
        Action::Repost => translate_repost_key(state),
        Action::Unselect => vec![Msg::Timeline(TimelineMsg::DeselectNote)],
        Action::Quit => vec![Msg::System(SystemMsg::Quit)],
        Action::Suspend => vec![Msg::System(SystemMsg::Suspend)],
        Action::SubmitTextNote => {
            // Only process submit in input mode
            if state.ui.is_composing() {
                vec![Msg::Ui(UiMsg::SubmitNote)]
            } else {
                vec![]
            }
        }
    }
}

/// Translate reply key with validation
fn translate_reply_key(state: &AppState) -> Vec<Msg> {
    if !can_interact_with_timeline(state) {
        return vec![Msg::System(SystemMsg::UpdateStatusMessage(
            "Cannot reply: No note selected or input mode active".to_string(),
        ))];
    }

    if let Some(selected_note) = state.selected_note() {
        if selected_note.pubkey == state.user.current_user_pubkey {
            vec![Msg::System(SystemMsg::UpdateStatusMessage(
                "Cannot reply to your own note".to_string(),
            ))]
        } else {
            vec![
                Msg::Ui(UiMsg::ShowReply(selected_note.clone())),
                Msg::System(SystemMsg::UpdateStatusMessage(format!(
                    "Replying to {}...",
                    get_display_name(selected_note, state)
                ))),
            ]
        }
    } else {
        vec![Msg::System(SystemMsg::UpdateStatusMessage(
            "No note selected for reply".to_string(),
        ))]
    }
}

/// Translate like key with duplicate prevention
fn translate_like_key(state: &AppState) -> Vec<Msg> {
    if !can_interact_with_timeline(state) {
        return vec![Msg::System(SystemMsg::UpdateStatusMessage(
            "Cannot react: No note selected or input mode active".to_string(),
        ))];
    }

    if let Some(selected_note) = state.selected_note() {
        if has_user_reacted_to_note(selected_note, state) {
            vec![Msg::System(SystemMsg::UpdateStatusMessage(
                "You have already liked this note".to_string(),
            ))]
        } else {
            {
                let note1 = selected_note.id.to_bech32().unwrap_or_default();
                vec![
                    Msg::System(SystemMsg::UpdateStatusMessage(format!("[Liked] {note1}"))),
                    Msg::Nostr(NostrMsg::SendReaction(selected_note.clone())),
                ]
            }
        }
    } else {
        vec![Msg::System(SystemMsg::UpdateStatusMessage(
            "No note selected for reaction".to_string(),
        ))]
    }
}

/// Translate repost key with validation
fn translate_repost_key(state: &AppState) -> Vec<Msg> {
    if !can_interact_with_timeline(state) {
        return vec![Msg::System(SystemMsg::UpdateStatusMessage(
            "Cannot repost: No note selected or input mode active".to_string(),
        ))];
    }

    if let Some(selected_note) = state.selected_note() {
        if selected_note.pubkey == state.user.current_user_pubkey {
            vec![Msg::System(SystemMsg::UpdateStatusMessage(
                "Cannot repost your own note".to_string(),
            ))]
        } else if has_user_reposted_note(selected_note, state) {
            vec![Msg::System(SystemMsg::UpdateStatusMessage(
                "You have already reposted this note".to_string(),
            ))]
        } else {
            {
                let note1 = selected_note.id.to_bech32().unwrap_or_default();
                vec![
                    Msg::System(SystemMsg::UpdateStatusMessage(format!(
                        "[Reposted] {note1}",
                    ))),
                    Msg::Nostr(NostrMsg::SendRepost(selected_note.clone())),
                ]
            }
        }
    } else {
        vec![Msg::System(SystemMsg::UpdateStatusMessage(
            "No note selected for repost".to_string(),
        ))]
    }
}

/// Helper: Check if user can interact with timeline
fn can_interact_with_timeline(state: &AppState) -> bool {
    !state.ui.is_composing()
        && state.timeline.selected_index.is_some()
        && !state.timeline_is_empty()
}

/// Helper: Check if user has already reacted to a note
fn has_user_reacted_to_note(note: &Event, state: &AppState) -> bool {
    state
        .timeline
        .reactions
        .get(&note.id)
        .is_some_and(|reactions| {
            reactions
                .iter()
                .any(|reaction| reaction.pubkey == state.user.current_user_pubkey)
        })
}

/// Helper: Check if user has already reposted a note
fn has_user_reposted_note(note: &Event, state: &AppState) -> bool {
    state.timeline.reposts.get(&note.id).is_some_and(|reposts| {
        reposts
            .iter()
            .any(|repost| repost.pubkey == state.user.current_user_pubkey)
    })
}

/// Helper: Get display name for a note's author
fn get_display_name(note: &Event, state: &AppState) -> String {
    state
        .user
        .profiles
        .get(&note.pubkey)
        .and_then(|profile| profile.metadata.name.as_ref())
        .cloned()
        .unwrap_or_else(|| note.pubkey.to_string()[0..8].to_string())
}

/// Translates Nostr events into domain events
fn translate_nostr_event(event: Event) -> Vec<Msg> {
    // Receiving any Nostr event implies that initial loading has progressed
    // Clear the loading indicator on first event reception
    let mut msgs = vec![Msg::System(SystemMsg::SetLoading(false))];
    match event.kind {
        Kind::Metadata => {
            // Parse metadata and update profile
            if let Ok(metadata) = Metadata::from_json(event.content.clone()) {
                let profile = Profile::new(event.pubkey, event.created_at, metadata);
                msgs.push(Msg::UpdateProfile(event.pubkey, profile));
            } else {
                msgs.push(Msg::System(SystemMsg::ShowError(
                    "Failed to parse profile metadata".to_string(),
                )));
            }
        }

        Kind::TextNote => msgs.push(Msg::Timeline(TimelineMsg::AddNote(event))),

        Kind::Reaction => msgs.push(Msg::Timeline(TimelineMsg::AddReaction(event))),

        Kind::Repost => msgs.push(Msg::Timeline(TimelineMsg::AddRepost(event))),

        Kind::ZapReceipt => msgs.push(Msg::Timeline(TimelineMsg::AddZapReceipt(event))),

        _ => {
            // Unknown event types are logged but not processed
            msgs.push(Msg::System(SystemMsg::UpdateStatusMessage(format!(
                "Received unknown event type: {}",
                event.kind
            ))));
        }
    }
    msgs
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cmp::Reverse;
    use std::collections::HashMap;

    use crate::core::state::ui::UiMode;
    use crate::domain::collections::EventSet;
    use crate::domain::nostr::SortableEvent;
    use crate::infrastructure::config::Config;
    use crate::presentation::config::keybindings::KeyBindings;

    fn create_test_state() -> AppState {
        // Create config with test keybindings
        let mut config = Config::default();

        // Create test keybindings that match the expected behavior
        let mut home_bindings = HashMap::new();
        home_bindings.insert(
            vec![KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)],
            Action::ScrollDown,
        );
        home_bindings.insert(
            vec![KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE)],
            Action::ScrollUp,
        );
        home_bindings.insert(
            vec![KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)],
            Action::ScrollDown,
        );
        home_bindings.insert(
            vec![KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)],
            Action::ScrollUp,
        );
        home_bindings.insert(
            vec![KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)],
            Action::ScrollToTop,
        );
        home_bindings.insert(
            vec![KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT)],
            Action::ScrollToBottom,
        );
        home_bindings.insert(
            vec![KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE)],
            Action::React,
        );
        home_bindings.insert(
            vec![KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)],
            Action::ReplyTextNote,
        );
        home_bindings.insert(
            vec![KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE)],
            Action::Repost,
        );
        home_bindings.insert(
            vec![KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)],
            Action::NewTextNote,
        );
        home_bindings.insert(
            vec![KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)],
            Action::Unselect,
        );

        config.keybindings = KeyBindings(home_bindings);

        AppState::new_with_config(Keys::generate().public_key(), config)
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
        assert_eq!(result, vec![Msg::System(SystemMsg::Quit)]);

        let result = translate_raw_to_domain(RawMsg::Suspend, &state);
        assert_eq!(result, vec![Msg::System(SystemMsg::Suspend)]);

        let result = translate_raw_to_domain(RawMsg::Resize(100, 50), &state);
        assert_eq!(result, vec![Msg::System(SystemMsg::Resize(100, 50))]);
    }

    #[test]
    fn test_translate_navigation_keys() {
        let state = create_test_state();

        // Test vim-style navigation
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::Timeline(TimelineMsg::ScrollDown)]);

        // Test Escape key in normal mode (should use keybinding configuration)
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::Timeline(TimelineMsg::DeselectNote)]);

        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::Timeline(TimelineMsg::ScrollUp)]);

        // Test arrow keys
        let key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::Timeline(TimelineMsg::ScrollDown)]);

        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::Timeline(TimelineMsg::ScrollUp)]);
    }

    #[test]
    fn test_translate_global_keys() {
        let state = create_test_state();

        // Test Ctrl+C
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::System(SystemMsg::Quit)]);

        // Test Ctrl+Z
        let key = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::System(SystemMsg::Suspend)]);
    }

    #[test]
    fn test_translate_input_mode_keys() {
        let mut state = create_test_state();
        state.ui.current_mode = UiMode::Composing;

        // Test Ctrl+P in input mode (submit)
        let key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::Ui(UiMsg::SubmitNote)]);

        // Test plain Enter in input mode (now delegated to TextArea)
        state.ui.input_content = "Test".to_string();
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::Ui(UiMsg::ProcessTextAreaInput(key))]);

        // Test Escape in input mode
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::Ui(UiMsg::CancelInput)]);

        // Test character input (now delegated to TextArea)
        state.ui.input_content = "Hello".to_string();
        let key = KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::Ui(UiMsg::ProcessTextAreaInput(key))]);

        // Test backspace (now delegated to TextArea)
        let key = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::Ui(UiMsg::ProcessTextAreaInput(key))]);

        // Test Shift+Enter (now delegated to TextArea)
        state.ui.input_content = "Line 1".to_string();
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::Ui(UiMsg::ProcessTextAreaInput(key))]);
    }

    #[test]
    fn test_translate_post_interaction_keys() {
        let mut state = create_test_state();
        let event = create_test_event();

        // Add event to timeline and select it
        let sortable = SortableEvent::new(event.clone());
        state.timeline.notes.find_or_insert(Reverse(sortable));
        state.timeline.selected_index = Some(0);

        // Test like key
        let key = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result.len(), 2);
        match (&result[0], &result[1]) {
            (
                Msg::System(SystemMsg::UpdateStatusMessage(msg)),
                Msg::Nostr(NostrMsg::SendReaction(ev)),
            ) => {
                assert!(msg.contains("[Liked]"));
                assert_eq!(ev.id, event.id);
            }
            _ => panic!("Expected status then SendReaction"),
        }

        // Test reply key
        let key = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result.len(), 2);
        match (&result[0], &result[1]) {
            (
                Msg::Ui(UiMsg::ShowReply(reply_event)),
                Msg::System(SystemMsg::UpdateStatusMessage(msg)),
            ) => {
                assert_eq!(reply_event.id, event.id);
                assert!(msg.contains("Replying to"));
            }
            _ => panic!("Expected ShowReply and UpdateStatusMessage"),
        }
    }

    #[test]
    fn test_translate_nostr_events() {
        let _state = create_test_state();

        // Test text note
        let event = create_test_event();
        let result = translate_raw_to_domain(RawMsg::ReceiveEvent(event.clone()), &_state);
        assert_eq!(
            result,
            vec![
                Msg::System(SystemMsg::SetLoading(false)),
                Msg::Timeline(TimelineMsg::AddNote(event))
            ]
        );

        // Test metadata event
        let keys = Keys::generate();
        let metadata = Metadata::new().name("Test User");
        let metadata_event = EventBuilder::metadata(&metadata)
            .sign_with_keys(&keys)
            .unwrap();

        let result = translate_raw_to_domain(RawMsg::ReceiveEvent(metadata_event), &_state);
        assert_eq!(result.len(), 2);
        match (&result[0], &result[1]) {
            (Msg::System(SystemMsg::SetLoading(false)), Msg::UpdateProfile(pubkey, _)) => {
                assert_eq!(*pubkey, keys.public_key());
            }
            _ => panic!("Expected SetLoading(false) then UpdateProfile message"),
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

    #[test]
    fn test_translate_reply_key_validation() {
        let mut state = create_test_state();

        // Cannot reply when in input mode - 'r' should be delegated to TextArea
        state.ui.current_mode = UiMode::Composing;
        state.ui.input_content = "Hello".to_string();
        state.timeline.selected_index = Some(0);
        let key = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result, vec![Msg::Ui(UiMsg::ProcessTextAreaInput(key))]);

        // Cannot reply when no note selected
        state.ui.current_mode = UiMode::Normal;
        state.timeline.selected_index = None;
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result.len(), 1);
        match &result[0] {
            Msg::System(SystemMsg::UpdateStatusMessage(msg)) => {
                assert!(msg.contains("Cannot reply"))
            }
            _ => panic!("Expected status message"),
        }
    }

    #[test]
    fn test_translate_like_key_duplicate_prevention() {
        let mut state = create_test_state();
        let event = create_test_event();

        // Add event to timeline and select it
        let sortable = SortableEvent::new(event.clone());
        state.timeline.notes.find_or_insert(Reverse(sortable));
        state.timeline.selected_index = Some(0);

        // First like should work
        let key = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result.len(), 2);
        match (&result[0], &result[1]) {
            (
                Msg::System(SystemMsg::UpdateStatusMessage(msg)),
                Msg::Nostr(NostrMsg::SendReaction(_)),
            ) => {
                assert!(msg.contains("[Liked]"));
            }
            _ => panic!("Expected status then SendReaction"),
        }

        // Simulate user has already reacted
        let reaction_keys = Keys::generate();
        let reaction = EventBuilder::reaction(&event, "+")
            .sign_with_keys(&reaction_keys)
            .unwrap();
        let mut reaction_with_user_key = reaction;
        reaction_with_user_key.pubkey = state.user.current_user_pubkey;

        state.timeline.reactions.insert(event.id, {
            let mut set = EventSet::new();
            set.insert(reaction_with_user_key);
            set
        });

        // Second like should be prevented
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result.len(), 1);
        match &result[0] {
            Msg::System(SystemMsg::UpdateStatusMessage(msg)) => {
                assert!(msg.contains("already liked"))
            }
            _ => panic!("Expected status message about duplicate like"),
        }
    }

    #[test]
    fn test_translate_repost_key_own_note_prevention() {
        let mut state = create_test_state();

        // Make the event authored by the current user
        let mut user_event = create_test_event();
        user_event.pubkey = state.user.current_user_pubkey;

        // Add user's own event to timeline and select it
        let sortable = SortableEvent::new(user_event);
        state.timeline.notes.find_or_insert(Reverse(sortable));
        state.timeline.selected_index = Some(0);

        // Attempt to repost own note should be prevented
        let key = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE);
        let result = translate_raw_to_domain(RawMsg::Key(key), &state);
        assert_eq!(result.len(), 1);
        match &result[0] {
            Msg::System(SystemMsg::UpdateStatusMessage(msg)) => {
                assert!(msg.contains("Cannot repost your own note"))
            }
            _ => panic!("Expected status message about own note repost"),
        }
    }

    #[test]
    fn test_can_interact_with_timeline_helper() {
        let mut state = create_test_state();

        // Cannot interact when input is showing
        state.ui.current_mode = UiMode::Composing;
        state.timeline.selected_index = Some(0);
        assert!(!can_interact_with_timeline(&state));

        // Cannot interact when no note selected
        state.ui.current_mode = UiMode::Normal;
        state.timeline.selected_index = None;
        assert!(!can_interact_with_timeline(&state));

        // Cannot interact when timeline is empty (even with selection)
        state.timeline.selected_index = Some(0);
        assert!(!can_interact_with_timeline(&state)); // timeline is empty

        // Can interact when conditions are met
        let event = create_test_event();
        let sortable = SortableEvent::new(event);
        state.timeline.notes.find_or_insert(Reverse(sortable));
        assert!(can_interact_with_timeline(&state));
    }

    #[test]
    fn test_get_display_name_helper() {
        let mut state = create_test_state();
        let event = create_test_event();

        // Without profile - should return truncated pubkey
        let name = get_display_name(&event, &state);
        assert_eq!(name.len(), 8);
        assert_eq!(name, event.pubkey.to_string()[0..8]);

        // With profile - should return profile name
        let metadata = Metadata::new().name("Test User");
        let profile = Profile::new(event.pubkey, event.created_at, metadata);
        state.user.profiles.insert(event.pubkey, profile);

        let name = get_display_name(&event, &state);
        assert_eq!(name, "Test User");
    }
}
