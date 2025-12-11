// Removed: crossterm imports moved to raw_msg.rs
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

pub mod system;
pub mod timeline;
pub mod ui;
pub mod user;

use crate::domain::nostr::Profile;
use system::SystemMsg;
use timeline::TimelineMsg;
use ui::UiMsg;
use user::UserMsg;

/// Domain messages representing application intent and business logic
/// These are processed by the update function and represent pure domain events
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Msg {
    // System operations (delegated to SystemState)
    System(SystemMsg),

    // Timeline operations (delegated to TimelineState)
    Timeline(TimelineMsg),

    // User operations (delegated to UserState)
    User(UserMsg),

    // UI operations (new path)
    Ui(UiMsg),

    // Legacy timeline messages (to be phased out)
    ScrollUp,
    ScrollDown,
    ScrollToTop,
    ScrollToBottom,
    SelectNote(usize),
    DeselectNote,
    AddNote(Event),
    AddReaction(Event),
    AddRepost(Event),
    AddZapReceipt(Event),
    SendReaction(Event),
    SendRepost(Event),
    SendTextNote(String, Vec<Tag>),

    // Legacy system messages (to be phased out)
    UpdateStatusMessage(String),
    ClearStatusMessage,
    SetLoading(bool),
    UpdateAppFps(f64),
    UpdateRenderFps(f64),
    ShowError(String),

    // Legacy user messages (to be phased out)
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
        use crate::core::msg::ui::UiMsg;
        assert!(!Msg::Ui(UiMsg::ShowNewNote).is_frequent());
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
