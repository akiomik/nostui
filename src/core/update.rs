use nostr_sdk::prelude::*;

use crate::{
    core::cmd::Cmd,
    core::msg::{system::SystemMsg, timeline::TimelineMsg, ui::UiMsg, user::UserMsg, Msg},
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

        // User messages (delegated to UserState)
        Msg::User(user_msg) => {
            let commands = state.user.update(user_msg);
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

        // New UI path (delegates to existing behavior for now)
        Msg::Ui(ui_msg) => {
            match ui_msg {
                UiMsg::SubmitNote => {
                    if let Some(submit_data) = crate::presentation::components::elm_home_input::ElmHomeInput::get_submit_data(&state) {
                        // Reset UiState through its update to centralize behavior
                        let mut cmds = state.ui.update(UiMsg::CancelInput);
                        cmds.push(Cmd::SendTextNote { content: submit_data.content, tags: submit_data.tags });
                        (
                            state,
                            cmds
                        )
                    } else {
                        (state, vec![])
                    }
                }
                UiMsg::ProcessTextAreaInput(key) => {
                    if state.ui.show_input {
                        state.ui.pending_input_keys.push(key);
                        let textarea_state = crate::presentation::components::elm_home_input::ElmHomeInput::process_pending_keys(&mut state);
                        textarea_state.apply_to_ui_state(&mut state.ui);
                    }
                    (state, vec![])
                }
                ref other => {
                    let cancel = matches!(other, UiMsg::CancelInput);
                    let mut cmds = state.ui.update(other.clone());
                    if cancel {
                        let tl_cmds = state.timeline.update(TimelineMsg::DeselectNote);
                        cmds.extend(tl_cmds);
                    }
                    (state, cmds)
                }
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

        // Legacy user messages (backward compatibility - to be phased out)
        Msg::UpdateProfile(pubkey, profile) => {
            let commands = state.user.update(UserMsg::UpdateProfile(pubkey, profile));
            (state, commands)
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
        let (new_state, cmds) = update(Msg::Ui(UiMsg::ShowNewNote), state);

        assert!(new_state.ui.show_input);
        assert!(new_state.ui.reply_to.is_none());
        assert!(new_state.ui.input_content.is_empty());
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_update_show_reply() {
        let state = create_test_state();
        let target_event = create_test_event();
        let (new_state, cmds) = update(Msg::Ui(UiMsg::ShowReply(target_event.clone())), state);

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

        let (new_state, cmds) = update(Msg::Ui(UiMsg::CancelInput), state);

        assert!(!new_state.ui.show_input);
        assert!(new_state.ui.reply_to.is_none());
        assert!(new_state.ui.input_content.is_empty());
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_ui_cancel_input_delegates_to_timeline_and_keeps_status_message() {
        let mut state = create_test_state();
        // Prepare UI state to be reset by CancelInput
        state.ui.show_input = true;
        state.ui.input_content = "typing...".into();
        state.ui.reply_to = Some(create_test_event());
        // Prepare timeline selection and a system status message
        state.timeline.selected_index = Some(3);
        state.system.status_message = Some("keep me".into());

        let (new_state, cmds) = update(Msg::Ui(UiMsg::CancelInput), state);

        // UiState was reset by UiState::update
        assert!(!new_state.ui.show_input);
        assert!(new_state.ui.reply_to.is_none());
        assert!(new_state.ui.input_content.is_empty());

        // Coordinator delegated to TimelineMsg::DeselectNote and aggregated commands (currently none)
        assert_eq!(new_state.timeline.selected_index, None);
        assert!(cmds.is_empty());

        // Unlike Msg::DeselectNote path, status_message is not cleared here (policy difference)
        assert_eq!(new_state.system.status_message.as_deref(), Some("keep me"));
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
        let (new_state, cmds) = update(
            Msg::Ui(UiMsg::UpdateInputContent(content.to_string())),
            state,
        );

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
