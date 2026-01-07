use crossterm::event::KeyEvent;
use nostr_sdk::prelude::*;

use crate::domain::nostr::nip10::ReplyTagsBuilder;
use crate::domain::ui::{CursorPosition, TextSelection};

/// Complete state representation of a TextArea component
/// This struct encapsulates all mutable state that needs to be
/// preserved across TextArea recreation in the stateless approach
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextAreaState {
    /// The complete text content
    pub content: String,
    /// Current cursor position within the text
    pub cursor_position: CursorPosition,
    /// Active text selection range, if any
    pub selection: Option<TextSelection>,
}

impl TextAreaState {
    /// Create new TextAreaState
    pub fn new(
        content: String,
        cursor_position: CursorPosition,
        selection: Option<TextSelection>,
    ) -> Self {
        Self {
            content,
            cursor_position,
            selection,
        }
    }

    /// Create empty TextAreaState
    pub fn empty() -> Self {
        Default::default()
    }

    pub fn content_length(&self) -> usize {
        self.content.len()
    }

    pub fn has_content(&self) -> bool {
        !self.content.trim().is_empty()
    }
}

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
    pub textarea: TextAreaState,
    pub reply_to: Option<Event>,
    pub current_mode: UiMode,
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
        self.is_composing() && self.textarea.has_content()
    }

    pub fn is_reply(&self) -> bool {
        self.reply_to.is_some()
    }

    pub fn reply_target(&self) -> Option<&Event> {
        self.reply_to.as_ref()
    }

    pub fn prepare_submit_data(&self) -> Option<SubmitData> {
        if !self.can_submit_input() {
            return None;
        }

        let content = self.textarea.content.clone();
        let tags = if let Some(ref reply_to) = self.reply_to {
            ReplyTagsBuilder::build(reply_to.clone())
        } else {
            vec![]
        };

        Some(SubmitData { content, tags })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_event() -> Result<Event> {
        let keys = Keys::generate();
        EventBuilder::text_note("t")
            .sign_with_keys(&keys)
            .map_err(|e| e.into())
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_prepare_submit_data() -> Result<()> {
        let mut ui = UiState {
            textarea: TextAreaState::new("Hello, Nostr!".to_string(), Default::default(), None),
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
        ui.reply_to = Some(create_event()?);
        let submit_data = ui.prepare_submit_data();
        assert!(submit_data.is_some());
        let data = submit_data.unwrap();
        assert!(!data.tags.is_empty()); // Should have reply tags

        // Cannot submit when input hidden
        ui.current_mode = UiMode::Normal;
        let submit_data = ui.prepare_submit_data();
        assert!(submit_data.is_none());

        Ok(())
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
