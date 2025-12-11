use crossterm::event::KeyEvent;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

/// Canonical cursor position type for UI messaging
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CursorPosition {
    pub line: usize,
    pub column: usize,
}

/// Canonical selection type for UI messaging
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSelection {
    pub start: CursorPosition,
    pub end: CursorPosition,
}

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

// Adapters between canonical UI types and current state types (row/col)
impl From<crate::core::state::CursorPosition> for CursorPosition {
    fn from(v: crate::core::state::CursorPosition) -> Self {
        CursorPosition {
            line: v.row,
            column: v.col,
        }
    }
}

impl From<CursorPosition> for crate::core::state::CursorPosition {
    fn from(v: CursorPosition) -> Self {
        crate::core::state::CursorPosition {
            row: v.line,
            col: v.column,
        }
    }
}

impl From<crate::core::state::TextSelection> for TextSelection {
    fn from(v: crate::core::state::TextSelection) -> Self {
        TextSelection {
            start: v.start.into(),
            end: v.end.into(),
        }
    }
}

impl From<TextSelection> for crate::core::state::TextSelection {
    fn from(v: TextSelection) -> Self {
        crate::core::state::TextSelection {
            start: v.start.into(),
            end: v.end.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_adapter_roundtrip() {
        let s = crate::core::state::CursorPosition { row: 3, col: 7 };
        let u: CursorPosition = s.clone().into();
        assert_eq!(u.line, 3);
        assert_eq!(u.column, 7);
        let s2: crate::core::state::CursorPosition = u.into();
        assert_eq!(s, s2);
    }

    #[test]
    fn text_selection_adapter_roundtrip() {
        let s = crate::core::state::TextSelection {
            start: crate::core::state::CursorPosition { row: 1, col: 2 },
            end: crate::core::state::CursorPosition { row: 3, col: 4 },
        };
        let u: TextSelection = s.clone().into();
        assert_eq!(u.start.line, 1);
        assert_eq!(u.start.column, 2);
        assert_eq!(u.end.line, 3);
        assert_eq!(u.end.column, 4);
        let s2: crate::core::state::TextSelection = u.into();
        assert_eq!(s, s2);
    }

    #[test]
    fn ui_msg_serde() {
        let msg = UiMsg::UpdateInputContent("hello".into());
        let s = serde_json::to_string(&msg).unwrap();
        let back: UiMsg = serde_json::from_str(&s).unwrap();
        assert_eq!(msg, back);
    }
}
