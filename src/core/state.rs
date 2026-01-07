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
pub use ui::UiState;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_default() {
        let state = AppState::default();

        assert_eq!(state.timeline.len(), 0);
        assert!(!state.ui.is_composing());
        assert!(!state.system.should_quit);
        assert!(state.system.is_loading);
    }

    #[test]
    fn test_app_state_new_with_pubkey() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let state = AppState::new(pubkey);

        assert_eq!(state.user.current_user_pubkey, pubkey);
        assert_eq!(state.timeline.len(), 0);
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
