use crossterm::event::KeyEvent;
use nostr_sdk::prelude::*;
use ratatui::widgets::{Block, Borders};
use tui_textarea::TextArea;

use crate::domain::{nostr::Profile, text::shorten_npub};

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    ComposingStarted,
    ReplyStarted {
        to: Box<Event>,
        profile: Box<Option<Profile>>,
    },
    ComposingCanceled,
    KeyEventReceived {
        event: KeyEvent,
    },
}

#[derive(Debug, Clone, Default)]
pub struct Editor<'a> {
    reply_to: Option<Event>,
    is_active: bool,
    textarea: TextArea<'a>,
}

impl<'a> Editor<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if editor is active
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Returns true if currently composing a reply
    pub fn is_reply(&self) -> bool {
        self.reply_to.is_some()
    }

    /// Returns the event being replied to, if any
    pub fn reply_target(&self) -> Option<&Event> {
        self.reply_to.as_ref()
    }

    /// Returns the textarea
    pub fn textarea(&self) -> &TextArea<'_> {
        &self.textarea
    }

    /// Get the current content from the TextArea
    pub fn get_content(&self) -> String {
        self.textarea.lines().join("\n")
    }

    /// Clear the TextArea content
    pub fn clear_content(&mut self) {
        // NOTE: We recreate the TextArea instance instead of using select_all() + delete_str()
        // because that approach has a bug in tui-textarea where undo() after deletion
        // restores an invalid cursor position and causes a panic.
        // Additionally, recreating the instance is the only way to clear the undo/redo history.
        //
        // See:
        // * https://github.com/rhysd/tui-textarea/issues/96
        // * https://github.com/rhysd/tui-textarea/issues/121
        let block = self.textarea.block().cloned();
        self.textarea = TextArea::default();
        if let Some(block) = block {
            self.textarea.set_block(block);
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::ComposingStarted => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .title("New note: Press ESC to close");
                self.textarea.set_block(block);
                self.clear_content();
                self.is_active = true;
                self.reply_to = None;
            }
            Message::ReplyStarted { to, profile } => {
                let reply_target_name =
                    profile.map(|profile| profile.name()).unwrap_or_else(|| {
                        let Ok(npub) = to.pubkey.to_bech32();
                        shorten_npub(npub)
                    });
                let block = Block::default().borders(Borders::ALL).title(format!(
                    "Replying to {reply_target_name}: Press ESC to close"
                ));
                self.textarea.set_block(block);
                self.clear_content();
                self.is_active = true;
                self.reply_to = Some(*to);
            }
            Message::ComposingCanceled => {
                self.is_active = false;
            }
            Message::KeyEventReceived { event } => {
                if self.is_active {
                    self.textarea.input(event);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::nostr::Profile;
    use crossterm::event::{KeyCode, KeyModifiers};
    use nostr_sdk::prelude::{Event, EventBuilder, Keys, Metadata, Timestamp};

    fn create_test_event() -> Event {
        let keys = Keys::generate();
        EventBuilder::text_note("test content")
            .sign_with_keys(&keys)
            .expect("Failed to create test event")
    }

    fn create_test_profile() -> Profile {
        let pubkey = Keys::generate().public_key();
        let created_at = Timestamp::now();
        let metadata = Metadata::new().display_name("TestUser");
        Profile::new(pubkey, created_at, metadata)
    }

    fn create_key_event(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_new_editor_default_state() {
        let editor = Editor::new();
        assert!(!editor.is_active());
        assert!(!editor.is_reply());
        assert_eq!(editor.reply_target(), None);
        assert_eq!(editor.get_content(), "");
    }

    #[test]
    fn test_composing_started() {
        let mut editor = Editor::new();
        editor.update(Message::ComposingStarted);

        assert!(editor.is_active());
        assert!(!editor.is_reply());
        assert_eq!(editor.reply_target(), None);
        assert_eq!(editor.get_content(), "");
    }

    #[test]
    fn test_reply_started_with_profile() {
        let mut editor = Editor::new();
        let event = create_test_event();
        let profile = create_test_profile();

        editor.update(Message::ReplyStarted {
            to: Box::new(event.clone()),
            profile: Box::new(Some(profile)),
        });

        assert!(editor.is_active());
        assert!(editor.is_reply());
        assert_eq!(editor.reply_target(), Some(&event));
    }

    #[test]
    fn test_reply_started_without_profile() {
        let mut editor = Editor::new();
        let event = create_test_event();

        editor.update(Message::ReplyStarted {
            to: Box::new(event.clone()),
            profile: Box::new(None),
        });

        assert!(editor.is_active());
        assert!(editor.is_reply());
        assert_eq!(editor.reply_target(), Some(&event));
    }

    #[test]
    fn test_composing_canceled() {
        let mut editor = Editor::new();
        editor.update(Message::ComposingStarted);
        assert!(editor.is_active());

        editor.update(Message::ComposingCanceled);
        assert!(!editor.is_active());
    }

    #[test]
    fn test_composing_started_clears_reply_state() {
        let mut editor = Editor::new();
        let event = create_test_event();

        // Start a reply
        editor.update(Message::ReplyStarted {
            to: Box::new(event),
            profile: Box::new(None),
        });
        assert!(editor.is_reply());

        // Start new composition
        editor.update(Message::ComposingStarted);
        assert!(!editor.is_reply());
        assert_eq!(editor.reply_target(), None);
    }

    #[test]
    fn test_key_event_received_when_active() {
        let mut editor = Editor::new();
        editor.update(Message::ComposingStarted);

        let key_event = create_key_event(KeyCode::Char('a'));
        editor.update(Message::KeyEventReceived { event: key_event });

        assert_eq!(editor.get_content(), "a");
    }

    #[test]
    fn test_key_event_received_when_inactive() {
        let mut editor = Editor::new();
        // Editor is inactive by default

        let key_event = create_key_event(KeyCode::Char('a'));
        editor.update(Message::KeyEventReceived { event: key_event });

        // Content should remain empty since editor is inactive
        assert_eq!(editor.get_content(), "");
    }

    #[test]
    fn test_multiple_key_events() {
        let mut editor = Editor::new();
        editor.update(Message::ComposingStarted);

        editor.update(Message::KeyEventReceived {
            event: create_key_event(KeyCode::Char('h')),
        });
        editor.update(Message::KeyEventReceived {
            event: create_key_event(KeyCode::Char('i')),
        });

        assert_eq!(editor.get_content(), "hi");
    }

    #[test]
    fn test_clear_content() {
        let mut editor = Editor::new();
        editor.update(Message::ComposingStarted);

        // Add some content
        editor.update(Message::KeyEventReceived {
            event: create_key_event(KeyCode::Char('t')),
        });
        editor.update(Message::KeyEventReceived {
            event: create_key_event(KeyCode::Char('e')),
        });
        editor.update(Message::KeyEventReceived {
            event: create_key_event(KeyCode::Char('s')),
        });
        editor.update(Message::KeyEventReceived {
            event: create_key_event(KeyCode::Char('t')),
        });

        assert_eq!(editor.get_content(), "test");

        // Clear content
        editor.clear_content();
        assert_eq!(editor.get_content(), "");
    }

    #[test]
    fn test_composing_started_clears_previous_content() {
        let mut editor = Editor::new();
        editor.update(Message::ComposingStarted);

        // Add some content
        editor.update(Message::KeyEventReceived {
            event: create_key_event(KeyCode::Char('x')),
        });
        assert_eq!(editor.get_content(), "x");

        // Start composing again
        editor.update(Message::ComposingStarted);
        assert_eq!(editor.get_content(), "");
    }

    #[test]
    fn test_reply_started_clears_previous_content() {
        let mut editor = Editor::new();
        editor.update(Message::ComposingStarted);

        // Add some content
        editor.update(Message::KeyEventReceived {
            event: create_key_event(KeyCode::Char('x')),
        });
        assert_eq!(editor.get_content(), "x");

        // Start a reply
        let event = create_test_event();
        editor.update(Message::ReplyStarted {
            to: Box::new(event),
            profile: Box::new(None),
        });
        assert_eq!(editor.get_content(), "");
    }

    #[test]
    fn test_textarea_reference() {
        let editor = Editor::new();
        let textarea = editor.textarea();
        assert_eq!(textarea.lines().len(), 1);
    }

    #[test]
    fn test_cancel_preserves_content() {
        let mut editor = Editor::new();
        editor.update(Message::ComposingStarted);

        // Add some content
        editor.update(Message::KeyEventReceived {
            event: create_key_event(KeyCode::Char('t')),
        });
        assert_eq!(editor.get_content(), "t");

        // Cancel composing
        editor.update(Message::ComposingCanceled);
        assert!(!editor.is_active());

        // Content should still be there
        assert_eq!(editor.get_content(), "t");
    }

    #[test]
    fn test_reply_target_after_multiple_replies() {
        let mut editor = Editor::new();
        let event1 = create_test_event();
        let event2 = create_test_event();

        // Start first reply
        editor.update(Message::ReplyStarted {
            to: Box::new(event1.clone()),
            profile: Box::new(None),
        });
        assert_eq!(editor.reply_target(), Some(&event1));

        // Start second reply
        editor.update(Message::ReplyStarted {
            to: Box::new(event2.clone()),
            profile: Box::new(None),
        });
        assert_eq!(editor.reply_target(), Some(&event2));
    }

    #[test]
    fn test_ctrl_u_as_first_key_event_when_active() {
        // NOTE: This is a regression test for a tui-textarea bug that occurred
        // when using select_all() + delete_str() to clear content.
        // Ctrl+U (undo) would panic with "cursor (1, 0) exceeds max lines 1".
        // The bug was fixed by recreating the TextArea instance in clear_content()
        // instead of using select_all() + delete_str().
        // See: https://github.com/rhysd/tui-textarea/issues/121
        let mut editor = Editor::new();
        editor.update(Message::ComposingStarted);

        // Send Ctrl+U as the first key event (this should not panic)
        let key_event = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        editor.update(Message::KeyEventReceived { event: key_event });

        // Content should remain empty and editor should still be active
        assert_eq!(editor.get_content(), "");
        assert!(editor.is_active());
    }
}
