use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::msg::Msg;

/// UI (TUI) specific sub-commands executed by the host/runtime
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TuiCommand {
    // Render is orchestrated exclusively by AppRunner, not via TuiCommand.
    // Render requests are coalesced and delivered via a bounded render_req_sender signal.
    // This removes the duplicate path (via TuiService) to avoid contention and spamming.
    Resize { width: u16, height: u16 },
}

/// Elm-like command definitions
/// Represents side effects (network communication, file I/O, etc.)
/// Note on duplication: Some command names also appear in infrastructure-level commands (e.g. NostrCommand).
/// This is intentional — Cmd captures application intent (what to do), while infrastructure commands capture
/// execution details (how to do it). Keeping both layers separate improves testability and allows swapping
/// infrastructure without leaking external types into the domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cmd {
    // Nostr-related commands
    SendReaction {
        target_event: Event,
    },
    SendRepost {
        target_event: Event,
    },
    SendTextNote {
        content: String,
        tags: Vec<Tag>,
    },
    ConnectToRelays {
        relays: Vec<String>,
    },
    DisconnectFromRelays,
    SubscribeToTimeline,

    // UI-related commands
    Tui(TuiCommand),
    /// Request a render; delivered via bounded render_req_sender and coalesced by AppRunner
    RequestRender,

    // File/configuration related
    SaveConfig,
    LoadConfig,

    // Logging related
    LogError {
        message: String,
    },
    LogInfo {
        message: String,
    },

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
            | Cmd::RequestRender
            | Cmd::LogError { .. }
            | Cmd::LogInfo { .. }
            | Cmd::None => false,

            Cmd::Batch(cmds) => cmds.iter().any(|cmd| cmd.is_async()),
        }
    }

    /// Get command priority (smaller numbers = higher priority)
    pub fn priority(&self) -> u8 {
        match self {
            // UI-related has highest priority
            Cmd::Tui(..) | Cmd::RequestRender => 0,

            // User actions have high priority
            Cmd::SendReaction { .. } | Cmd::SendRepost { .. } | Cmd::SendTextNote { .. } => 1,

            // Network-related has medium priority
            Cmd::ConnectToRelays { .. } | Cmd::DisconnectFromRelays | Cmd::SubscribeToTimeline => 2,

            // File operations have low priority
            Cmd::SaveConfig | Cmd::LoadConfig => 3,

            // Logging have lowest priority
            Cmd::LogError { .. } | Cmd::LogInfo { .. } => 4,

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
        let original_cmd = Cmd::SaveConfig;
        let cmd = Cmd::batch(vec![original_cmd.clone()]);
        assert_eq!(cmd, original_cmd);
    }

    #[test]
    fn test_cmd_batch_multiple() {
        // Batch should wrap when there are 2+ commands
        let cmds = vec![Cmd::SaveConfig, Cmd::LoadConfig];
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

        assert!(!Cmd::Tui(TuiCommand::Resize {
            width: 100,
            height: 50
        })
        .is_async());
    }

    #[test]
    fn test_cmd_priority() {
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
            Cmd::SaveConfig, // priority 3
        ]);

        // Batch priority should be the minimum of its children (lower = higher priority)
        assert_eq!(batch.priority(), 3);
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
        let sync_batch = Cmd::Batch(vec![Cmd::LogInfo {
            message: "test".to_string(),
        }]);
        assert!(!sync_batch.is_async());

        let async_batch = Cmd::Batch(vec![Cmd::SendTextNote {
            content: "test".to_string(),
            tags: vec![],
        }]);
        assert!(async_batch.is_async());
    }
}
