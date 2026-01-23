use ratatui::prelude::*;
use ratatui::widgets::Widget;

use crate::model::editor::Editor;

pub struct EditorWidget<'a> {
    editor: &'a Editor<'a>,
}

impl<'a> EditorWidget<'a> {
    pub fn new(editor: &'a Editor<'a>) -> Self {
        Self { editor }
    }
}

impl<'a> Widget for EditorWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        if !self.editor.is_active() {
            return;
        }

        self.editor.textarea().render(area, buf);
    }
}
