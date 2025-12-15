use std::collections::VecDeque;
use tokio::sync::mpsc;

use crate::{
    core::cmd::Cmd, core::cmd_executor::CmdExecutor, core::msg::Msg, core::raw_msg::RawMsg,
    core::state::AppState, core::translator::translate_raw_to_domain, core::update::update,
    infrastructure::nostr_command::NostrCommand, integration::legacy::action::Action,
};

/// Integration point between Elm architecture runtime and existing app
pub struct ElmRuntime {
    state: AppState,
    msg_queue: VecDeque<Msg>,
    raw_msg_queue: VecDeque<RawMsg>,
    cmd_queue: VecDeque<Cmd>,
    msg_tx: Option<mpsc::UnboundedSender<Msg>>,
    msg_rx: mpsc::UnboundedReceiver<Msg>,
    raw_msg_tx: Option<mpsc::UnboundedSender<RawMsg>>,
    raw_msg_rx: mpsc::UnboundedReceiver<RawMsg>,
    cmd_executor: Option<CmdExecutor>,
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
            cmd_executor: None,
        }
    }

    /// Create a new ElmRuntime with command executor
    pub fn new_with_executor(
        initial_state: AppState,
        action_sender: mpsc::UnboundedSender<Action>,
    ) -> Self {
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let (raw_msg_tx, raw_msg_rx) = mpsc::unbounded_channel();
        let executor = CmdExecutor::new(action_sender);

        Self {
            state: initial_state,
            msg_queue: VecDeque::new(),
            raw_msg_queue: VecDeque::new(),
            cmd_queue: VecDeque::new(),
            msg_tx: Some(msg_tx),
            msg_rx,
            raw_msg_tx: Some(raw_msg_tx),
            raw_msg_rx,
            cmd_executor: Some(executor),
        }
    }

    /// Create a new ElmRuntime with both Action and NostrCommand support
    pub fn new_with_nostr_executor(
        initial_state: AppState,
        action_sender: mpsc::UnboundedSender<Action>,
        nostr_sender: mpsc::UnboundedSender<NostrCommand>,
    ) -> Self {
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let (raw_msg_tx, raw_msg_rx) = mpsc::unbounded_channel();
        let executor = CmdExecutor::new_with_nostr(action_sender, nostr_sender);

        Self {
            state: initial_state,
            msg_queue: VecDeque::new(),
            raw_msg_queue: VecDeque::new(),
            cmd_queue: VecDeque::new(),
            msg_tx: Some(msg_tx),
            msg_rx,
            raw_msg_tx: Some(raw_msg_tx),
            raw_msg_rx,
            cmd_executor: Some(executor),
        }
    }

    /// Set command executor (Action only)
    pub fn set_executor(&mut self, action_sender: mpsc::UnboundedSender<Action>) {
        self.cmd_executor = Some(CmdExecutor::new(action_sender));
    }

    /// Set command executor with NostrCommand support
    pub fn set_nostr_executor(
        &mut self,
        action_sender: mpsc::UnboundedSender<Action>,
        nostr_sender: mpsc::UnboundedSender<NostrCommand>,
    ) {
        self.cmd_executor = Some(CmdExecutor::new_with_nostr(action_sender, nostr_sender));
    }

    /// Add NostrCommand support to existing executor
    pub fn add_nostr_support(
        &mut self,
        nostr_sender: mpsc::UnboundedSender<NostrCommand>,
    ) -> Result<(), String> {
        if let Some(executor) = &mut self.cmd_executor {
            executor.set_nostr_sender(nostr_sender);
            Ok(())
        } else {
            Err(
                "No executor available. Use set_executor() or set_nostr_executor() first."
                    .to_string(),
            )
        }
    }

    /// Get sender for message transmission
    pub fn get_sender(&self) -> Option<mpsc::UnboundedSender<Msg>> {
        self.msg_tx.clone()
    }

    /// Add TUI command sender support to existing executor (for TuiCommand execution)
    pub fn add_tui_sender(
        &mut self,
        tui_sender: mpsc::UnboundedSender<crate::core::cmd::TuiCommand>,
    ) -> Result<(), String> {
        if let Some(executor) = &mut self.cmd_executor {
            executor.set_tui_sender(tui_sender);
            Ok(())
        } else {
            Err(
                "No executor available. Use set_executor() or set_nostr_executor() first."
                    .to_string(),
            )
        }
    }

    /// Add render request sender for orchestrated rendering in AppRunner
    pub fn add_render_request_sender(
        &mut self,
        render_sender: mpsc::UnboundedSender<()>,
    ) -> Result<(), String> {
        if let Some(executor) = &mut self.cmd_executor {
            executor.set_render_request_sender(render_sender);
            Ok(())
        } else {
            Err(
                "No executor available. Use set_executor() or set_nostr_executor() first."
                    .to_string(),
            )
        }
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

    /// Execute all pending commands using the command executor
    pub fn execute_pending_commands(&mut self) -> Result<Vec<String>, String> {
        if self.cmd_executor.is_none() {
            return Err(
                "No command executor available. Use set_executor() to configure.".to_string(),
            );
        }

        let commands = self.pending_commands();
        if commands.is_empty() {
            return Ok(vec![]);
        }

        // Now safely get the executor
        let executor = self.cmd_executor.as_ref().unwrap();
        executor
            .execute_commands(&commands)
            .map_err(|e| format!("Command execution failed: {}", e))
    }

    /// Execute a single command immediately
    pub fn execute_command(&self, cmd: &Cmd) -> Result<(), String> {
        if let Some(executor) = &self.cmd_executor {
            executor
                .execute_command(cmd)
                .map_err(|e| format!("Command execution failed: {}", e))
        } else {
            Err("No command executor available. Use set_executor() to configure.".to_string())
        }
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

    /// Process all messages and execute commands in one step
    pub fn run_update_cycle(&mut self) -> Result<Vec<String>, String> {
        let _commands = self.process_all_messages();
        self.execute_pending_commands()
    }

    /// Get runtime statistics
    pub fn get_stats(&self) -> ElmRuntimeStats {
        let has_nostr_support = self
            .cmd_executor
            .as_ref()
            .map(|executor| executor.get_stats().has_nostr_sender)
            .unwrap_or(false);

        ElmRuntimeStats {
            queued_messages: self.msg_queue.len(),
            queued_commands: self.cmd_queue.len(),
            timeline_notes_count: self.state.timeline_len(),
            profiles_count: self.state.user.profiles.len(),
            is_input_shown: self.state.ui.show_input,
            selected_note_index: self.state.timeline.selected_index,
            has_executor: self.cmd_executor.is_some(),
            has_nostr_support,
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
    pub has_executor: bool,
    pub has_nostr_support: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::msg::timeline::TimelineMsg;
    use crate::core::msg::ui::UiMsg;
    use crate::core::msg::Msg;
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

        runtime.send_msg(Msg::Ui(UiMsg::ShowNewNote));
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

        let commands =
            runtime.process_message(Msg::System(crate::core::msg::system::SystemMsg::Quit));
        assert!(commands.is_empty());
        assert!(runtime.state().system.should_quit);
    }

    #[test]
    fn test_process_scroll_messages() {
        let mut runtime = create_test_runtime();

        // Add event to timeline
        let event = create_test_event();
        runtime.process_message(Msg::Timeline(TimelineMsg::AddNote(event)));

        // Test scroll operations
        runtime.process_message(Msg::Timeline(TimelineMsg::ScrollDown));
        assert_eq!(runtime.state().timeline.selected_index, Some(0));

        runtime.process_message(Msg::Timeline(TimelineMsg::ScrollUp));
        assert_eq!(runtime.state().timeline.selected_index, Some(0)); // No change as it's at the top
    }

    #[test]
    fn test_send_reaction_command() {
        let mut runtime = create_test_runtime();
        let target_event = create_test_event();

        let commands = runtime.process_message(Msg::Nostr(
            crate::core::msg::nostr::NostrMsg::SendReaction(target_event.clone()),
        ));
        assert_eq!(commands.len(), 1);

        match &commands[0] {
            Cmd::SendReaction {
                target_event: cmd_event,
            } => {
                assert_eq!(cmd_event, &target_event);
            }
            _ => panic!("Expected SendReaction command"),
        }

        // Status message is now set by translator when generating messages,
        // not by update() on Msg::Nostr
        assert!(runtime.state().system.status_message.is_none());
    }

    #[test]
    fn test_input_workflow() {
        let mut runtime = create_test_runtime();

        // Start new post
        runtime.process_message(Msg::Ui(UiMsg::ShowNewNote));
        assert!(runtime.state().ui.show_input);
        assert!(runtime.state().ui.reply_to.is_none());

        // Update input content
        let content = "Hello, Nostr!";
        runtime.process_message(Msg::Ui(UiMsg::UpdateInputContent(content.to_string())));
        assert_eq!(runtime.state().ui.input_content, content);

        // Submit post
        let commands = runtime.process_message(Msg::Ui(UiMsg::SubmitNote));
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
        runtime.process_message(Msg::Ui(UiMsg::ShowReply(target_event.clone())));
        assert!(runtime.state().ui.show_input);
        assert_eq!(runtime.state().ui.reply_to, Some(target_event));

        // Cancel input
        runtime.process_message(Msg::Ui(UiMsg::CancelInput));
        assert!(!runtime.state().ui.show_input);
        assert!(runtime.state().ui.reply_to.is_none());
    }

    #[test]
    fn test_receive_events() {
        let mut runtime = create_test_runtime();

        // Receive text note
        let text_event = create_test_event();
        runtime.process_message(Msg::Timeline(TimelineMsg::AddNote(text_event)));
        assert_eq!(runtime.state().timeline_len(), 1);

        // Receive metadata event
        let keys = Keys::generate();
        let metadata = Metadata::new()
            .name("Test User")
            .display_name("Test Display Name");
        let metadata_event = EventBuilder::metadata(&metadata)
            .sign_with_keys(&keys)
            .unwrap();

        let profile = crate::domain::nostr::Profile::new(
            keys.public_key(),
            metadata_event.created_at,
            metadata,
        );
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
        sender.send(Msg::Ui(UiMsg::ShowNewNote)).unwrap();
        sender
            .send(Msg::Ui(UiMsg::UpdateInputContent("test".to_string())))
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
        runtime.process_message(Msg::Nostr(crate::core::msg::nostr::NostrMsg::SendReaction(
            target_event.clone(),
        )));
        runtime.process_message(Msg::Nostr(crate::core::msg::nostr::NostrMsg::SendRepost(
            target_event,
        )));

        // Get pending commands
        let pending = runtime.pending_commands();
        assert_eq!(pending.len(), 2);

        // Getting them again returns empty
        let pending2 = runtime.pending_commands();
        assert!(pending2.is_empty());
    }

    #[test]
    fn test_runtime_with_executor() {
        use crate::integration::legacy::action::Action;
        use tokio::sync::mpsc;

        let keys = Keys::generate();
        let state = AppState::new(keys.public_key());
        let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();
        let mut runtime = ElmRuntime::new_with_executor(state, action_tx);

        // Check stats show executor is available but no Nostr support
        let stats = runtime.get_stats();
        assert!(stats.has_executor);
        assert!(!stats.has_nostr_support);

        // Send a message that generates a command
        let target_event = create_test_event();
        runtime.send_msg(Msg::Nostr(crate::core::msg::nostr::NostrMsg::SendReaction(
            target_event.clone(),
        )));

        // Process messages and execute commands
        let execution_log = runtime.run_update_cycle().unwrap();
        assert_eq!(execution_log.len(), 1);
        assert!(execution_log[0].contains("✓ Executed: SendReaction"));

        // Without Nostr executor, no Action should be sent (command is dropped with warning)
        assert!(action_rx.try_recv().is_err());
    }

    #[test]
    fn test_runtime_with_nostr_executor() {
        use crate::integration::legacy::action::Action;
        use tokio::sync::mpsc;

        let keys = Keys::generate();
        let state = AppState::new(keys.public_key());
        let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();
        let (nostr_tx, mut nostr_rx) = mpsc::unbounded_channel::<NostrCommand>();
        let mut runtime = ElmRuntime::new_with_nostr_executor(state, action_tx, nostr_tx);

        // Check stats show both executor and Nostr support
        let stats = runtime.get_stats();
        assert!(stats.has_executor);
        assert!(stats.has_nostr_support);

        // Send a message that generates a command
        let target_event = create_test_event();
        runtime.send_msg(Msg::Nostr(crate::core::msg::nostr::NostrMsg::SendReaction(
            target_event.clone(),
        )));

        // Process messages and execute commands
        let execution_log = runtime.run_update_cycle().unwrap();
        assert_eq!(execution_log.len(), 1);
        assert!(execution_log[0].contains("✓ Executed: SendReaction"));

        // Check that NostrCommand was sent (not Action)
        let nostr_cmd = nostr_rx.try_recv().unwrap();
        match nostr_cmd {
            NostrCommand::SendReaction {
                target_event: received_event,
                content,
            } => {
                assert_eq!(received_event.id, target_event.id);
                assert_eq!(content, "+");
            }
            _ => panic!("Expected SendReaction NostrCommand"),
        }

        // No Action should be sent when NostrCommand is available
        assert!(action_rx.try_recv().is_err());
    }

    #[test]
    fn test_add_nostr_support() {
        use crate::integration::legacy::action::Action;
        use tokio::sync::mpsc;

        let keys = Keys::generate();
        let state = AppState::new(keys.public_key());
        let (action_tx, _action_rx) = mpsc::unbounded_channel::<Action>();
        let (nostr_tx, _nostr_rx) = mpsc::unbounded_channel::<NostrCommand>();
        let mut runtime = ElmRuntime::new_with_executor(state, action_tx);

        // Initially no Nostr support
        assert!(!runtime.get_stats().has_nostr_support);

        // Add Nostr support
        let result = runtime.add_nostr_support(nostr_tx);
        assert!(result.is_ok());

        // Now has Nostr support
        assert!(runtime.get_stats().has_nostr_support);
    }

    #[test]
    fn test_add_nostr_support_without_executor() {
        use tokio::sync::mpsc;

        let keys = Keys::generate();
        let state = AppState::new(keys.public_key());
        let (nostr_tx, _nostr_rx) = mpsc::unbounded_channel::<NostrCommand>();
        let mut runtime = ElmRuntime::new(state);

        // No executor available
        assert!(!runtime.get_stats().has_executor);

        // Should fail to add Nostr support
        let result = runtime.add_nostr_support(nostr_tx);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No executor available"));
    }

    #[test]
    fn test_execute_command_without_executor() {
        let runtime = create_test_runtime();
        let cmd = Cmd::Tui(crate::core::cmd::TuiCommand::Render);

        // Should fail without executor
        let result = runtime.execute_command(&cmd);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("No command executor available"));
    }

    #[test]
    fn test_set_executor() {
        use crate::integration::legacy::action::Action;
        use tokio::sync::mpsc;

        let mut runtime = create_test_runtime();
        let (action_tx, _action_rx) = mpsc::unbounded_channel::<Action>();

        // Initially no executor
        assert!(!runtime.get_stats().has_executor);

        // Set executor
        runtime.set_executor(action_tx);
        assert!(runtime.get_stats().has_executor);

        // Should now be able to execute commands
        let cmd = Cmd::Tui(crate::core::cmd::TuiCommand::Render);
        let result = runtime.execute_command(&cmd);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_pending_commands_empty() {
        use crate::integration::legacy::action::Action;
        use tokio::sync::mpsc;

        let keys = Keys::generate();
        let state = AppState::new(keys.public_key());
        let (action_tx, _action_rx) = mpsc::unbounded_channel::<Action>();
        let mut runtime = ElmRuntime::new_with_executor(state, action_tx);

        // No pending commands
        let result = runtime.execute_pending_commands();
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
