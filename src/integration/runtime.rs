use color_eyre::eyre::eyre;
use color_eyre::Result;
use std::collections::VecDeque;
use tokio::sync::mpsc;

use crate::{
    core::{
        cmd::{Cmd, TuiCmd},
        msg::Msg,
        raw_msg::RawMsg,
        state::AppState,
        translator::translate_raw_to_domain,
        update::{update_with_context, UpdateContext},
    },
    infrastructure::{nostr::NostrOperation, tui::textarea_engine::TuiTextAreaEngine},
};

use super::cmd_executor::CmdExecutor;

/// Integration point between Elm architecture runtime and existing app
pub struct Runtime {
    ctx: UpdateContext<'static>,
    state: AppState,
    msg_queue: VecDeque<Msg>,
    raw_msg_queue: VecDeque<RawMsg>,
    cmd_queue: VecDeque<Cmd>,
    raw_msg_tx: mpsc::UnboundedSender<RawMsg>,
    raw_msg_rx: mpsc::UnboundedReceiver<RawMsg>,
    cmd_executor: Option<CmdExecutor>,
}

impl Runtime {
    /// Create a new Runtime
    pub fn new(initial_state: AppState) -> Self {
        static ENGINE: TuiTextAreaEngine = TuiTextAreaEngine;
        let (raw_msg_tx, raw_msg_rx) = mpsc::unbounded_channel();

        Self {
            ctx: UpdateContext { text_area: &ENGINE },
            state: initial_state,
            msg_queue: VecDeque::new(),
            raw_msg_queue: VecDeque::new(),
            cmd_queue: VecDeque::new(),
            raw_msg_tx,
            raw_msg_rx,
            cmd_executor: None,
        }
    }

    /// Create a new Runtime with command executor
    pub fn new_with_executor(initial_state: AppState) -> Self {
        static ENGINE: TuiTextAreaEngine = TuiTextAreaEngine;
        let (raw_msg_tx, raw_msg_rx) = mpsc::unbounded_channel();
        let executor = CmdExecutor::new();

        Self {
            ctx: UpdateContext { text_area: &ENGINE },
            state: initial_state,
            msg_queue: VecDeque::new(),
            raw_msg_queue: VecDeque::new(),
            cmd_queue: VecDeque::new(),
            raw_msg_tx,
            raw_msg_rx,
            cmd_executor: Some(executor),
        }
    }

    /// Create a new Runtime with NostrOperation support
    pub fn new_with_nostr_executor(
        initial_state: AppState,
        nostr_sender: mpsc::UnboundedSender<NostrOperation>,
    ) -> Self {
        static ENGINE: TuiTextAreaEngine = TuiTextAreaEngine;
        let (raw_msg_tx, raw_msg_rx) = mpsc::unbounded_channel();
        let executor = CmdExecutor::new_with_nostr(nostr_sender);

        Self {
            ctx: UpdateContext { text_area: &ENGINE },
            state: initial_state,
            msg_queue: VecDeque::new(),
            raw_msg_queue: VecDeque::new(),
            cmd_queue: VecDeque::new(),
            raw_msg_tx,
            raw_msg_rx,
            cmd_executor: Some(executor),
        }
    }

    /// Set command executor
    pub fn set_executor(&mut self) {
        self.cmd_executor = Some(CmdExecutor::new());
    }

    /// Set command executor with NostrOperation support
    pub fn set_nostr_executor(&mut self, nostr_sender: mpsc::UnboundedSender<NostrOperation>) {
        self.cmd_executor = Some(CmdExecutor::new_with_nostr(nostr_sender));
    }

    /// Add NostrOperation support to existing executor
    pub fn add_nostr_support(
        &mut self,
        nostr_sender: mpsc::UnboundedSender<NostrOperation>,
    ) -> Result<()> {
        if let Some(executor) = &mut self.cmd_executor {
            executor.set_nostr_sender(nostr_sender);
            Ok(())
        } else {
            Err(eyre!(
                "No executor available. Use set_executor() or set_nostr_executor() first."
            ))
        }
    }

    /// Add TUI command sender support to existing executor (for TuiCmd execution)
    pub fn add_tui_sender(&mut self, tui_sender: mpsc::UnboundedSender<TuiCmd>) -> Result<()> {
        if let Some(executor) = &mut self.cmd_executor {
            executor.set_tui_sender(tui_sender);
            Ok(())
        } else {
            Err(eyre!(
                "No executor available. Use set_executor() or set_nostr_executor() first."
            ))
        }
    }

    /// Add render request sender for orchestrated rendering in AppRunner
    pub fn add_render_request_sender(&mut self, render_sender: mpsc::Sender<()>) -> Result<()> {
        if let Some(executor) = &mut self.cmd_executor {
            executor.set_render_request_sender(render_sender);
            Ok(())
        } else {
            Err(eyre!(
                "No executor available. Use set_executor() or set_nostr_executor() first."
            ))
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
    pub fn get_raw_sender(&self) -> mpsc::UnboundedSender<RawMsg> {
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
    #[allow(clippy::unwrap_used)]
    pub fn execute_pending_commands(&mut self) -> Result<Vec<String>> {
        if self.cmd_executor.is_none() {
            return Err(eyre!(
                "No command executor available. Use set_executor() to configure."
            ));
        }

        let mut commands = self.pending_commands();
        if commands.is_empty() {
            return Ok(vec![]);
        }

        // Sort commands by priority
        commands.sort_by_key(Cmd::priority);

        // Now safely get the executor
        let executor = self.cmd_executor.as_ref().unwrap();
        executor
            .execute_commands(&commands)
            .map_err(|e| eyre!("Command execution failed: {e}"))
    }

    /// Execute a single command immediately
    pub fn execute_command(&self, cmd: &Cmd) -> Result<()> {
        if let Some(executor) = &self.cmd_executor {
            executor
                .execute_command(cmd)
                .map_err(|e| eyre!("Command execution failed: {e}"))
        } else {
            Err(eyre!(
                "No command executor available. Use set_executor() to configure."
            ))
        }
    }

    /// Process a single message
    pub fn process_message(&mut self, msg: Msg) -> Vec<Cmd> {
        let (new_state, commands) = update_with_context(msg, self.state.clone(), &self.ctx);
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

        all_commands
    }

    /// Process all messages and execute commands in one step
    pub fn run_update_cycle(&mut self) -> Result<Vec<String>> {
        let _commands = self.process_all_messages();
        self.execute_pending_commands()
    }

    /// Get runtime statistics
    pub fn get_stats(&self) -> RuntimeStats {
        let has_nostr_support = self
            .cmd_executor
            .as_ref()
            .map(|executor| executor.get_stats().has_nostr_sender)
            .unwrap_or(false);

        RuntimeStats {
            queued_messages: self.msg_queue.len(),
            queued_commands: self.cmd_queue.len(),
            timeline_notes_count: self.state.timeline.len(),
            profiles_count: self.state.user.profiles.len(),
            is_input_shown: self.state.ui.is_composing(),
            selected_note_index: self.state.timeline.selected_index,
            has_executor: self.cmd_executor.is_some(),
            has_nostr_support,
        }
    }
}

/// Runtime statistics
#[derive(Debug, Clone)]
pub struct RuntimeStats {
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
    use crate::core::cmd::NostrCmd;
    use crate::core::msg::nostr::NostrMsg;
    use crate::core::msg::system::SystemMsg;
    use crate::core::msg::timeline::TimelineMsg;
    use crate::core::msg::ui::UiMsg;
    use crate::core::msg::Msg;
    use crate::domain::nostr::Profile;
    use color_eyre::Result;
    use nostr_sdk::prelude::*;

    fn create_test_runtime() -> Runtime {
        let keys = Keys::generate();
        let state = AppState::new(keys.public_key());
        Runtime::new(state)
    }

    fn create_test_event() -> Result<Event> {
        let keys = Keys::generate();
        EventBuilder::text_note("test content")
            .sign_with_keys(&keys)
            .map_err(|e| e.into())
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

        let commands = runtime.process_message(Msg::System(SystemMsg::Quit));
        assert!(commands.is_empty());
        assert!(runtime.state().system.should_quit);
    }

    #[test]
    fn test_process_scroll_messages() -> Result<()> {
        let mut runtime = create_test_runtime();

        // Add event to timeline
        let event = create_test_event()?;
        runtime.process_message(Msg::Timeline(TimelineMsg::AddNote(event)));

        // Test scroll operations
        runtime.process_message(Msg::Timeline(TimelineMsg::ScrollDown));
        assert_eq!(runtime.state().timeline.selected_index, Some(0));

        runtime.process_message(Msg::Timeline(TimelineMsg::ScrollUp));
        assert_eq!(runtime.state().timeline.selected_index, Some(0)); // No change as it's at the top

        Ok(())
    }

    #[test]
    fn test_send_reaction_command() -> Result<()> {
        let mut runtime = create_test_runtime();
        let target_event = create_test_event()?;

        let commands =
            runtime.process_message(Msg::Nostr(NostrMsg::SendReaction(target_event.clone())));
        assert_eq!(commands.len(), 1);

        match &commands[0] {
            Cmd::Nostr(NostrCmd::SendReaction {
                target_event: cmd_event,
            }) => {
                assert_eq!(cmd_event, &target_event);
            }
            _ => panic!("Expected SendReaction command"),
        }

        // Status message is now set by translator when generating messages,
        // not by update() on Msg::Nostr
        assert!(runtime.state().system.status_message.is_none());

        Ok(())
    }

    #[test]
    fn test_input_workflow() {
        let mut runtime = create_test_runtime();

        // Start new post
        runtime.process_message(Msg::Ui(UiMsg::ShowNewNote));
        assert!(runtime.state().ui.is_composing());
        assert!(runtime.state().ui.reply_to.is_none());

        // Update input content
        let content = "Hello, Nostr!";
        runtime.process_message(Msg::Ui(UiMsg::UpdateInputContent(content.to_string())));
        assert_eq!(runtime.state().ui.textarea.content, content);

        // Submit post
        let commands = runtime.process_message(Msg::Ui(UiMsg::SubmitNote));
        assert_eq!(commands.len(), 1);

        match &commands[0] {
            Cmd::Nostr(NostrCmd::SendTextNote {
                content: cmd_content,
                ..
            }) => {
                assert_eq!(cmd_content, content);
            }
            _ => panic!("Expected SendTextNote command"),
        }

        // Check if UI is reset
        assert!(runtime.state().ui.is_normal());
        assert!(runtime.state().ui.textarea.content.is_empty());
    }

    #[test]
    fn test_reply_workflow() -> Result<()> {
        let mut runtime = create_test_runtime();
        let target_event = create_test_event()?;

        // Start reply
        runtime.process_message(Msg::Ui(UiMsg::ShowReply(target_event.clone())));
        assert!(runtime.state().ui.is_composing());
        assert_eq!(runtime.state().ui.reply_to, Some(target_event));

        // Cancel input
        runtime.process_message(Msg::Ui(UiMsg::CancelInput));
        assert!(runtime.state().ui.is_normal());
        assert!(runtime.state().ui.reply_to.is_none());

        Ok(())
    }

    #[test]
    fn test_receive_events() -> Result<()> {
        let mut runtime = create_test_runtime();

        // Receive text note
        let text_event = create_test_event()?;
        runtime.process_message(Msg::Timeline(TimelineMsg::AddNote(text_event)));
        assert_eq!(runtime.state().timeline.len(), 1);

        // Receive metadata event
        let keys = Keys::generate();
        let metadata = Metadata::new()
            .name("Test User")
            .display_name("Test Display Name");
        let metadata_event = EventBuilder::metadata(&metadata).sign_with_keys(&keys)?;

        let profile = Profile::new(keys.public_key(), metadata_event.created_at, metadata);
        runtime.process_message(Msg::UpdateProfile(keys.public_key(), profile));
        assert!(runtime
            .state()
            .user
            .profiles
            .contains_key(&keys.public_key()));

        Ok(())
    }

    #[test]
    fn test_external_message_channel() -> Result<()> {
        let mut runtime = create_test_runtime();

        // Send messages from external source (now via internal API)
        runtime.send_msg(Msg::Ui(UiMsg::ShowNewNote));
        runtime.send_msg(Msg::Ui(UiMsg::UpdateInputContent("test".to_string())));

        // Not processed yet
        assert!(runtime.state().ui.is_normal());

        // Process all messages
        let commands = runtime.process_all_messages();

        // State has been updated
        assert!(runtime.state().ui.is_composing());
        assert_eq!(runtime.state().ui.textarea.content, "test");
        assert!(commands.is_empty());

        Ok(())
    }

    #[test]
    fn test_pending_commands() -> Result<()> {
        let mut runtime = create_test_runtime();
        let target_event = create_test_event()?;

        // Send messages that generate commands
        runtime.process_message(Msg::Nostr(NostrMsg::SendReaction(target_event.clone())));
        runtime.process_message(Msg::Nostr(NostrMsg::SendRepost(target_event)));

        // Get pending commands
        let pending = runtime.pending_commands();
        assert_eq!(pending.len(), 2);

        // Getting them again returns empty
        let pending2 = runtime.pending_commands();
        assert!(pending2.is_empty());

        Ok(())
    }

    #[test]
    fn test_runtime_with_executor() -> Result<()> {
        let keys = Keys::generate();
        let state = AppState::new(keys.public_key());
        let mut runtime = Runtime::new_with_executor(state);

        // Check stats show executor is available but no Nostr support
        let stats = runtime.get_stats();
        assert!(stats.has_executor);
        assert!(!stats.has_nostr_support);

        // Send a message that generates a command
        let target_event = create_test_event()?;
        runtime.send_msg(Msg::Nostr(NostrMsg::SendReaction(target_event)));

        // Process messages and execute commands
        let execution_log = runtime.run_update_cycle()?;
        assert_eq!(execution_log.len(), 1);
        assert!(execution_log[0].contains("✓ Executed: SendReaction"));

        Ok(())
    }

    #[test]
    fn test_runtime_with_nostr_executor() -> Result<()> {
        let keys = Keys::generate();
        let state = AppState::new(keys.public_key());
        let (nostr_tx, mut nostr_rx) = mpsc::unbounded_channel::<NostrOperation>();
        let mut runtime = Runtime::new_with_nostr_executor(state, nostr_tx);

        // Check stats show both executor and Nostr support
        let stats = runtime.get_stats();
        assert!(stats.has_executor);
        assert!(stats.has_nostr_support);

        // Send a message that generates a command
        let target_event = create_test_event()?;
        runtime.send_msg(Msg::Nostr(NostrMsg::SendReaction(target_event.clone())));

        // Process messages and execute commands
        let execution_log = runtime.run_update_cycle()?;
        assert_eq!(execution_log.len(), 1);
        assert!(execution_log[0].contains("✓ Executed: SendReaction"));

        // Check that NostrOperation was sent (not Action)
        let nostr_op = nostr_rx.try_recv()?;
        match nostr_op {
            NostrOperation::SendReaction {
                target_event: received_event,
                content,
            } => {
                assert_eq!(received_event.id, target_event.id);
                assert_eq!(content, "+");
            }
            _ => panic!("Expected SendReaction NostrOperation"),
        }

        Ok(())
    }

    #[test]
    fn test_add_nostr_support() -> Result<()> {
        let keys = Keys::generate();
        let state = AppState::new(keys.public_key());
        let (nostr_tx, _nostr_rx) = mpsc::unbounded_channel::<NostrOperation>();
        let mut runtime = Runtime::new_with_executor(state);

        // Initially no Nostr support
        assert!(!runtime.get_stats().has_nostr_support);

        // Add Nostr support
        runtime.add_nostr_support(nostr_tx)?;

        // Now has Nostr support
        assert!(runtime.get_stats().has_nostr_support);

        Ok(())
    }

    #[test]
    fn test_add_nostr_support_without_executor() {
        let keys = Keys::generate();
        let state = AppState::new(keys.public_key());
        let (nostr_tx, _nostr_rx) = mpsc::unbounded_channel::<NostrOperation>();
        let mut runtime = Runtime::new(state);

        // No executor available
        assert!(!runtime.get_stats().has_executor);

        // Should fail to add Nostr support
        let result = runtime.add_nostr_support(nostr_tx);
        assert!(
            matches!(result, Err(ref report) if report.to_string().contains("No executor available"))
        );
    }

    #[test]
    fn test_execute_command_without_executor() {
        let runtime = create_test_runtime();
        let cmd = Cmd::LoadConfig;

        // Should fail without executor
        let result = runtime.execute_command(&cmd);
        assert!(
            matches!(result, Err(ref report) if report.to_string().contains("No command executor available"))
        );
    }

    #[test]
    fn test_set_executor() -> Result<()> {
        let mut runtime = create_test_runtime();

        // Initially no executor
        assert!(!runtime.get_stats().has_executor);

        // Set executor
        runtime.set_executor();
        assert!(runtime.get_stats().has_executor);

        // Should now be able to execute commands
        let cmd = Cmd::LoadConfig;
        runtime.execute_command(&cmd)
    }

    #[test]
    fn test_execute_pending_commands_empty() -> Result<()> {
        let keys = Keys::generate();
        let state = AppState::new(keys.public_key());
        let mut runtime = Runtime::new_with_executor(state);

        // No pending commands
        let result = runtime.execute_pending_commands();
        assert!(result?.is_empty());

        Ok(())
    }
}
