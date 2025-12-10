use nostr_sdk::prelude::*;
use sorted_vec::ReverseSortedSet;
use std::collections::HashMap;

use crate::{
    domain::collections::EventSet,
    domain::nostr::{Profile, SortableEvent},
    infrastructure::config::Config,
    integration::legacy::mode::Mode,
};

/// Unified application state
#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub timeline: TimelineState,
    pub ui: UiState,
    pub user: UserState,
    pub system: SystemState,
    pub config: ConfigState,
}

/// Configuration state - holds all user-configurable settings
#[derive(Debug, Clone, Default)]
pub struct ConfigState {
    /// Current configuration loaded from file
    pub config: Config,
}

/// Timeline-related state
#[derive(Debug, Clone)]
pub struct TimelineState {
    pub notes: ReverseSortedSet<SortableEvent>,
    pub reactions: HashMap<EventId, EventSet>,
    pub reposts: HashMap<EventId, EventSet>,
    pub zap_receipts: HashMap<EventId, EventSet>,
    pub selected_index: Option<usize>,
}

/// UI-related state
#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub show_input: bool,
    pub input_content: String,
    pub reply_to: Option<Event>,
    pub current_mode: Mode,
    pub cursor_position: CursorPosition,
    pub selection: Option<TextSelection>,
    pub pending_input_keys: Vec<crossterm::event::KeyEvent>, // Queue for stateless TextArea processing
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

/// User-related state
#[derive(Debug, Clone)]
pub struct UserState {
    pub profiles: HashMap<PublicKey, Profile>,
    pub current_user_pubkey: PublicKey,
}

/// System-related state
#[derive(Debug, Clone)]
pub struct SystemState {
    pub should_quit: bool,
    pub should_suspend: bool,
    pub fps_data: FpsData,
    pub status_message: Option<String>,
    pub is_loading: bool,
}

/// FPS measurement data
#[derive(Debug, Clone)]
pub struct FpsData {
    pub app_fps: f64,
    pub render_fps: f64,
    pub app_frames: u32,
    pub render_frames: u32,
    // Only holds computed values since Instant is not Clone
}

impl Default for TimelineState {
    fn default() -> Self {
        Self {
            notes: ReverseSortedSet::new(),
            reactions: HashMap::new(),
            reposts: HashMap::new(),
            zap_receipts: HashMap::new(),
            selected_index: None,
        }
    }
}

impl Default for UserState {
    fn default() -> Self {
        // Temporary implementation - actual initialization needs proper public key
        let dummy_keys = Keys::generate();
        Self {
            profiles: HashMap::new(),
            current_user_pubkey: dummy_keys.public_key(),
        }
    }
}

impl Default for SystemState {
    fn default() -> Self {
        Self {
            should_quit: false,
            should_suspend: false,
            fps_data: FpsData::default(),
            status_message: None,
            is_loading: true,
        }
    }
}

impl Default for FpsData {
    fn default() -> Self {
        Self {
            app_fps: 0.0,
            render_fps: 0.0,
            app_frames: 0,
            render_frames: 0,
        }
    }
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
