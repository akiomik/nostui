use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::{action::Action, cmd::Cmd, nostr_command::NostrCommand};

/// Command executor that bridges Elm commands to Action and NostrCommand systems
#[derive(Clone)]
pub struct CmdExecutor {
    action_sender: mpsc::UnboundedSender<Action>,
    nostr_sender: Option<mpsc::UnboundedSender<NostrCommand>>,
}

impl CmdExecutor {
    /// Create a new command executor with Action support only
    pub fn new(action_sender: mpsc::UnboundedSender<Action>) -> Self {
        Self {
            action_sender,
            nostr_sender: None,
        }
    }

    /// Create a new command executor with both Action and NostrCommand support
    pub fn new_with_nostr(
        action_sender: mpsc::UnboundedSender<Action>,
        nostr_sender: mpsc::UnboundedSender<NostrCommand>,
    ) -> Self {
        Self {
            action_sender,
            nostr_sender: Some(nostr_sender),
        }
    }

    /// Add NostrCommand support to existing executor
    pub fn set_nostr_sender(&mut self, nostr_sender: mpsc::UnboundedSender<NostrCommand>) {
        self.nostr_sender = Some(nostr_sender);
    }

    /// Execute a single command by converting it to appropriate Action or NostrCommand
    pub fn execute_command(&self, cmd: &Cmd) -> Result<()> {
        match cmd {
            Cmd::None => {
                // No-op command, nothing to execute
            }

            // Nostr protocol commands - route to NostrService if available, fallback to Action
            Cmd::SendReaction { target_event } => {
                if let Some(nostr_sender) = &self.nostr_sender {
                    let nostr_cmd = NostrCommand::like(target_event.clone());
                    nostr_sender.send(nostr_cmd)?;
                } else {
                    // Fallback to legacy Action system
                    self.action_sender
                        .send(Action::SendReaction(target_event.clone()))?;
                }
            }

            Cmd::SendRepost { target_event } => {
                if let Some(nostr_sender) = &self.nostr_sender {
                    let nostr_cmd = NostrCommand::repost(target_event.clone(), None);
                    nostr_sender.send(nostr_cmd)?;
                } else {
                    // Fallback to legacy Action system
                    self.action_sender
                        .send(Action::SendRepost(target_event.clone()))?;
                }
            }

            Cmd::SendTextNote { content, tags } => {
                if let Some(nostr_sender) = &self.nostr_sender {
                    let nostr_cmd = NostrCommand::text_note(content.clone(), tags.clone());
                    nostr_sender.send(nostr_cmd)?;
                } else {
                    // Fallback to legacy Action system
                    self.action_sender
                        .send(Action::SendTextNote(content.clone(), tags.clone()))?;
                }
            }

            Cmd::ConnectToRelays { relays } => {
                if let Some(nostr_sender) = &self.nostr_sender {
                    let nostr_cmd = NostrCommand::connect_relays(relays.clone());
                    nostr_sender.send(nostr_cmd)?;
                } else {
                    log::warn!("ConnectToRelays command ignored: NostrService not available");
                }
            }

            Cmd::DisconnectFromRelays => {
                if let Some(nostr_sender) = &self.nostr_sender {
                    let nostr_cmd = NostrCommand::DisconnectFromRelays;
                    nostr_sender.send(nostr_cmd)?;
                } else {
                    log::warn!("DisconnectFromRelays command ignored: NostrService not available");
                }
            }

            Cmd::SubscribeToTimeline => {
                if let Some(nostr_sender) = &self.nostr_sender {
                    let nostr_cmd = NostrCommand::SubscribeToTimeline;
                    nostr_sender.send(nostr_cmd)?;
                } else {
                    log::warn!("SubscribeToTimeline command ignored: NostrService not available");
                }
            }

            Cmd::SaveConfig => {
                log::info!("Command to save config");
                // TODO: Implement config saving
                // This remains as TODO since it's not Nostr-related
            }

            Cmd::LoadConfig => {
                log::info!("Command to load config");
                // TODO: Implement config loading
                // This remains as TODO since it's not Nostr-related
            }

            Cmd::Resize { width, height } => {
                self.action_sender.send(Action::Resize(*width, *height))?;
            }

            Cmd::Render => {
                self.action_sender.send(Action::Render)?;
            }

            Cmd::LogError { message } => {
                log::error!("Elm command error: {}", message);
            }

            Cmd::LogInfo { message } => {
                log::info!("Elm command info: {}", message);
            }

            Cmd::StartTimer { id, duration_ms } => {
                log::info!("Start timer {} for {}ms", id, duration_ms);
                // TODO: Implement timer system
            }

            Cmd::StopTimer { id } => {
                log::info!("Stop timer {}", id);
                // TODO: Implement timer system
            }

            Cmd::Batch(commands) => {
                for cmd in commands {
                    self.execute_command(cmd)?;
                }
            }
        }

        Ok(())
    }

    /// Execute multiple commands
    pub fn execute_commands(&self, commands: &[Cmd]) -> Result<Vec<String>> {
        let mut execution_log = Vec::new();

        for cmd in commands {
            match self.execute_command(cmd) {
                Ok(()) => {
                    execution_log.push(format!("✓ Executed: {}", cmd.name()));
                }
                Err(e) => {
                    let error_msg = format!("✗ Failed to execute {}: {}", cmd.name(), e);
                    log::error!("{}", error_msg);
                    execution_log.push(error_msg);
                }
            }
        }

        Ok(execution_log)
    }

    /// Get execution statistics
    pub fn get_stats(&self) -> CmdExecutorStats {
        CmdExecutorStats {
            is_action_sender_closed: self.action_sender.is_closed(),
            has_nostr_sender: self.nostr_sender.is_some(),
            is_nostr_sender_closed: self.nostr_sender.as_ref().map(|sender| sender.is_closed()),
        }
    }
}

/// Command executor statistics
#[derive(Debug, Clone)]
pub struct CmdExecutorStats {
    pub is_action_sender_closed: bool,
    pub has_nostr_sender: bool,
    pub is_nostr_sender_closed: Option<bool>,
}

/// Extension trait for Cmd to get human-readable names
trait CmdName {
    fn name(&self) -> String;
}

impl CmdName for Cmd {
    fn name(&self) -> String {
        match self {
            Cmd::None => "None".to_string(),
            Cmd::SendReaction { .. } => "SendReaction".to_string(),
            Cmd::SendRepost { .. } => "SendRepost".to_string(),
            Cmd::SendTextNote { .. } => "SendTextNote".to_string(),
            Cmd::ConnectToRelays { .. } => "ConnectToRelays".to_string(),
            Cmd::DisconnectFromRelays => "DisconnectFromRelays".to_string(),
            Cmd::SubscribeToTimeline => "SubscribeToTimeline".to_string(),
            Cmd::SaveConfig => "SaveConfig".to_string(),
            Cmd::LoadConfig => "LoadConfig".to_string(),
            Cmd::Resize { .. } => "Resize".to_string(),
            Cmd::Render => "Render".to_string(),
            Cmd::LogError { .. } => "LogError".to_string(),
            Cmd::LogInfo { .. } => "LogInfo".to_string(),
            Cmd::StartTimer { .. } => "StartTimer".to_string(),
            Cmd::StopTimer { .. } => "StopTimer".to_string(),
            Cmd::Batch(cmds) => format!("Batch({})", cmds.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::*;
    use tokio::sync::mpsc;

    fn create_test_executor() -> (CmdExecutor, mpsc::UnboundedReceiver<Action>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let executor = CmdExecutor::new(tx);
        (executor, rx)
    }

    fn create_test_event() -> Event {
        let keys = Keys::generate();
        EventBuilder::text_note("test content")
            .sign_with_keys(&keys)
            .unwrap()
    }

    #[test]
    fn test_execute_send_reaction() {
        let (executor, mut rx) = create_test_executor();
        let event = create_test_event();
        let cmd = Cmd::SendReaction {
            target_event: event.clone(),
        };

        executor.execute_command(&cmd).unwrap();

        let received_action = rx.try_recv().unwrap();
        match received_action {
            Action::SendReaction(received_event) => {
                assert_eq!(received_event.id, event.id);
            }
            _ => panic!("Expected SendReaction action"),
        }
    }

    #[test]
    fn test_execute_send_text_note() {
        let (executor, mut rx) = create_test_executor();
        let cmd = Cmd::SendTextNote {
            content: "Hello, Nostr!".to_string(),
            tags: vec![],
        };

        executor.execute_command(&cmd).unwrap();

        let received_action = rx.try_recv().unwrap();
        match received_action {
            Action::SendTextNote(content, tags) => {
                assert_eq!(content, "Hello, Nostr!");
                assert!(tags.is_empty());
            }
            _ => panic!("Expected SendTextNote action"),
        }
    }

    #[test]
    fn test_execute_resize() {
        let (executor, mut rx) = create_test_executor();
        let cmd = Cmd::Resize {
            width: 80,
            height: 24,
        };

        executor.execute_command(&cmd).unwrap();

        let received_action = rx.try_recv().unwrap();
        match received_action {
            Action::Resize(width, height) => {
                assert_eq!(width, 80);
                assert_eq!(height, 24);
            }
            _ => panic!("Expected Resize action"),
        }
    }

    #[test]
    fn test_execute_render() {
        let (executor, mut rx) = create_test_executor();
        let cmd = Cmd::Render;

        executor.execute_command(&cmd).unwrap();

        let received_action = rx.try_recv().unwrap();
        match received_action {
            Action::Render => {}
            _ => panic!("Expected Render action"),
        }
    }

    #[test]
    fn test_execute_none() {
        let (executor, mut rx) = create_test_executor();
        let cmd = Cmd::None;

        executor.execute_command(&cmd).unwrap();

        // Should not send any action
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_execute_batch() {
        let (executor, mut rx) = create_test_executor();
        let cmds = vec![
            Cmd::Render,
            Cmd::Resize {
                width: 100,
                height: 50,
            },
        ];
        let batch_cmd = Cmd::Batch(cmds);

        executor.execute_command(&batch_cmd).unwrap();

        // Should receive both actions
        let action1 = rx.try_recv().unwrap();
        let action2 = rx.try_recv().unwrap();

        assert!(matches!(action1, Action::Render));
        assert!(matches!(action2, Action::Resize(100, 50)));
    }

    #[test]
    fn test_execute_multiple_commands() {
        let (executor, mut rx) = create_test_executor();
        let commands = vec![
            Cmd::Render,
            Cmd::LogInfo {
                message: "test".to_string(),
            },
        ];

        let log = executor.execute_commands(&commands).unwrap();

        assert_eq!(log.len(), 2);
        assert!(log[0].contains("✓ Executed: Render"));
        assert!(log[1].contains("✓ Executed: LogInfo"));

        // Should receive the render action
        let action = rx.try_recv().unwrap();
        assert!(matches!(action, Action::Render));
    }

    #[test]
    fn test_cmd_name_trait() {
        let cmd = Cmd::SendReaction {
            target_event: create_test_event(),
        };
        assert_eq!(cmd.name(), "SendReaction");

        let batch_cmd = Cmd::Batch(vec![Cmd::Render, Cmd::None]);
        assert_eq!(batch_cmd.name(), "Batch(2)");
    }

    #[test]
    fn test_executor_stats() {
        let (executor, _rx) = create_test_executor();
        let stats = executor.get_stats();

        assert!(!stats.is_action_sender_closed);
        assert!(!stats.has_nostr_sender);
        assert!(stats.is_nostr_sender_closed.is_none());
    }

    #[test]
    fn test_executor_with_nostr_sender() {
        let (action_tx, _action_rx) = mpsc::unbounded_channel();
        let (nostr_tx, _nostr_rx) = mpsc::unbounded_channel::<NostrCommand>();
        let executor = CmdExecutor::new_with_nostr(action_tx, nostr_tx);

        let stats = executor.get_stats();
        assert!(stats.has_nostr_sender);
        assert_eq!(stats.is_nostr_sender_closed, Some(false));
    }

    #[test]
    fn test_nostr_command_routing() {
        let (action_tx, mut action_rx) = mpsc::unbounded_channel();
        let (nostr_tx, mut nostr_rx) = mpsc::unbounded_channel::<NostrCommand>();
        let executor = CmdExecutor::new_with_nostr(action_tx, nostr_tx);

        let target_event = create_test_event();
        let cmd = Cmd::SendReaction {
            target_event: target_event.clone(),
        };

        // Should route to NostrCommand, not Action
        executor.execute_command(&cmd).unwrap();

        // NostrCommand should be sent
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

        // Action should NOT be sent
        assert!(action_rx.try_recv().is_err());
    }

    #[test]
    fn test_fallback_to_action() {
        let (executor, mut rx) = create_test_executor(); // No NostrSender
        let target_event = create_test_event();
        let cmd = Cmd::SendReaction {
            target_event: target_event.clone(),
        };

        // Should fallback to Action
        executor.execute_command(&cmd).unwrap();

        let received_action = rx.try_recv().unwrap();
        match received_action {
            Action::SendReaction(received_event) => {
                assert_eq!(received_event.id, target_event.id);
            }
            _ => panic!("Expected SendReaction action"),
        }
    }

    #[test]
    fn test_nostr_only_commands() {
        let (action_tx, mut action_rx) = mpsc::unbounded_channel();
        let (nostr_tx, mut nostr_rx) = mpsc::unbounded_channel::<NostrCommand>();
        let executor = CmdExecutor::new_with_nostr(action_tx, nostr_tx);

        // Test ConnectToRelays (NostrService only)
        let cmd = Cmd::ConnectToRelays {
            relays: vec!["wss://relay.example.com".to_string()],
        };
        executor.execute_command(&cmd).unwrap();

        let nostr_cmd = nostr_rx.try_recv().unwrap();
        match nostr_cmd {
            NostrCommand::ConnectToRelays { relays } => {
                assert_eq!(relays, vec!["wss://relay.example.com".to_string()]);
            }
            _ => panic!("Expected ConnectToRelays NostrCommand"),
        }

        // No Action should be sent
        assert!(action_rx.try_recv().is_err());
    }
}
