use nostr_sdk::prelude::*;

use crate::{
    core::cmd::Cmd,
    core::msg::{system::SystemMsg, timeline::TimelineMsg, Msg},
    core::state::AppState,
};

/// Elm-like update function
/// Returns new state and list of commands from current state and message
pub fn update(msg: Msg, mut state: AppState) -> (AppState, Vec<Cmd>) {
    match msg {
        // System messages (delegated to SystemState)
        Msg::System(system_msg) => {
            let commands = state.system.update(system_msg);
            (state, commands)
        }

        // Timeline messages (delegated to TimelineState)
        Msg::Timeline(timeline_msg) => {
            let commands = state.timeline.update(timeline_msg);
            (state, commands)
        }

        // Legacy timeline operations (backward compatibility - to be phased out)
        Msg::ScrollUp => {
            if !state.ui.show_input {
                let commands = state.timeline.update(TimelineMsg::ScrollUp);
                (state, commands)
            } else {
                (state, vec![])
            }
        }

        Msg::ScrollDown => {
            if !state.ui.show_input {
                let commands = state.timeline.update(TimelineMsg::ScrollDown);
                (state, commands)
            } else {
                (state, vec![])
            }
        }

        Msg::ScrollToTop => {
            if !state.ui.show_input {
                let commands = state.timeline.update(TimelineMsg::ScrollToTop);
                (state, commands)
            } else {
                (state, vec![])
            }
        }

        Msg::ScrollToBottom => {
            if !state.ui.show_input {
                let commands = state.timeline.update(TimelineMsg::ScrollToBottom);
                (state, commands)
            } else {
                (state, vec![])
            }
        }

        Msg::SelectNote(index) => {
            let commands = state.timeline.update(TimelineMsg::SelectNote(index));
            (state, commands)
        }

        Msg::DeselectNote => {
            let commands = state.timeline.update(TimelineMsg::DeselectNote);
            // Also clear system status message for legacy compatibility
            state.system.status_message = None;
            (state, commands)
        }

        // Legacy Nostr domain events (backward compatibility - to be phased out)
        Msg::AddNote(event) => {
            let commands = state.timeline.update(TimelineMsg::AddNote(event));
            (state, commands)
        }

        Msg::AddReaction(reaction) => {
            let commands = state.timeline.update(TimelineMsg::AddReaction(reaction));
            (state, commands)
        }

        Msg::AddRepost(repost) => {
            let commands = state.timeline.update(TimelineMsg::AddRepost(repost));
            (state, commands)
        }

        Msg::AddZapReceipt(zap_receipt) => {
            let commands = state
                .timeline
                .update(TimelineMsg::AddZapReceipt(zap_receipt));
            (state, commands)
        }

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
            log::info!(
                "update.rs: Processing Msg::SendTextNote - content: '{}', tags: {:?}",
                content,
                tags
            );
            let cmd = Cmd::SendTextNote {
                content: content.clone(),
                tags,
            };
            state.system.status_message = Some(format!("[Posted] {}", content));
            log::info!("update.rs: Generated Cmd::SendTextNote, returning command");
            (state, vec![cmd])
        }

        // UI operations
        Msg::ShowNewNote => {
            state.ui.reply_to = None;
            state.ui.show_input = true;
            state.ui.input_content.clear();
            state.ui.cursor_position = Default::default();
            state.ui.selection = None;
            (state, vec![])
        }

        Msg::ShowReply(target_event) => {
            state.ui.reply_to = Some(target_event);
            state.ui.show_input = true;
            state.ui.input_content.clear();
            state.ui.cursor_position = Default::default();
            state.ui.selection = None;
            (state, vec![])
        }

        Msg::CancelInput => {
            state.ui.show_input = false;
            state.ui.reply_to = None;
            state.ui.input_content.clear();
            state.ui.cursor_position = Default::default();
            state.ui.selection = None;
            state.timeline.selected_index = None;
            (state, vec![])
        }

        Msg::UpdateInputContent(content) => {
            state.ui.input_content = content;
            (state, vec![])
        }

        Msg::UpdateInputContentWithCursor(content, cursor_pos) => {
            state.ui.input_content = content;
            state.ui.cursor_position = cursor_pos;
            (state, vec![])
        }

        Msg::UpdateCursorPosition(cursor_pos) => {
            state.ui.cursor_position = cursor_pos;
            (state, vec![])
        }

        Msg::UpdateSelection(selection) => {
            state.ui.selection = selection;
            (state, vec![])
        }

        // Process TextArea input using pending_keys approach for state consistency
        Msg::ProcessTextAreaInput(key) => {
            if state.ui.show_input {
                // Add key to pending queue
                state.ui.pending_input_keys.push(key);

                // Process all pending keys and extract new state
                let textarea_state =
                    crate::presentation::components::elm_home_input::ElmHomeInput::process_pending_keys(&mut state);

                // Update AppState with processed results
                textarea_state.apply_to_ui_state(&mut state.ui);
            }
            (state, vec![])
        }

        Msg::SubmitNote => {
            // Use ElmHomeInput logic for submission
            if let Some(submit_data) =
                crate::presentation::components::elm_home_input::ElmHomeInput::get_submit_data(
                    &state,
                )
            {
                state.ui.show_input = false;
                state.ui.reply_to = None;
                state.ui.input_content.clear();
                state.ui.cursor_position = Default::default();
                state.ui.selection = None;

                (
                    state,
                    vec![Cmd::SendTextNote {
                        content: submit_data.content,
                        tags: submit_data.tags,
                    }],
                )
            } else {
                (state, vec![])
            }
        }

        // Legacy system messages (backward compatibility - to be phased out)
        Msg::UpdateStatusMessage(message) => {
            let commands = state.system.update(SystemMsg::UpdateStatusMessage(message));
            (state, commands)
        }

        Msg::ClearStatusMessage => {
            let commands = state.system.update(SystemMsg::ClearStatusMessage);
            (state, commands)
        }

        Msg::SetLoading(loading) => {
            let commands = state.system.update(SystemMsg::SetLoading(loading));
            (state, commands)
        }

        Msg::UpdateAppFps(fps) => {
            let commands = state.system.update(SystemMsg::UpdateAppFps(fps));
            (state, commands)
        }

        Msg::UpdateRenderFps(fps) => {
            let commands = state.system.update(SystemMsg::UpdateRenderFps(fps));
            (state, commands)
        }

        Msg::ShowError(error) => {
            let commands = state.system.update(SystemMsg::ShowError(error));
            (state, commands)
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
    }
}

// Timeline-related helper functions moved to src/core/state/timeline.rs

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
        let (new_state, cmds) = update(
            Msg::System(crate::core::msg::system::SystemMsg::Quit),
            state,
        );

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
    fn test_update_add_text_note() {
        let state = create_test_state();
        let event = create_test_event();
        let (new_state, cmds) = update(Msg::AddNote(event), state);

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

    #[test]
    fn test_update_select_note() {
        let state = create_test_state();
        let (new_state, cmds) = update(Msg::SelectNote(3), state);

        assert_eq!(new_state.timeline.selected_index, Some(3));
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_update_deselect_note() {
        let mut state = create_test_state();
        state.timeline.selected_index = Some(5);
        state.system.status_message = Some("test status".to_string());

        let (new_state, cmds) = update(Msg::DeselectNote, state);

        assert_eq!(new_state.timeline.selected_index, None);
        assert_eq!(new_state.system.status_message, None);
        assert!(cmds.is_empty());
    }
}
