use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::msg::Msg;

/// UI (TUI) specific sub-commands executed by the host/runtime
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TuiCommand {
    Render,
    Resize { width: u16, height: u16 },
}

/// Elm-like command definitions
/// Represents side effects (network communication, file I/O, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cmd {
    // Nostr-related commands
    SendReaction { target_event: Event },
    SendRepost { target_event: Event },
    SendTextNote { content: String, tags: Vec<Tag> },
    ConnectToRelays { relays: Vec<String> },
    DisconnectFromRelays,
    SubscribeToTimeline,

    // UI-related commands
    Tui(TuiCommand),

    // File/configuration related
    SaveConfig,
    LoadConfig,

    // Logging related
    LogError { message: String },
    LogInfo { message: String },

    // Time related
    StartTimer { id: String, duration_ms: u64 },
    StopTimer { id: String },

    // Batch command (execute multiple commands together)
    Batch(Vec<Cmd>),

    // Do nothing (for testing)
    None,
}

impl Cmd {
    /// Combine multiple commands into one
    pub fn batch(commands: Vec<Cmd>) -> Cmd {
        match commands.len() {
            0 => Cmd::None,
            1 => commands.into_iter().next().unwrap(),
            _ => Cmd::Batch(commands),
        }
    }

    /// Whether the command requires asynchronous processing
    pub fn is_async(&self) -> bool {
        match self {
            Cmd::SendReaction { .. }
            | Cmd::SendRepost { .. }
            | Cmd::SendTextNote { .. }
            | Cmd::ConnectToRelays { .. }
            | Cmd::DisconnectFromRelays
            | Cmd::SubscribeToTimeline
            | Cmd::SaveConfig
            | Cmd::LoadConfig => true,

            Cmd::Tui(..)
            | Cmd::LogError { .. }
            | Cmd::LogInfo { .. }
            | Cmd::StartTimer { .. }
            | Cmd::StopTimer { .. }
            | Cmd::None => false,

            Cmd::Batch(cmds) => cmds.iter().any(|cmd| cmd.is_async()),
        }
    }

    /// Get command priority (smaller numbers = higher priority)
    pub fn priority(&self) -> u8 {
        match self {
            // UI-related has highest priority
            Cmd::Tui(..) => 0,

            // User actions have high priority
            Cmd::SendReaction { .. } | Cmd::SendRepost { .. } | Cmd::SendTextNote { .. } => 1,

            // Network-related has medium priority
            Cmd::ConnectToRelays { .. } | Cmd::DisconnectFromRelays | Cmd::SubscribeToTimeline => 2,

            // File operations have low priority
            Cmd::SaveConfig | Cmd::LoadConfig => 3,

            // Logging and timers have lowest priority
            Cmd::LogError { .. }
            | Cmd::LogInfo { .. }
            | Cmd::StartTimer { .. }
            | Cmd::StopTimer { .. } => 4,

            // Batch takes highest priority of contained commands
            Cmd::Batch(cmds) => cmds.iter().map(|cmd| cmd.priority()).min().unwrap_or(255),

            Cmd::None => 255,
        }
    }
}

/// Command execution result
#[derive(Debug, Clone)]
pub enum CmdResult {
    /// Success (may generate new messages)
    Success(Vec<Msg>),
    /// Error
    Error(String),
    /// Still executing (for async commands)
    Pending,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_event() -> Event {
        let keys = Keys::generate();
        EventBuilder::text_note("test content")
            .sign_with_keys(&keys)
            .unwrap()
    }

    #[test]
    fn test_cmd_batch_empty() {
        let cmd = Cmd::batch(vec![]);
        assert_eq!(cmd, Cmd::None);
    }

    #[test]
    fn test_cmd_batch_single() {
        let original_cmd = Cmd::Tui(TuiCommand::Render);
        let cmd = Cmd::batch(vec![original_cmd.clone()]);
        assert_eq!(cmd, original_cmd);
    }

    #[test]
    fn test_cmd_batch_multiple() {
        let cmds = vec![Cmd::Tui(TuiCommand::Render), Cmd::SaveConfig];
        let batch_cmd = Cmd::batch(cmds.clone());
        assert_eq!(batch_cmd, Cmd::Batch(cmds));
    }

    #[test]
    fn test_cmd_is_async() {
        assert!(Cmd::SendTextNote {
            content: "test".to_string(),
            tags: vec![]
        }
        .is_async());

        assert!(Cmd::ConnectToRelays {
            relays: vec!["wss://relay.damus.io".to_string()]
        }
        .is_async());

        assert!(!Cmd::Tui(TuiCommand::Render).is_async());
        assert!(!Cmd::Tui(TuiCommand::Resize {
            width: 100,
            height: 50
        })
        .is_async());
    }

    #[test]
    fn test_cmd_priority() {
        assert_eq!(Cmd::Tui(TuiCommand::Render).priority(), 0);
        assert_eq!(
            Cmd::SendReaction {
                target_event: create_test_event()
            }
            .priority(),
            1
        );
        assert_eq!(Cmd::ConnectToRelays { relays: vec![] }.priority(), 2);
        assert_eq!(Cmd::SaveConfig.priority(), 3);
        assert_eq!(
            Cmd::LogInfo {
                message: "test".to_string()
            }
            .priority(),
            4
        );
        assert_eq!(Cmd::None.priority(), 255);
    }

    #[test]
    fn test_cmd_batch_priority() {
        let batch = Cmd::Batch(vec![
            Cmd::LogInfo {
                message: "test".to_string(),
            }, // priority 4
            Cmd::Tui(TuiCommand::Render), // priority 0
            Cmd::SaveConfig,              // priority 3
        ]);

        // バッチの優先度は含まれるコマンドの最高優先度（最小値）
        assert_eq!(batch.priority(), 0);
    }

    #[test]
    fn test_cmd_serialization() {
        let cmd = Cmd::SendTextNote {
            content: "Hello, Nostr!".to_string(),
            tags: vec![],
        };

        let serialized = serde_json::to_string(&cmd).unwrap();
        let deserialized: Cmd = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized);
    }

    #[test]
    fn test_cmd_batch_is_async() {
        let sync_batch = Cmd::Batch(vec![
            Cmd::Tui(TuiCommand::Render),
            Cmd::LogInfo {
                message: "test".to_string(),
            },
        ]);
        assert!(!sync_batch.is_async());

        let async_batch = Cmd::Batch(vec![
            Cmd::Tui(TuiCommand::Render),
            Cmd::SendTextNote {
                content: "test".to_string(),
                tags: vec![],
            },
        ]);
        assert!(async_batch.is_async());
    }
}
