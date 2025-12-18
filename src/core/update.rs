use crate::{
    core::cmd::Cmd,
    core::msg::{timeline::TimelineMsg, ui::UiMsg, user::UserMsg, Msg},
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

        // User messages (delegated to UserState)
        Msg::User(user_msg) => {
            let commands = state.user.update(user_msg);
            (state, commands)
        }

        // Timeline messages (delegated to TimelineState)
        Msg::Timeline(timeline_msg) => {
            // When composing, ignore scroll-related timeline msgs
            if state.ui.is_composing() {
                match timeline_msg {
                    TimelineMsg::ScrollUp
                    | TimelineMsg::ScrollDown
                    | TimelineMsg::ScrollToTop
                    | TimelineMsg::ScrollToBottom => {
                        return (state, vec![]);
                    }
                    _ => {}
                }
            }

            let commands = match timeline_msg {
                TimelineMsg::DeselectNote => {
                    let cmds = state.timeline.update(TimelineMsg::DeselectNote);
                    // Clear system status message when explicitly deselecting via TimelineMsg
                    state.system.status_message = None;
                    cmds
                }
                other => state.timeline.update(other),
            };
            (state, commands)
        }

        // Nostr operations (delegated via NostrState)
        Msg::Nostr(nostr_msg) => {
            let commands = state.nostr.update(nostr_msg);
            (state, commands)
        }

        // New UI path (delegates to existing behavior for now)
        Msg::Ui(ui_msg) => {
            match ui_msg {
                UiMsg::SubmitNote => {
                    if let Some(submit_data) =
                        crate::presentation::components::home_input::HomeInput::get_submit_data(
                            &state,
                        )
                    {
                        // Reset UiState through its update to centralize behavior
                        let mut cmds = state.ui.update(UiMsg::CancelInput);
                        cmds.push(Cmd::SendTextNote {
                            content: submit_data.content,
                            tags: submit_data.tags,
                        });
                        (state, cmds)
                    } else {
                        (state, vec![])
                    }
                }
                UiMsg::ProcessTextAreaInput(key) => {
                    if state.ui.is_composing() {
                        state.ui.pending_input_keys.push(key);
                        let textarea_state = crate::presentation::components::home_input::HomeInput::process_pending_keys(&mut state);
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
    use nostr_sdk::prelude::*;

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

        let (new_state, cmds) = update(Msg::Timeline(TimelineMsg::ScrollUp), state);

        // No change due to empty timeline
        assert_eq!(new_state.timeline.selected_index, Some(5));
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_update_scroll_down() {
        let mut state = create_test_state();
        state.timeline.selected_index = Some(3);
        // With empty timeline, selection index doesn't change

        let (new_state, _cmds) = update(Msg::Timeline(TimelineMsg::ScrollDown), state);

        // No change due to empty timeline
        assert_eq!(new_state.timeline.selected_index, Some(3));
    }

    #[test]
    fn test_update_show_new_note() {
        let state = create_test_state();
        let (new_state, cmds) = update(Msg::Ui(UiMsg::ShowNewNote), state);

        assert!(new_state.ui.is_composing());
        assert!(new_state.ui.reply_to.is_none());
        assert!(new_state.ui.input_content.is_empty());
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_update_show_reply() {
        let state = create_test_state();
        let target_event = create_test_event();
        let (new_state, cmds) = update(Msg::Ui(UiMsg::ShowReply(target_event.clone())), state);

        assert!(new_state.ui.is_composing());
        assert_eq!(new_state.ui.reply_to, Some(target_event));
        assert!(new_state.ui.input_content.is_empty());
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_update_cancel_input() {
        let mut state = create_test_state();
        state.ui.current_mode = crate::core::state::ui::UiMode::Composing;
        state.ui.input_content = "test".to_string();
        state.ui.reply_to = Some(create_test_event());

        let (new_state, cmds) = update(Msg::Ui(UiMsg::CancelInput), state);

        assert!(new_state.ui.is_normal());
        assert!(new_state.ui.reply_to.is_none());
        assert!(new_state.ui.input_content.is_empty());
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_ui_cancel_input_delegates_to_timeline_and_keeps_status_message() {
        let mut state = create_test_state();
        // Prepare UI state to be reset by CancelInput
        state.ui.current_mode = crate::core::state::ui::UiMode::Composing;
        state.ui.input_content = "typing...".into();
        state.ui.reply_to = Some(create_test_event());
        // Prepare timeline selection and a system status message
        state.timeline.selected_index = Some(3);
        state.system.status_message = Some("keep me".into());

        let (new_state, cmds) = update(Msg::Ui(UiMsg::CancelInput), state);

        // UiState was reset by UiState::update
        assert!(new_state.ui.is_normal());
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
        let (new_state, cmds) = update(
            Msg::Nostr(crate::core::msg::nostr::NostrMsg::SendReaction(
                target_event.clone(),
            )),
            state,
        );

        assert!(new_state.system.status_message.is_none());
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
        let (new_state, cmds) = update(Msg::Timeline(TimelineMsg::AddNote(event)), state);

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
        let (new_state, cmds) = update(Msg::Timeline(TimelineMsg::SelectNote(3)), state);

        assert_eq!(new_state.timeline.selected_index, Some(3));
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_update_deselect_note() {
        let mut state = create_test_state();
        state.timeline.selected_index = Some(5);
        state.system.status_message = Some("test status".to_string());

        let (new_state, cmds) = update(Msg::Timeline(TimelineMsg::DeselectNote), state);

        assert_eq!(new_state.timeline.selected_index, None);
        assert_eq!(new_state.system.status_message, None);
        assert!(cmds.is_empty());
    }
}
