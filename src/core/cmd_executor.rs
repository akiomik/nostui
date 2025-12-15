use color_eyre::eyre::Result;
use tokio::sync::mpsc;

use crate::{
    core::cmd::{Cmd, TuiCommand},
    infrastructure::{nostr_command::NostrCommand, tui_service::TuiService},
    integration::legacy::action::Action,
};

/// Command executor that bridges Elm commands to Action and NostrCommand systems
#[derive(Clone)]
pub struct CmdExecutor {
    action_sender: mpsc::UnboundedSender<Action>,
    nostr_sender: Option<mpsc::UnboundedSender<NostrCommand>>,
    tui_service: Option<TuiService>,
    tui_sender: Option<mpsc::UnboundedSender<TuiCommand>>,
    render_req_sender: Option<mpsc::UnboundedSender<()>>,
}

impl CmdExecutor {
    /// Create a new command executor with Action support only
    pub fn new(action_sender: mpsc::UnboundedSender<Action>) -> Self {
        Self {
            action_sender,
            nostr_sender: None,
            tui_service: None,
            tui_sender: None,
            render_req_sender: None,
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
            tui_service: None,
            tui_sender: None,
            render_req_sender: None,
        }
    }

    /// Add NostrCommand support to existing executor
    pub fn set_nostr_sender(&mut self, nostr_sender: mpsc::UnboundedSender<NostrCommand>) {
        self.nostr_sender = Some(nostr_sender);
    }

    /// Inject TUI service for executing TuiCommand (legacy compatibility).
    pub fn set_tui_service(&mut self, tui_service: TuiService) {
        self.tui_service = Some(tui_service);
    }

    /// Inject TUI command sender for executing TuiCommand asynchronously.
    pub fn set_tui_sender(&mut self, sender: mpsc::UnboundedSender<TuiCommand>) {
        self.tui_sender = Some(sender);
    }

    /// Inject render request sender for AppRunner-orchestrated rendering.
    pub fn set_render_request_sender(&mut self, sender: mpsc::UnboundedSender<()>) {
        self.render_req_sender = Some(sender);
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
                    // No NostrService available: drop with warning (no legacy Action fallback)
                    log::warn!("SendReaction ignored: NostrService not available");
                }
            }

            Cmd::SendRepost { target_event } => {
                if let Some(nostr_sender) = &self.nostr_sender {
                    let nostr_cmd = NostrCommand::repost(target_event.clone(), None);
                    nostr_sender.send(nostr_cmd)?;
                } else {
                    // No NostrService available: drop with warning (no legacy Action fallback)
                    log::warn!("SendRepost ignored: NostrService not available");
                }
            }

            Cmd::SendTextNote { content, tags } => {
                log::info!(
                    "CmdExecutor: Processing SendTextNote - content: '{}', tags: {:?}",
                    content,
                    tags
                );
                if let Some(nostr_sender) = &self.nostr_sender {
                    log::info!("CmdExecutor: Routing to NostrService");
                    let nostr_cmd = NostrCommand::text_note(content.clone(), tags.clone());
                    nostr_sender.send(nostr_cmd)?;
                    log::info!("CmdExecutor: Successfully sent NostrCommand::SendTextNote");
                } else {
                    // No NostrService available: drop with warning (no legacy Action fallback)
                    log::warn!("SendTextNote ignored: NostrService not available");
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

            Cmd::Tui(tui_cmd) => {
                match tui_cmd {
                    TuiCommand::Render => {
                        // Prefer AppRunner-orchestrated rendering via render request channel
                        if let Some(rtx) = &self.render_req_sender {
                            let _ = rtx.send(());
                            return Ok(());
                        }
                        // Fallback: if a TUI sender is configured, forward Render (legacy behavior)
                        if let Some(tx) = &self.tui_sender {
                            let _ = tx.send(TuiCommand::Render);
                            return Ok(());
                        }
                        // Fallback during transition (will be removed): if no sender available
                        log::warn!("CmdExecutor: falling back to Action::Render (no render sender configured)");
                        self.action_sender.send(Action::Render)?;
                    }
                    TuiCommand::Resize { width, height } => {
                        if let Some(tx) = &self.tui_sender {
                            let _ = tx.send(TuiCommand::Resize {
                                width: *width,
                                height: *height,
                            });
                            return Ok(());
                        }
                        // No TUI sender configured: drop with warning (no legacy Action fallback)
                        log::warn!(
                            "CmdExecutor: TUI sender not configured; dropping Resize command {}x{}",
                            width,
                            height
                        );
                    }
                }
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
            Cmd::LogError { .. } => "LogError".to_string(),
            Cmd::LogInfo { .. } => "LogInfo".to_string(),
            Cmd::StartTimer { .. } => "StartTimer".to_string(),
            Cmd::StopTimer { .. } => "StopTimer".to_string(),
            Cmd::Batch(cmds) => format!("Batch({})", cmds.len()),
            Cmd::Tui(tc) => match tc {
                TuiCommand::Render => "Tui(Render)".to_string(),
                TuiCommand::Resize { .. } => "Tui(Resize)".to_string(),
            },
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

        // No Nostr sender configured: should NOT send any Action (dropped with warn)
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_execute_send_text_note() {
        let (executor, mut rx) = create_test_executor();
        let cmd = Cmd::SendTextNote {
            content: "Hello, Nostr!".to_string(),
            tags: vec![],
        };

        executor.execute_command(&cmd).unwrap();

        // No Nostr sender configured: should NOT send any Action (dropped with warn)
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_execute_resize() {
        let (mut executor, _action_rx) = create_test_executor();
        // Provide TUI sender and assert that Resize is routed there
        let (tui_tx, mut tui_rx) = mpsc::unbounded_channel::<TuiCommand>();
        executor.set_tui_sender(tui_tx);

        let cmd = Cmd::Tui(crate::core::cmd::TuiCommand::Resize {
            width: 80,
            height: 24,
        });

        executor.execute_command(&cmd).unwrap();

        let tui_cmd = tui_rx.try_recv().unwrap();
        match tui_cmd {
            TuiCommand::Resize { width, height } => {
                assert_eq!(width, 80);
                assert_eq!(height, 24);
            }
            _ => panic!("Expected TuiCommand::Resize"),
        }
    }

    #[test]
    fn test_execute_render() {
        let (executor, mut rx) = create_test_executor();
        let cmd = Cmd::Tui(crate::core::cmd::TuiCommand::Render);

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
        let (mut executor, _rx) = create_test_executor();
        // Provide TUI and Render request senders
        let (tui_tx, mut tui_rx) = mpsc::unbounded_channel::<TuiCommand>();
        executor.set_tui_sender(tui_tx);
        let (render_tx, mut render_rx) = mpsc::unbounded_channel::<()>();
        executor.set_render_request_sender(render_tx);

        let cmds = vec![
            Cmd::Tui(crate::core::cmd::TuiCommand::Render),
            Cmd::Tui(crate::core::cmd::TuiCommand::Resize {
                width: 100,
                height: 50,
            }),
        ];
        let batch_cmd = Cmd::Batch(cmds);

        executor.execute_command(&batch_cmd).unwrap();

        // Should receive render request and resize command
        render_rx.try_recv().unwrap();
        let tui_cmd = tui_rx.try_recv().unwrap();
        assert!(matches!(
            tui_cmd,
            TuiCommand::Resize {
                width: 100,
                height: 50
            }
        ));
    }

    #[test]
    fn test_execute_multiple_commands() {
        let (mut executor, _rx) = create_test_executor();
        // Provide render request sender to observe execution
        let (render_tx, mut render_rx) = mpsc::unbounded_channel::<()>();
        executor.set_render_request_sender(render_tx);

        let commands = vec![
            Cmd::Tui(crate::core::cmd::TuiCommand::Render),
            Cmd::LogInfo {
                message: "test".to_string(),
            },
        ];

        let log = executor.execute_commands(&commands).unwrap();

        assert_eq!(log.len(), 2);
        assert!(log[0].contains("✓ Executed: Tui(Render)"));
        assert!(log[1].contains("✓ Executed: LogInfo"));

        // Should receive the render signal
        render_rx.try_recv().unwrap();
    }

    #[test]
    fn test_cmd_name_trait() {
        let cmd = Cmd::SendReaction {
            target_event: create_test_event(),
        };
        assert_eq!(cmd.name(), "SendReaction");

        let batch_cmd = Cmd::Batch(vec![
            Cmd::Tui(crate::core::cmd::TuiCommand::Render),
            Cmd::None,
        ]);
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
    fn test_no_fallback_without_nostr_sender() {
        let (executor, mut rx) = create_test_executor(); // No NostrSender
        let target_event = create_test_event();
        let cmd = Cmd::SendReaction {
            target_event: target_event.clone(),
        };

        // Should NOT fallback to Action
        executor.execute_command(&cmd).unwrap();

        assert!(rx.try_recv().is_err());
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
