use color_eyre::eyre::Result;
use ratatui::{prelude::*, widgets::*};

use crate::{
    core::state::AppState, infrastructure::tui::Frame, presentation::widgets::public_key::PublicKey,
};

/// Elm-architecture compatible status bar component
/// This component is purely functional - it receives state and renders status information
/// No internal state management
#[derive(Debug, Clone)]
pub struct StatusBar;

impl StatusBar {
    pub fn new() -> Self {
        Self
    }

    /// Render status bar from AppState
    /// This is a pure function that only handles rendering
    pub fn draw(&self, state: &AppState, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let layout = Layout::new(
            Direction::Vertical,
            [
                Constraint::Min(0),
                Constraint::Length(1), // User info line
                Constraint::Length(1), // Status message line
            ],
        )
        .split(area);

        f.render_widget(Clear, layout[1]);
        f.render_widget(Clear, layout[2]);

        // Render user info line
        let user_name = self.get_display_name(state);
        let name_span = Span::styled(user_name, Style::default().fg(Color::Gray).italic());
        let status_line = Paragraph::new(name_span).style(Style::default().bg(Color::Black));
        f.render_widget(status_line, layout[1]);

        // Render status message line
        let message_line = if state.system.is_loading {
            Paragraph::new("Loading...")
        } else {
            let message = state
                .system
                .status_message
                .as_ref()
                .cloned()
                .unwrap_or_default();
            Paragraph::new(message)
        };
        f.render_widget(message_line, layout[2]);

        Ok(())
    }

    /// Get display name from current user state
    /// Pure function that computes display name based on available profile data
    pub fn get_display_name(&self, state: &AppState) -> String {
        let current_pubkey = state.user.current_user_pubkey;

        // Try to get profile for current user
        if let Some(profile) = state.user.profiles.get(&current_pubkey) {
            profile.name()
        } else {
            // Fallback to shortened public key
            PublicKey::new(current_pubkey).shortened()
        }
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper functions for status bar operations
impl StatusBar {
    /// Get connection status from app state
    pub fn get_connection_status(state: &AppState) -> String {
        // This could be extended to show network connection status
        // For now, we derive status from loading state and message presence
        if state.system.is_loading {
            "Connecting...".to_string()
        } else if state.system.status_message.is_some() {
            "Active".to_string()
        } else {
            "Ready".to_string()
        }
    }

    /// Check if current user has profile data
    pub fn has_profile_data(state: &AppState) -> bool {
        state
            .user
            .profiles
            .contains_key(&state.user.current_user_pubkey)
    }

    /// Get profile creation timestamp if available
    pub fn get_profile_timestamp(state: &AppState) -> Option<nostr_sdk::Timestamp> {
        state
            .user
            .profiles
            .get(&state.user.current_user_pubkey)
            .map(|profile| profile.created_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::*;

    fn create_test_state_with_profile(has_profile: bool) -> AppState {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        if has_profile {
            let metadata = Metadata::new()
                .name("Test User")
                .display_name("Test Display");
            let profile =
                crate::domain::nostr::Profile::new(keys.public_key(), Timestamp::now(), metadata);
            state.user.profiles.insert(keys.public_key(), profile);
        }

        state
    }

    #[test]
    fn test_status_bar_creation() {
        let status_bar = StatusBar::new();
        let default_status_bar = StatusBar;

        // Both should be equivalent (stateless)
        assert_eq!(
            format!("{:?}", status_bar),
            format!("{:?}", default_status_bar)
        );
    }

    #[test]
    fn test_status_bar_is_stateless() {
        let status1 = StatusBar::new();
        let status2 = StatusBar::new();

        // Since it's stateless, all instances should be equivalent
        assert_eq!(format!("{:?}", status1), format!("{:?}", status2));
    }

    #[test]
    fn test_display_name_with_profile() {
        let state = create_test_state_with_profile(true);
        let status_bar = StatusBar::new();

        let display_name = status_bar.get_display_name(&state);
        assert_eq!(display_name, "Test Display");
    }

    #[test]
    fn test_display_name_without_profile() {
        let state = create_test_state_with_profile(false);
        let status_bar = StatusBar::new();

        let display_name = status_bar.get_display_name(&state);
        // Should fall back to shortened public key
        assert!(!display_name.is_empty());
        assert!(display_name.contains(":")); // Hex shortened format
    }

    #[test]
    fn test_connection_status() {
        let mut state = create_test_state_with_profile(false);

        // Test loading state
        state.system.is_loading = true;
        let status = StatusBar::get_connection_status(&state);
        assert_eq!(status, "Connecting...");

        // Test active state with message
        state.system.is_loading = false;
        state.system.status_message = Some("Connected to relay".to_string());
        let status = StatusBar::get_connection_status(&state);
        assert_eq!(status, "Active");

        // Test ready state
        state.system.status_message = None;
        let status = StatusBar::get_connection_status(&state);
        assert_eq!(status, "Ready");
    }

    #[test]
    fn test_has_profile_data() {
        let state_with_profile = create_test_state_with_profile(true);
        let state_without_profile = create_test_state_with_profile(false);

        assert!(StatusBar::has_profile_data(&state_with_profile));
        assert!(!StatusBar::has_profile_data(&state_without_profile));
    }

    #[test]
    fn test_get_profile_timestamp() {
        let state_with_profile = create_test_state_with_profile(true);
        let state_without_profile = create_test_state_with_profile(false);

        let timestamp = StatusBar::get_profile_timestamp(&state_with_profile);
        assert!(timestamp.is_some());

        let timestamp = StatusBar::get_profile_timestamp(&state_without_profile);
        assert!(timestamp.is_none());
    }

    #[test]
    fn test_multiple_profiles() {
        let keys1 = Keys::generate();
        let keys2 = Keys::generate();
        let mut state = AppState::new(keys1.public_key());

        // Add profile for current user
        let metadata1 = Metadata::new().name("Current User");
        let profile1 =
            crate::domain::nostr::Profile::new(keys1.public_key(), Timestamp::now(), metadata1);
        state.user.profiles.insert(keys1.public_key(), profile1);

        // Add profile for another user
        let metadata2 = Metadata::new().name("Other User");
        let profile2 =
            crate::domain::nostr::Profile::new(keys2.public_key(), Timestamp::now(), metadata2);
        state.user.profiles.insert(keys2.public_key(), profile2);

        let status_bar = StatusBar::new();
        let display_name = status_bar.get_display_name(&state);

        // Should show current user's name, not other user's
        assert_eq!(display_name, "@Current User");
    }
}
