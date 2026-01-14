use nostr_sdk::prelude::*;

use crate::infrastructure::config::Config;

pub mod editor;
pub mod fps;
pub mod nostr;
pub mod system;
pub mod timeline;
pub mod user;

pub use editor::EditorState;
pub use fps::FpsState;
pub use nostr::NostrState;
pub use system::SystemState;
pub use timeline::TimelineState;
pub use user::UserState;

/// Unified application state
#[derive(Debug, Default)]
pub struct AppState {
    pub timeline: TimelineState,
    pub editor: EditorState,
    pub user: UserState,
    pub system: SystemState,
    pub nostr: NostrState,
    pub config: ConfigState,
    pub fps: FpsState,
}

/// Configuration state - holds all user-configurable settings
#[derive(Debug, Clone, Default)]
pub struct ConfigState {
    /// Current configuration loaded from file
    pub config: Config,
}

impl AppState {
    /// Initialize AppState with the specified public key
    pub fn new(current_user_pubkey: PublicKey) -> Self {
        Self {
            user: UserState::new_with_pubkey(current_user_pubkey),
            ..Default::default()
        }
    }

    /// Initialize AppState with the specified public key and config
    pub fn new_with_config(current_user_pubkey: PublicKey, config: Config) -> Self {
        Self {
            user: UserState::new_with_pubkey(current_user_pubkey),
            config: ConfigState { config },
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_default() {
        let state = AppState::default();

        assert_eq!(state.timeline.len(), 0);
        assert!(!state.editor.is_composing());
        assert!(state.system.is_loading());
    }

    #[test]
    fn test_app_state_new_with_pubkey() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let state = AppState::new(pubkey);

        assert_eq!(state.user.current_user_pubkey(), pubkey);
        assert_eq!(state.timeline.len(), 0);
    }
}
