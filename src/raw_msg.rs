use crossterm::event::KeyEvent;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

/// Raw messages from external sources (input, network, system)
/// These represent unprocessed external events that need to be translated to domain events
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RawMsg {
    // System events
    Tick,
    Render,
    Resize(u16, u16),
    Quit,
    Suspend,
    Resume,

    // User input (raw keyboard events)
    Key(KeyEvent),

    // Network events (raw Nostr events)
    ReceiveEvent(Event),

    // FPS updates
    AppFpsUpdate(f64),
    RenderFpsUpdate(f64),

    // System status
    Error(String),
}

impl RawMsg {
    /// Helper to exclude frequent messages during debugging
    pub fn is_frequent(&self) -> bool {
        matches!(self, RawMsg::Tick | RawMsg::Render)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn test_raw_msg_frequent_detection() {
        assert!(RawMsg::Tick.is_frequent());
        assert!(RawMsg::Render.is_frequent());
        assert!(!RawMsg::Quit.is_frequent());
        assert!(!RawMsg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).is_frequent());
        assert!(!RawMsg::AppFpsUpdate(60.0).is_frequent());
    }

    #[test]
    fn test_raw_msg_equality() {
        assert_eq!(RawMsg::Quit, RawMsg::Quit);
        assert_eq!(RawMsg::Tick, RawMsg::Tick);
        assert_ne!(RawMsg::Tick, RawMsg::Render);
    }

    #[test]
    fn test_raw_msg_serialization() {
        let msg = RawMsg::Error("test error".to_string());
        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: RawMsg = serde_json::from_str(&serialized).unwrap();
        assert_eq!(msg, deserialized);
    }
}
