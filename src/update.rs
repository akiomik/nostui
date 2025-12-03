use nostr_sdk::prelude::*;
use std::cmp::Reverse;

use crate::{
    cmd::Cmd,
    msg::Msg,
    nostr::{Profile, SortableEvent},
    state::AppState,
};

/// Elm-like update function
/// Returns new state and list of commands from current state and message
pub fn update(msg: Msg, mut state: AppState) -> (AppState, Vec<Cmd>) {
    match msg {
        // System messages
        Msg::Quit => {
            state.system.should_quit = true;
            (state, vec![])
        }

        Msg::Suspend => {
            state.system.should_suspend = true;
            (state, vec![])
        }

        Msg::Resume => {
            state.system.should_suspend = false;
            (state, vec![])
        }

        Msg::Resize(width, height) => (state, vec![Cmd::Resize { width, height }]),

        // Timeline operations
        Msg::ScrollUp => {
            if !state.ui.show_input && !state.timeline_is_empty() {
                let new_index = match state.timeline.selected_index {
                    Some(i) if i > 0 => Some(i - 1),
                    Some(_) => Some(0),
                    None => Some(0),
                };
                state.timeline.selected_index = new_index;
            }
            (state, vec![])
        }

        Msg::ScrollDown => {
            if !state.ui.show_input && !state.timeline_is_empty() {
                let max_index = state.timeline_len().saturating_sub(1);
                let new_index = match state.timeline.selected_index {
                    Some(i) if i < max_index => Some(i + 1),
                    Some(_) => Some(max_index),
                    None => Some(0),
                };
                state.timeline.selected_index = new_index;
            }
            (state, vec![])
        }

        Msg::ScrollToTop => {
            if !state.ui.show_input && !state.timeline_is_empty() {
                state.timeline.selected_index = Some(0);
            }
            (state, vec![])
        }

        Msg::ScrollToBottom => {
            if !state.ui.show_input && !state.timeline_is_empty() {
                state.timeline.selected_index = Some(state.timeline_len().saturating_sub(1));
            }
            (state, vec![])
        }

        Msg::SelectNote(index) => {
            state.timeline.selected_index = index;
            (state, vec![])
        }

        // Nostr event processing
        Msg::ReceiveEvent(event) => update_receive_event(event, state),

        Msg::SendReaction(target_event) => {
            let cmd = Cmd::SendReaction {
                target_event: target_event.clone(),
            };
            let note1 = target_event.id.to_bech32().unwrap_or_default();
            state.system.status_message = Some(format!("[Liked] {}", note1));
            (state, vec![cmd])
        }

        Msg::SendRepost(target_event) => {
            let cmd = Cmd::SendRepost {
                target_event: target_event.clone(),
            };
            let note1 = target_event.id.to_bech32().unwrap_or_default();
            state.system.status_message = Some(format!("[Reposted] {}", note1));
            (state, vec![cmd])
        }

        Msg::SendTextNote(content, tags) => {
            let cmd = Cmd::SendTextNote {
                content: content.clone(),
                tags,
            };
            state.system.status_message = Some(format!("[Posted] {}", content));
            (state, vec![cmd])
        }

        // UI operations
        Msg::ShowNewNote => {
            state.ui.reply_to = None;
            state.ui.show_input = true;
            state.ui.input_content.clear();
            (state, vec![])
        }

        Msg::ShowReply(target_event) => {
            state.ui.reply_to = Some(target_event);
            state.ui.show_input = true;
            state.ui.input_content.clear();
            (state, vec![])
        }

        Msg::ToggleInput => {
            state.ui.show_input = !state.ui.show_input;
            if !state.ui.show_input {
                state.ui.reply_to = None;
                state.ui.input_content.clear();
                state.timeline.selected_index = None;
            }
            (state, vec![])
        }

        Msg::CancelInput => {
            state.ui.show_input = false;
            state.ui.reply_to = None;
            state.ui.input_content.clear();
            state.timeline.selected_index = None;
            (state, vec![])
        }

        Msg::UpdateInputContent(content) => {
            state.ui.input_content = content;
            (state, vec![])
        }

        Msg::SubmitNote => {
            if state.ui.show_input && !state.ui.input_content.is_empty() {
                let content = state.ui.input_content.clone();
                let tags = if let Some(ref _reply_to) = state.ui.reply_to {
                    // TODO: Reply tags generation should be moved here or as a separate function
                    vec![]
                } else {
                    vec![]
                };

                state.ui.show_input = false;
                state.ui.reply_to = None;
                state.ui.input_content.clear();

                (state, vec![Cmd::SendTextNote { content, tags }])
            } else {
                (state, vec![])
            }
        }

        // Status updates
        Msg::UpdateStatusMessage(message) => {
            state.system.status_message = Some(message);
            (state, vec![])
        }

        Msg::ClearStatusMessage => {
            state.system.status_message = None;
            (state, vec![])
        }

        Msg::SetLoading(loading) => {
            state.system.is_loading = loading;
            (state, vec![])
        }

        // FPS updates
        Msg::UpdateAppFps(fps) => {
            state.system.fps_data.app_fps = fps;
            state.system.fps_data.app_frames += 1;
            (state, vec![])
        }

        Msg::UpdateRenderFps(fps) => {
            state.system.fps_data.render_fps = fps;
            state.system.fps_data.render_frames += 1;
            (state, vec![])
        }

        // Profile updates
        Msg::UpdateProfile(pubkey, profile) => {
            // Update only if newer than existing profile
            let should_update = state
                .user
                .profiles
                .get(&pubkey)
                .is_none_or(|existing| existing.created_at < profile.created_at);

            if should_update {
                state.user.profiles.insert(pubkey, profile);
            }
            (state, vec![])
        }

        // Error
        Msg::Error(error) => {
            state.system.status_message = Some(format!("Error: {}", error));
            (state, vec![])
        }

        // Others (not implemented yet)
        Msg::Tick | Msg::Render | Msg::Key(_) => (state, vec![]),
    }
}

/// State update when receiving Nostr events
fn update_receive_event(event: Event, state: AppState) -> (AppState, Vec<Cmd>) {
    match event.kind {
        Kind::Metadata => {
            if let Ok(metadata) = Metadata::from_json(event.content.clone()) {
                let profile = Profile::new(event.pubkey, event.created_at, metadata);
                let msg = Msg::UpdateProfile(event.pubkey, profile);
                update(msg, state)
            } else {
                (state, vec![])
            }
        }

        Kind::TextNote => update_add_note(event, state),

        Kind::Reaction => update_add_reaction(event, state),

        Kind::Repost => update_add_repost(event, state),

        Kind::ZapReceipt => update_add_zap_receipt(event, state),

        _ => (state, vec![]),
    }
}

/// Add note to timeline
fn update_add_note(event: Event, mut state: AppState) -> (AppState, Vec<Cmd>) {
    let sortable_event = SortableEvent::new(event);
    let note = Reverse(sortable_event);

    state.timeline.notes.find_or_insert(note);

    // Adjust selection position (new note was added)
    if let Some(selected) = state.timeline.selected_index {
        state.timeline.selected_index = Some(selected + 1);
    }

    (state, vec![])
}

/// Add reaction
fn update_add_reaction(reaction: Event, mut state: AppState) -> (AppState, Vec<Cmd>) {
    if let Some(event_id) = extract_last_event_id(&reaction) {
        state
            .timeline
            .reactions
            .entry(event_id)
            .or_default()
            .insert(reaction);
    }
    (state, vec![])
}

/// Add repost
fn update_add_repost(repost: Event, mut state: AppState) -> (AppState, Vec<Cmd>) {
    if let Some(event_id) = extract_last_event_id(&repost) {
        state
            .timeline
            .reposts
            .entry(event_id)
            .or_default()
            .insert(repost);
    }
    (state, vec![])
}

/// Add zap receipt
fn update_add_zap_receipt(zap_receipt: Event, mut state: AppState) -> (AppState, Vec<Cmd>) {
    if let Some(event_id) = extract_last_event_id(&zap_receipt) {
        state
            .timeline
            .zap_receipts
            .entry(event_id)
            .or_default()
            .insert(zap_receipt);
    }
    (state, vec![])
}

/// Helper function to extract event_id from the last e tag of an event
fn extract_last_event_id(event: &Event) -> Option<EventId> {
    use nostr_sdk::nostr::{Alphabet, SingleLetterTag, TagKind, TagStandard};

    event
        .tags
        .iter()
        .filter(|tag| tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E)))
        .next_back()
        .and_then(|tag| {
            if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized() {
                Some(*event_id)
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_update_quit() {
        let state = create_test_state();
        let (new_state, cmds) = update(Msg::Quit, state);

        assert!(new_state.system.should_quit);
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_update_scroll_up() {
        let mut state = create_test_state();
        // With empty timeline, selection index remains unchanged
        state.timeline.selected_index = Some(5);

        let (new_state, cmds) = update(Msg::ScrollUp, state);

        // No change due to empty timeline
        assert_eq!(new_state.timeline.selected_index, Some(5));
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_update_scroll_down() {
        let mut state = create_test_state();
        state.timeline.selected_index = Some(3);
        // With empty timeline, selection index doesn't change

        let (new_state, _cmds) = update(Msg::ScrollDown, state);

        // No change due to empty timeline
        assert_eq!(new_state.timeline.selected_index, Some(3));
    }

    #[test]
    fn test_update_show_new_note() {
        let state = create_test_state();
        let (new_state, cmds) = update(Msg::ShowNewNote, state);

        assert!(new_state.ui.show_input);
        assert!(new_state.ui.reply_to.is_none());
        assert!(new_state.ui.input_content.is_empty());
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_update_show_reply() {
        let state = create_test_state();
        let target_event = create_test_event();
        let (new_state, cmds) = update(Msg::ShowReply(target_event.clone()), state);

        assert!(new_state.ui.show_input);
        assert_eq!(new_state.ui.reply_to, Some(target_event));
        assert!(new_state.ui.input_content.is_empty());
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_update_cancel_input() {
        let mut state = create_test_state();
        state.ui.show_input = true;
        state.ui.input_content = "test".to_string();
        state.ui.reply_to = Some(create_test_event());

        let (new_state, cmds) = update(Msg::CancelInput, state);

        assert!(!new_state.ui.show_input);
        assert!(new_state.ui.reply_to.is_none());
        assert!(new_state.ui.input_content.is_empty());
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_update_send_reaction() {
        let state = create_test_state();
        let target_event = create_test_event();
        let (new_state, cmds) = update(Msg::SendReaction(target_event.clone()), state);

        assert!(new_state.system.status_message.is_some());
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::SendReaction {
                target_event: cmd_event,
            } => {
                assert_eq!(cmd_event, &target_event);
            }
            _ => panic!("Expected SendReaction command"),
        }
    }

    #[test]
    fn test_update_receive_text_note() {
        let state = create_test_state();
        let event = create_test_event();
        let (new_state, cmds) = update(Msg::ReceiveEvent(event), state);

        assert_eq!(new_state.timeline.notes.len(), 1);
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_update_input_content() {
        let state = create_test_state();
        let content = "Hello, world!";
        let (new_state, cmds) = update(Msg::UpdateInputContent(content.to_string()), state);

        assert_eq!(new_state.ui.input_content, content);
        assert!(cmds.is_empty());
    }
}
