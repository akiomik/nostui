use crossterm::event::KeyEvent;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::nostr::Profile;

/// Elm-like message definitions
/// Represents events that occur within the application, replacing Action
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Msg {
    // System messages
    Tick,
    Render,
    Resize(u16, u16),
    Quit,
    Suspend,
    Resume,

    // User input
    Key(KeyEvent),

    // Timeline operations
    ScrollUp,
    ScrollDown,
    ScrollToTop,
    ScrollToBottom,
    SelectNote(Option<usize>),

    // Nostr events
    ReceiveEvent(Event),
    SendReaction(Event),
    SendRepost(Event),
    SendTextNote(String, Vec<Tag>),

    // UI operations
    ToggleInput,
    ShowNewNote,
    ShowReply(Event),
    SubmitNote,
    CancelInput,
    UpdateInputContent(String),

    // Status
    UpdateStatusMessage(String),
    ClearStatusMessage,
    SetLoading(bool),

    // FPS updates
    UpdateAppFps(f64),
    UpdateRenderFps(f64),

    // Profile
    UpdateProfile(PublicKey, Profile),

    // Error
    Error(String),
}

impl Msg {
    /// Helper to exclude frequent messages during debugging
    pub fn is_frequent(&self) -> bool {
        matches!(self, Msg::Tick | Msg::Render)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msg_frequent_detection() {
        assert!(Msg::Tick.is_frequent());
        assert!(Msg::Render.is_frequent());
        assert!(!Msg::Quit.is_frequent());
        assert!(!Msg::ScrollUp.is_frequent());
    }

    #[test]
    fn test_msg_equality() {
        assert_eq!(Msg::Quit, Msg::Quit);
        assert_eq!(Msg::ScrollUp, Msg::ScrollUp);
        assert_ne!(Msg::ScrollUp, Msg::ScrollDown);
    }

    #[test]
    fn test_msg_serialization() {
        let msg = Msg::UpdateStatusMessage("test".to_string());
        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: Msg = serde_json::from_str(&serialized).unwrap();
        assert_eq!(msg, deserialized);
    }
}
