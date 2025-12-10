// Removed: crossterm imports moved to raw_msg.rs
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

pub mod system;

use crate::domain::nostr::Profile;
use system::SystemMsg;

/// Domain messages representing application intent and business logic
/// These are processed by the update function and represent pure domain events
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Msg {
    // System operations (delegated to SystemState)
    System(SystemMsg),

    // Timeline operations
    ScrollUp,
    ScrollDown,
    ScrollToTop,
    ScrollToBottom,
    SelectNote(usize),
    DeselectNote,

    // Nostr domain events
    AddNote(Event),
    AddReaction(Event),
    AddRepost(Event),
    AddZapReceipt(Event),
    SendReaction(Event),
    SendRepost(Event),
    SendTextNote(String, Vec<Tag>),

    // UI operations
    ShowNewNote,
    ShowReply(Event),
    SubmitNote,
    CancelInput,
    UpdateInputContent(String),
    UpdateInputContentWithCursor(String, crate::core::state::CursorPosition),
    UpdateCursorPosition(crate::core::state::CursorPosition),
    UpdateSelection(Option<crate::core::state::TextSelection>),
    ProcessTextAreaInput(crossterm::event::KeyEvent), // Hybrid: Delegate to TextArea component

    // Legacy system messages (to be phased out)
    UpdateStatusMessage(String),
    ClearStatusMessage,
    SetLoading(bool),
    UpdateAppFps(f64),
    UpdateRenderFps(f64),
    ShowError(String),

    // User management
    UpdateProfile(PublicKey, Profile),
}

impl Msg {
    /// Helper to exclude frequent messages during debugging
    /// Domain messages are generally not frequent (raw messages handle Tick/Render)
    pub fn is_frequent(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msg_frequent_detection() {
        // Domain messages are not frequent
        assert!(!Msg::System(SystemMsg::Quit).is_frequent());
        assert!(!Msg::ScrollUp.is_frequent());
        assert!(!Msg::ShowNewNote.is_frequent());
    }

    #[test]
    fn test_msg_equality() {
        assert_eq!(Msg::System(SystemMsg::Quit), Msg::System(SystemMsg::Quit));
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
