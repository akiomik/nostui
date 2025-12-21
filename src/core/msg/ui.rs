use crossterm::event::KeyEvent;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::domain::ui::{CursorPosition, TextSelection};

/// UI-specific messages for UiState transitions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiMsg {
    ShowNewNote,
    ShowReply(Event),
    SubmitNote,
    CancelInput,

    UpdateInputContent(String),
    UpdateInputContentWithCursor(String, CursorPosition),
    UpdateCursorPosition(CursorPosition),
    UpdateSelection(Option<TextSelection>),

    // Keep for compatibility with current TextArea path (no behavior change yet)
    ProcessTextAreaInput(KeyEvent),
}

impl UiMsg {
    pub fn is_frequent(&self) -> bool {
        // conservative: none of these are considered frequent for now
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_msg_serde() -> Result<()> {
        let msg = UiMsg::UpdateInputContent("hello".into());
        let s = serde_json::to_string(&msg)?;
        let back: UiMsg = serde_json::from_str(&s)?;
        assert_eq!(msg, back);

        Ok(())
    }
}
