use crossterm::event::KeyEvent;
use nostr_sdk::prelude::*;

use crate::core::cmd::Cmd;
use crate::core::msg::ui::UiMsg;
use crate::domain::nostr::nip10::ReplyTagsBuilder;
use crate::domain::ui::{CursorPosition, TextSelection};

/// High-level UI mode for keybindings and view switching
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiMode {
    #[default]
    Normal,
    Composing,
}

/// Data required for submitting a note
#[derive(Debug, Clone, PartialEq)]
pub struct SubmitData {
    pub content: String,
    pub tags: Vec<nostr_sdk::Tag>,
}

/// UI-related state
#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub input_content: String,
    pub reply_to: Option<Event>,
    pub current_mode: UiMode,
    pub cursor_position: CursorPosition,
    pub selection: Option<TextSelection>,
    pub pending_input_keys: Vec<KeyEvent>, // Queue for stateless TextArea processing
}

impl UiState {
    pub fn is_composing(&self) -> bool {
        self.current_mode == UiMode::Composing
    }

    pub fn is_normal(&self) -> bool {
        self.current_mode == UiMode::Normal
    }

    pub fn can_submit_input(&self) -> bool {
        self.is_composing() && self.has_input_content()
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
        !self.input_content.trim().is_empty()
    }

    pub fn prepare_submit_data(&self) -> Option<SubmitData> {
        if !self.can_submit_input() {
            return None;
        }

        let content = self.input_content.clone();
        let tags = if let Some(ref reply_to) = self.reply_to {
            ReplyTagsBuilder::build(reply_to.clone())
        } else {
            vec![]
        };

        Some(SubmitData { content, tags })
    }

    /// UiState-specific update function performing pure state transitions
    /// and returning generated commands (currently none; coordinator emits commands)
    pub fn update(&mut self, msg: UiMsg) -> Vec<Cmd> {
        match msg {
            UiMsg::ShowNewNote => {
                self.reply_to = None;
                self.current_mode = UiMode::Composing;
                self.input_content.clear();
                self.cursor_position = Default::default();
                self.selection = None;
                vec![]
            }

            UiMsg::ShowReply(target_event) => {
                self.reply_to = Some(target_event);
                self.current_mode = UiMode::Composing;
                self.input_content.clear();
                self.cursor_position = Default::default();
                self.selection = None;
                vec![]
            }

            UiMsg::CancelInput => {
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
                self.cursor_position = pos;
                vec![]
            }

            UiMsg::UpdateCursorPosition(pos) => {
                self.cursor_position = pos;
                vec![]
            }

            UiMsg::UpdateSelection(sel) => {
                self.selection = sel;
                vec![]
            }

            UiMsg::SubmitNote => {
                if let Some(submit_data) = self.prepare_submit_data() {
                    let mut cmds = self.update(UiMsg::CancelInput);
                    cmds.push(Cmd::SendTextNote {
                        content: submit_data.content,
                        tags: submit_data.tags,
                    });
                    cmds
                } else {
                    vec![]
                }
            }

            // Keep legacy textarea path intact (no-op here; AppState handles it)
            UiMsg::ProcessTextAreaInput(_) => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(ui.is_composing());
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
        assert!(ui.is_composing());
        assert!(ui.reply_to.as_ref().is_some());
        assert_eq!(ui.reply_to.as_ref().unwrap().id, ev_id);
        assert!(ui.input_content.is_empty());
    }

    #[test]
    fn test_cancel_input_hides_and_resets() {
        let mut ui = UiState {
            current_mode: UiMode::Composing,
            input_content: "x".into(),
            reply_to: Some(create_event()),
            ..Default::default()
        };
        let _ = ui.update(UiMsg::CancelInput);
        assert!(ui.is_normal());
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
        let _ = ui.update(UiMsg::UpdateCursorPosition(CursorPosition {
            line: 1,
            column: 2,
        }));
        assert_eq!(ui.cursor_position.line, 1);
        assert_eq!(ui.cursor_position.column, 2);

        let sel = TextSelection {
            start: CursorPosition { line: 0, column: 1 },
            end: CursorPosition { line: 2, column: 3 },
        };
        let _ = ui.update(UiMsg::UpdateSelection(Some(sel)));
        let s = ui.selection.unwrap();
        assert_eq!(s.start.line, 0);
        assert_eq!(s.start.column, 1);
        assert_eq!(s.end.line, 2);
        assert_eq!(s.end.column, 3);
    }

    #[test]
    fn test_prepare_submit_data() {
        let mut ui = UiState {
            input_content: "Hello, Nostr!".to_string(),
            current_mode: UiMode::Composing,
            ..Default::default()
        };

        // Basic submission (new note)
        let submit_data = ui.prepare_submit_data();
        assert!(submit_data.is_some());
        let data = submit_data.unwrap();
        assert_eq!(data.content, "Hello, Nostr!");
        assert!(data.tags.is_empty()); // No reply tags

        // Reply submission
        ui.reply_to = Some(create_event());
        let submit_data = ui.prepare_submit_data();
        assert!(submit_data.is_some());
        let data = submit_data.unwrap();
        assert!(!data.tags.is_empty()); // Should have reply tags

        // Cannot submit when input hidden
        ui.current_mode = UiMode::Normal;
        let submit_data = ui.prepare_submit_data();
        assert!(submit_data.is_none());
    }

    #[test]
    fn test_submit_data_equality() {
        let data1 = SubmitData {
            content: "Hello".to_string(),
            tags: vec![],
        };
        let data2 = SubmitData {
            content: "Hello".to_string(),
            tags: vec![],
        };
        let data3 = SubmitData {
            content: "World".to_string(),
            tags: vec![],
        };

        assert_eq!(data1, data2);
        assert_ne!(data1, data3);
    }
}
