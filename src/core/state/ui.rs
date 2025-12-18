use nostr_sdk::prelude::*;

use crate::core::cmd::Cmd;
use crate::core::msg::ui::UiMsg;

/// High-level UI mode for keybindings and view switching
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiMode {
    #[default]
    Normal,
    Composing,
}

/// Cursor position in text
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct CursorPosition {
    pub row: usize,
    pub col: usize,
}

/// Text selection range
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TextSelection {
    pub start: CursorPosition,
    pub end: CursorPosition,
}

/// UI-related state
#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub show_input: bool, // TODO: Remove after migrating all checks to UiMode
    pub input_content: String,
    pub reply_to: Option<Event>,
    pub current_mode: UiMode,
    pub cursor_position: CursorPosition,
    pub selection: Option<TextSelection>,
    pub pending_input_keys: Vec<crossterm::event::KeyEvent>, // Queue for stateless TextArea processing
}

impl UiState {
    pub fn is_input_shown(&self) -> bool {
        self.show_input
    }

    pub fn is_reply(&self) -> bool {
        self.reply_to.is_some()
    }

    pub fn reply_target(&self) -> Option<&Event> {
        self.reply_to.as_ref()
    }

    pub fn input_length(&self) -> usize {
        self.input_content.len()
    }

    pub fn has_input_content(&self) -> bool {
        !self.input_content.is_empty()
    }

    /// UiState-specific update function performing pure state transitions
    /// and returning generated commands (currently none; coordinator emits commands)
    pub fn update(&mut self, msg: UiMsg) -> Vec<Cmd> {
        match msg {
            UiMsg::ShowNewNote => {
                self.reply_to = None;
                self.show_input = true; // TODO: migrate callers to UiMode and remove this flag
                self.current_mode = UiMode::Composing;
                self.input_content.clear();
                self.cursor_position = Default::default();
                self.selection = None;
                vec![]
            }
            UiMsg::ShowReply(target_event) => {
                self.reply_to = Some(target_event);
                self.show_input = true; // TODO: migrate callers to UiMode and remove this flag
                self.current_mode = UiMode::Composing;
                self.input_content.clear();
                self.cursor_position = Default::default();
                self.selection = None;
                vec![]
            }
            UiMsg::CancelInput => {
                self.show_input = false; // TODO: migrate callers to UiMode and remove this flag
                self.current_mode = UiMode::Normal;
                self.reply_to = None;
                self.input_content.clear();
                self.cursor_position = Default::default();
                self.selection = None;
                vec![]
            }

            // Content/cursor/selection updates
            UiMsg::UpdateInputContent(content) => {
                self.input_content = content;
                vec![]
            }
            UiMsg::UpdateInputContentWithCursor(content, pos) => {
                self.input_content = content;
                self.cursor_position = pos.into();
                vec![]
            }
            UiMsg::UpdateCursorPosition(pos) => {
                self.cursor_position = pos.into();
                vec![]
            }
            UiMsg::UpdateSelection(sel) => {
                self.selection = sel.map(Into::into);
                vec![]
            }

            // Not handled here: coordinator owns integration/commands
            UiMsg::SubmitNote => vec![],

            // Keep legacy textarea path intact (no-op here; AppState handles it)
            UiMsg::ProcessTextAreaInput(_) => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::state::UiState;

    fn create_event() -> Event {
        let keys = Keys::generate();
        EventBuilder::text_note("t").sign_with_keys(&keys).unwrap()
    }

    #[test]
    fn test_show_new_note_resets_and_shows_input() {
        let mut ui = UiState {
            input_content: "abc".into(),
            reply_to: Some(create_event()),
            ..Default::default()
        };
        let cmds = ui.update(UiMsg::ShowNewNote);
        assert!(cmds.is_empty());
        assert!(ui.show_input);
        assert!(ui.reply_to.is_none());
        assert!(ui.input_content.is_empty());
        assert_eq!(ui.cursor_position, Default::default());
        assert!(ui.selection.is_none());
    }

    #[test]
    fn test_show_reply_sets_target_and_shows_input() {
        let mut ui = UiState::default();
        let ev = create_event();
        let ev_id = ev.id;
        let _ = ui.update(UiMsg::ShowReply(ev));
        assert!(ui.show_input);
        assert!(ui.reply_to.as_ref().is_some());
        assert_eq!(ui.reply_to.as_ref().unwrap().id, ev_id);
        assert!(ui.input_content.is_empty());
    }

    #[test]
    fn test_cancel_input_hides_and_resets() {
        let mut ui = UiState {
            show_input: true,
            input_content: "x".into(),
            reply_to: Some(create_event()),
            ..Default::default()
        };
        let _ = ui.update(UiMsg::CancelInput);
        assert!(!ui.show_input);
        assert!(ui.reply_to.is_none());
        assert!(ui.input_content.is_empty());
        assert!(ui.selection.is_none());
    }

    #[test]
    fn test_update_input_content() {
        let mut ui = UiState::default();
        let _ = ui.update(UiMsg::UpdateInputContent("hello".into()));
        assert_eq!(ui.input_content, "hello");
    }

    #[test]
    fn test_update_cursor_and_selection() {
        let mut ui = UiState::default();
        let _ = ui.update(UiMsg::UpdateCursorPosition(
            crate::core::msg::ui::CursorPosition { line: 1, column: 2 },
        ));
        assert_eq!(ui.cursor_position.row, 1);
        assert_eq!(ui.cursor_position.col, 2);

        let sel = crate::core::msg::ui::TextSelection {
            start: crate::core::msg::ui::CursorPosition { line: 0, column: 1 },
            end: crate::core::msg::ui::CursorPosition { line: 2, column: 3 },
        };
        let _ = ui.update(UiMsg::UpdateSelection(Some(sel)));
        let s = ui.selection.unwrap();
        assert_eq!(s.start.row, 0);
        assert_eq!(s.start.col, 1);
        assert_eq!(s.end.row, 2);
        assert_eq!(s.end.col, 3);
    }
}
