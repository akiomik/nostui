// Removed: crossterm imports moved to raw_msg.rs
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

pub mod nostr;
pub mod system;
pub mod timeline;
pub mod ui;
pub mod user;

use crate::domain::nostr::Profile;
use nostr::NostrMsg;
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

    // Nostr operations (delegated to NostrMsg)
    Nostr(NostrMsg),

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
        assert!(!Msg::Timeline(TimelineMsg::ScrollUp).is_frequent());
        assert!(!Msg::Ui(UiMsg::ShowNewNote).is_frequent());
    }

    #[test]
    fn test_msg_equality() {
        assert_eq!(Msg::System(SystemMsg::Quit), Msg::System(SystemMsg::Quit));
        assert_eq!(
            Msg::Timeline(TimelineMsg::ScrollUp),
            Msg::Timeline(TimelineMsg::ScrollUp)
        );
        assert_ne!(
            Msg::Timeline(TimelineMsg::ScrollUp),
            Msg::Timeline(TimelineMsg::ScrollDown)
        );
    }

    #[test]
    fn test_msg_serialization() -> Result<()> {
        let msg = Msg::System(SystemMsg::UpdateStatusMessage("test".to_string()));
        let serialized = serde_json::to_string(&msg)?;
        let deserialized: Msg = serde_json::from_str(&serialized)?;
        assert_eq!(msg, deserialized);

        Ok(())
    }
}
