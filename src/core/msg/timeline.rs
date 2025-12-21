use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

/// Messages specific to TimelineState
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TimelineMsg {
    // Scroll operations
    ScrollUp,
    ScrollDown,
    ScrollToTop,
    ScrollToBottom,

    // Selection operations
    SelectNote(usize),
    DeselectNote,

    // Nostr event additions
    AddNote(Event),
    AddReaction(Event),
    AddRepost(Event),
    AddZapReceipt(Event),
}

impl TimelineMsg {
    /// Determine if this is a frequent message during debugging
    pub fn is_frequent(&self) -> bool {
        // Timeline messages are generally not frequent
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeline_msg_frequent_detection() {
        assert!(!TimelineMsg::ScrollUp.is_frequent());
        assert!(!TimelineMsg::SelectNote(0).is_frequent());
        assert!(!TimelineMsg::DeselectNote.is_frequent());
    }

    #[test]
    fn test_timeline_msg_equality() {
        assert_eq!(TimelineMsg::ScrollUp, TimelineMsg::ScrollUp);
        assert_eq!(TimelineMsg::SelectNote(5), TimelineMsg::SelectNote(5));
        assert_ne!(TimelineMsg::ScrollUp, TimelineMsg::ScrollDown);
        assert_ne!(TimelineMsg::SelectNote(1), TimelineMsg::SelectNote(2));
    }

    #[test]
    fn test_timeline_msg_serialization() -> Result<()> {
        let msg = TimelineMsg::SelectNote(42);
        let serialized = serde_json::to_string(&msg)?;
        let deserialized: TimelineMsg = serde_json::from_str(&serialized)?;
        assert_eq!(msg, deserialized);

        Ok(())
    }
}
