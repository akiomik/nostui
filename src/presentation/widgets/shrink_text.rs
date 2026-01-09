use std::borrow::Cow;

use ratatui::text::Text;

use crate::domain::text;

#[derive(Clone, Debug, Default)]
pub struct ShrinkText<'a> {
    pub content: Cow<'a, str>,
    pub width: usize,
    pub max_height: usize,
}

impl<'a> ShrinkText<'a> {
    pub fn new<T>(content: T, width: usize, max_height: usize) -> Self
    where
        T: Into<Cow<'a, str>>,
    {
        Self {
            content: content.into(),
            width,
            max_height,
        }
    }
}

impl<'a> From<ShrinkText<'a>> for Text<'a> {
    fn from(value: ShrinkText) -> Self {
        Text::from(text::truncate_text(
            &text::wrap_text(&value.content, value.width),
            value.max_height,
        ))
    }
}
