use std::collections::VecDeque;
use tokio::sync::mpsc;

use crate::{
    cmd::Cmd, msg::Msg, raw_msg::RawMsg, state::AppState, translator::translate_raw_to_domain,
    update::update,
};

/// Integration point between Elm architecture runtime and existing app
/// Allows testing new architecture without affecting existing code
pub struct ElmRuntime {
    state: AppState,
    msg_queue: VecDeque<Msg>,
    raw_msg_queue: VecDeque<RawMsg>,
    cmd_queue: VecDeque<Cmd>,
    msg_tx: Option<mpsc::UnboundedSender<Msg>>,
    msg_rx: mpsc::UnboundedReceiver<Msg>,
    raw_msg_tx: Option<mpsc::UnboundedSender<RawMsg>>,
    raw_msg_rx: mpsc::UnboundedReceiver<RawMsg>,
}

impl ElmRuntime {
    /// Create a new ElmRuntime
    pub fn new(initial_state: AppState) -> Self {
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let (raw_msg_tx, raw_msg_rx) = mpsc::unbounded_channel();

        Self {
            state: initial_state,
            msg_queue: VecDeque::new(),
            raw_msg_queue: VecDeque::new(),
            cmd_queue: VecDeque::new(),
            msg_tx: Some(msg_tx),
            msg_rx,
            raw_msg_tx: Some(raw_msg_tx),
            raw_msg_rx,
        }
    }

    /// Get sender for message transmission
    pub fn get_sender(&self) -> Option<mpsc::UnboundedSender<Msg>> {
        self.msg_tx.clone()
    }

    /// Get current state (read-only)
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Send message directly (for testing)
    pub fn send_msg(&mut self, msg: Msg) {
        self.msg_queue.push_back(msg);
    }

    /// Send raw message (for integration with external systems)
    pub fn send_raw_msg(&mut self, raw_msg: RawMsg) {
        self.raw_msg_queue.push_back(raw_msg);
    }

    /// Get raw message sender
    pub fn get_raw_sender(&self) -> Option<mpsc::UnboundedSender<RawMsg>> {
        self.raw_msg_tx.clone()
    }

    /// Get pending commands
    pub fn pending_commands(&mut self) -> Vec<Cmd> {
        let mut commands = Vec::new();
        while let Some(cmd) = self.cmd_queue.pop_front() {
            commands.push(cmd);
        }
        commands
    }

    /// Process a single message
    pub fn process_message(&mut self, msg: Msg) -> Vec<Cmd> {
        let (new_state, commands) = update(msg, self.state.clone());
        self.state = new_state;

        // Add commands to queue
        for cmd in &commands {
            self.cmd_queue.push_back(cmd.clone());
        }

        commands
    }

    /// Process all messages in queue
    pub fn process_all_messages(&mut self) -> Vec<Cmd> {
        let mut all_commands = Vec::new();

        // First process raw messages and convert to domain messages
        while let Some(raw_msg) = self.raw_msg_queue.pop_front() {
            let domain_msgs = translate_raw_to_domain(raw_msg, &self.state);
            for msg in domain_msgs {
                self.msg_queue.push_back(msg);
            }
        }

        // Process raw messages from external sources
        while let Ok(raw_msg) = self.raw_msg_rx.try_recv() {
            let domain_msgs = translate_raw_to_domain(raw_msg, &self.state);
            for msg in domain_msgs {
                self.msg_queue.push_back(msg);
            }
        }

        // Process domain messages in internal queue
        while let Some(msg) = self.msg_queue.pop_front() {
            let commands = self.process_message(msg);
            all_commands.extend(commands);
        }

        // Process domain messages from external sources
        while let Ok(msg) = self.msg_rx.try_recv() {
            let commands = self.process_message(msg);
            all_commands.extend(commands);
        }

        all_commands
    }

    /// Get runtime statistics
    pub fn get_stats(&self) -> ElmRuntimeStats {
        ElmRuntimeStats {
            queued_messages: self.msg_queue.len(),
            queued_commands: self.cmd_queue.len(),
            timeline_notes_count: self.state.timeline_len(),
            profiles_count: self.state.user.profiles.len(),
            is_input_shown: self.state.ui.show_input,
            selected_note_index: self.state.timeline.selected_index,
        }
    }
}

/// Runtime statistics
#[derive(Debug, Clone)]
pub struct ElmRuntimeStats {
    pub queued_messages: usize,
    pub queued_commands: usize,
    pub timeline_notes_count: usize,
    pub profiles_count: usize,
    pub is_input_shown: bool,
    pub selected_note_index: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::*;

    fn create_test_runtime() -> ElmRuntime {
        let keys = Keys::generate();
        let state = AppState::new(keys.public_key());
        ElmRuntime::new(state)
    }

    fn create_test_event() -> Event {
        let keys = Keys::generate();
        EventBuilder::text_note("test content")
            .sign_with_keys(&keys)
            .unwrap()
    }

    #[test]
    fn test_elm_runtime_creation() {
        let runtime = create_test_runtime();
        let stats = runtime.get_stats();

        assert_eq!(stats.queued_messages, 0);
        assert_eq!(stats.queued_commands, 0);
        assert_eq!(stats.timeline_notes_count, 0);
        assert!(!stats.is_input_shown);
    }

    #[test]
    fn test_send_message() {
        let mut runtime = create_test_runtime();

        runtime.send_msg(Msg::ShowNewNote);
        let stats = runtime.get_stats();
        assert_eq!(stats.queued_messages, 1);

        let commands = runtime.process_all_messages();
        let new_stats = runtime.get_stats();

        assert_eq!(new_stats.queued_messages, 0);
        assert!(new_stats.is_input_shown);
        assert!(commands.is_empty()); // ShowNewNote doesn't generate commands
    }

    #[test]
    fn test_process_message() {
        let mut runtime = create_test_runtime();

        let commands = runtime.process_message(Msg::Quit);
        assert!(commands.is_empty());
        assert!(runtime.state().system.should_quit);
    }

    #[test]
    fn test_process_scroll_messages() {
        let mut runtime = create_test_runtime();

        // Add event to timeline
        let event = create_test_event();
        runtime.process_message(Msg::AddNote(event));

        // Test scroll operations
        runtime.process_message(Msg::ScrollDown);
        assert_eq!(runtime.state().timeline.selected_index, Some(0));

        runtime.process_message(Msg::ScrollUp);
        assert_eq!(runtime.state().timeline.selected_index, Some(0)); // No change as it's at the top
    }

    #[test]
    fn test_send_reaction_command() {
        let mut runtime = create_test_runtime();
        let target_event = create_test_event();

        let commands = runtime.process_message(Msg::SendReaction(target_event.clone()));
        assert_eq!(commands.len(), 1);

        match &commands[0] {
            Cmd::SendReaction {
                target_event: cmd_event,
            } => {
                assert_eq!(cmd_event, &target_event);
            }
            _ => panic!("Expected SendReaction command"),
        }

        // Check if status message is set
        assert!(runtime.state().system.status_message.is_some());
    }

    #[test]
    fn test_input_workflow() {
        let mut runtime = create_test_runtime();

        // Start new post
        runtime.process_message(Msg::ShowNewNote);
        assert!(runtime.state().ui.show_input);
        assert!(runtime.state().ui.reply_to.is_none());

        // Update input content
        let content = "Hello, Nostr!";
        runtime.process_message(Msg::UpdateInputContent(content.to_string()));
        assert_eq!(runtime.state().ui.input_content, content);

        // Submit post
        let commands = runtime.process_message(Msg::SubmitNote);
        assert_eq!(commands.len(), 1);

        match &commands[0] {
            Cmd::SendTextNote {
                content: cmd_content,
                ..
            } => {
                assert_eq!(cmd_content, content);
            }
            _ => panic!("Expected SendTextNote command"),
        }

        // Check if UI is reset
        assert!(!runtime.state().ui.show_input);
        assert!(runtime.state().ui.input_content.is_empty());
    }

    #[test]
    fn test_reply_workflow() {
        let mut runtime = create_test_runtime();
        let target_event = create_test_event();

        // Start reply
        runtime.process_message(Msg::ShowReply(target_event.clone()));
        assert!(runtime.state().ui.show_input);
        assert_eq!(runtime.state().ui.reply_to, Some(target_event));

        // Cancel input
        runtime.process_message(Msg::CancelInput);
        assert!(!runtime.state().ui.show_input);
        assert!(runtime.state().ui.reply_to.is_none());
    }

    #[test]
    fn test_receive_events() {
        let mut runtime = create_test_runtime();

        // Receive text note
        let text_event = create_test_event();
        runtime.process_message(Msg::AddNote(text_event));
        assert_eq!(runtime.state().timeline_len(), 1);

        // Receive metadata event
        let keys = Keys::generate();
        let metadata = Metadata::new()
            .name("Test User")
            .display_name("Test Display Name");
        let metadata_event = EventBuilder::metadata(&metadata)
            .sign_with_keys(&keys)
            .unwrap();

        let profile =
            crate::nostr::Profile::new(keys.public_key(), metadata_event.created_at, metadata);
        runtime.process_message(Msg::UpdateProfile(keys.public_key(), profile));
        assert!(runtime
            .state()
            .user
            .profiles
            .contains_key(&keys.public_key()));
    }

    #[test]
    fn test_external_message_channel() {
        let mut runtime = create_test_runtime();
        let sender = runtime.get_sender().unwrap();

        // Send messages from external source
        sender.send(Msg::ShowNewNote).unwrap();
        sender
            .send(Msg::UpdateInputContent("test".to_string()))
            .unwrap();

        // Not processed yet
        assert!(!runtime.state().ui.show_input);

        // Process all messages
        let commands = runtime.process_all_messages();

        // State has been updated
        assert!(runtime.state().ui.show_input);
        assert_eq!(runtime.state().ui.input_content, "test");
        assert!(commands.is_empty());
    }

    #[test]
    fn test_pending_commands() {
        let mut runtime = create_test_runtime();
        let target_event = create_test_event();

        // Send messages that generate commands
        runtime.process_message(Msg::SendReaction(target_event.clone()));
        runtime.process_message(Msg::SendRepost(target_event));

        // Get pending commands
        let pending = runtime.pending_commands();
        assert_eq!(pending.len(), 2);

        // Getting them again returns empty
        let pending2 = runtime.pending_commands();
        assert!(pending2.is_empty());
    }
}
