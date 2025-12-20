use crossterm::event::{Event, KeyEvent};
use tui_textarea::TextArea;

use crate::core::state::ui::TextAreaState;
use crate::core::textarea_engine::TextAreaEngine;
use crate::domain::ui::{CursorPosition, TextSelection};

/// Production engine based on tui-textarea. It constructs a temporary TextArea,
/// hydrates it from the given snapshot, applies keys, then extracts the new snapshot.
pub struct TuiTextAreaEngine;

impl TuiTextAreaEngine {
    fn restore_textarea_from_snapshot(textarea: &mut TextArea<'_>, snapshot: &TextAreaState) {
        if !snapshot.content.is_empty() {
            textarea.insert_str(&snapshot.content);
        }
        textarea.move_cursor(tui_textarea::CursorMove::Jump(
            snapshot.cursor_position.line as u16,
            snapshot.cursor_position.column as u16,
        ));
        if let Some(sel) = &snapshot.selection {
            Self::restore_selection(textarea, sel);
        }
    }

    fn extract_cursor_position(textarea: &tui_textarea::TextArea<'_>) -> CursorPosition {
        let (line, column) = textarea.cursor();
        CursorPosition { line, column }
    }

    fn extract_selection(textarea: &tui_textarea::TextArea<'_>) -> Option<TextSelection> {
        textarea.selection_range().map(|((sr, sc), (er, ec))| TextSelection {
            start: CursorPosition { line: sr, column: sc },
            end: CursorPosition { line: er, column: ec },
        })
    }

    fn restore_selection(textarea: &mut TextArea<'_>, selection: &TextSelection) {
        textarea.move_cursor(tui_textarea::CursorMove::Jump(
            selection.start.line as u16,
            selection.start.column as u16,
        ));
        textarea.start_selection();
        textarea.move_cursor(tui_textarea::CursorMove::Jump(
            selection.end.line as u16,
            selection.end.column as u16,
        ));
    }
}

impl TextAreaEngine for TuiTextAreaEngine {
    fn apply_keys(&self, snapshot: &TextAreaState, keys: &[KeyEvent]) -> TextAreaState {
        let mut textarea = TextArea::default();
        Self::restore_textarea_from_snapshot(&mut textarea, snapshot);
        for key in keys {
            textarea.input(Event::Key(*key));
        }
        let content = textarea.lines().join("\n");
        let cursor = Self::extract_cursor_position(&textarea);
        let selection = Self::extract_selection(&textarea);
        TextAreaState::new(content, cursor, selection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn applies_basic_editing_with_left_and_char() {
        let engine = TuiTextAreaEngine;
        let snap = TextAreaState::new(
            "ab".into(),
            CursorPosition { line: 0, column: 2 },
            None,
        );
        let keys = vec![
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('X'), KeyModifiers::NONE),
        ];
        let out = engine.apply_keys(&snap, &keys);
        assert_eq!(out.content, "aXb");
        assert_eq!(out.cursor_position, CursorPosition { line: 0, column: 2 });
        // original untouched
        assert_eq!(snap.content, "ab");
    }

    #[test]
    fn applies_backspace_and_selection_delete() {
        let engine = TuiTextAreaEngine;
        // Backspace at end
        let snap = TextAreaState::new(
            "ab".into(),
            CursorPosition { line: 0, column: 2 },
            None,
        );
        let out = engine.apply_keys(&snap, &[KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)]);
        assert_eq!(out.content, "a");
        assert_eq!(out.cursor_position, CursorPosition { line: 0, column: 1 });

        // Selection delete
        let with_sel = TextAreaState::new(
            "hello".into(),
            CursorPosition { line: 0, column: 5 },
            Some(TextSelection {
                start: CursorPosition { line: 0, column: 1 },
                end: CursorPosition { line: 0, column: 4 },
            }),
        );
        let out2 = engine.apply_keys(&with_sel, &[KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)]);
        assert_eq!(out2.content, "ho");
        assert_eq!(out2.cursor_position, CursorPosition { line: 0, column: 1 });
    }
}
