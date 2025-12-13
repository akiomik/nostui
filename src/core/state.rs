use nostr_sdk::prelude::*;

use crate::infrastructure::config::Config;

pub mod nostr;
pub mod system;
pub mod timeline;
pub mod ui;
pub mod user;

pub use nostr::NostrState;
pub use system::{FpsData, SystemState};
pub use timeline::TimelineState;
pub use user::UserState;

/// Unified application state
#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub timeline: TimelineState,
    pub ui: UiState,
    pub user: UserState,
    pub system: SystemState,
    pub nostr: NostrState,
    pub config: ConfigState,
}

/// Configuration state - holds all user-configurable settings
#[derive(Debug, Clone, Default)]
pub struct ConfigState {
    /// Current configuration loaded from file
    pub config: Config,
}

/// High-level UI mode for keybindings and view switching
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiMode {
    #[default]
    Normal,
    Composing,
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

impl AppState {
    /// Initialize AppState with the specified public key
    pub fn new(current_user_pubkey: PublicKey) -> Self {
        Self {
            user: UserState {
                current_user_pubkey,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Initialize AppState with the specified public key and config
    pub fn new_with_config(current_user_pubkey: PublicKey, config: Config) -> Self {
        Self {
            user: UserState {
                current_user_pubkey,
                ..Default::default()
            },
            config: ConfigState { config },
            ..Default::default()
        }
    }

    /// Get the selected note in the timeline
    pub fn selected_note(&self) -> Option<&Event> {
        self.timeline
            .selected_index
            .and_then(|i| self.timeline.notes.get(i))
            .map(|sortable| &sortable.0.event)
    }

    /// Get the length of the timeline
    pub fn timeline_len(&self) -> usize {
        self.timeline.notes.len()
    }

    /// Check if the timeline is empty
    pub fn timeline_is_empty(&self) -> bool {
        self.timeline.notes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_event() -> Event {
        let keys = Keys::generate();
        EventBuilder::text_note("test content")
            .sign_with_keys(&keys)
            .unwrap()
    }

    #[test]
    fn test_app_state_default() {
        let state = AppState::default();

        assert_eq!(state.timeline.notes.len(), 0);
        assert!(!state.ui.show_input);
        assert!(!state.system.should_quit);
        assert!(state.system.is_loading);
    }

    #[test]
    fn test_app_state_new_with_pubkey() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let state = AppState::new(pubkey);

        assert_eq!(state.user.current_user_pubkey, pubkey);
        assert_eq!(state.timeline.notes.len(), 0);
    }

    #[test]
    fn test_selected_note() {
        let mut state = AppState::default();

        // 最初は何も選択されていない
        assert!(state.selected_note().is_none());

        // インデックスを設定してもノートがなければNone
        state.timeline.selected_index = Some(0);
        assert!(state.selected_note().is_none());
    }

    #[test]
    fn test_timeline_properties() {
        let state = AppState::default();

        assert_eq!(state.timeline_len(), 0);
        assert!(state.timeline_is_empty());
    }

    #[test]
    fn test_fps_data() {
        let fps_data = FpsData::default();

        assert_eq!(fps_data.app_fps, 0.0);
        assert_eq!(fps_data.render_fps, 0.0);
        assert_eq!(fps_data.app_frames, 0);
        assert_eq!(fps_data.render_frames, 0);
    }
}
