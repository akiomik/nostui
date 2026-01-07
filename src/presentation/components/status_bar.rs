//! Status bar component
//!
//! Displays status information at the bottom of the screen.
//! This is a pure, stateless component that renders status data from AppState.

use ratatui::{prelude::*, widgets::*};

use crate::{core::state::AppState, presentation::widgets::public_key::PublicKey};

/// Status bar component
///
/// Displays user information and system status messages at the bottom of the screen.
/// It's a stateless component following the Elm architecture pattern.
#[derive(Debug, Clone)]
pub struct StatusBarComponent;

impl StatusBarComponent {
    /// Create a new status bar component
    pub fn new() -> Self {
        Self
    }

    /// Render the status bar
    ///
    /// This renders two lines:
    /// 1. User info line (current user's display name)
    /// 2. Status message line (loading status or custom messages)
    pub fn view(&self, state: &AppState, frame: &mut Frame, area: Rect) {
        let layout = Layout::new(
            Direction::Vertical,
            [
                Constraint::Min(0),    // Main content area (not used by status bar)
                Constraint::Length(1), // User info line
                Constraint::Length(1), // Status message line
            ],
        )
        .split(area);

        // Clear the status bar area
        frame.render_widget(Clear, layout[1]);
        frame.render_widget(Clear, layout[2]);

        // Render user info line
        let user_name = self.get_display_name(state);
        let name_span = Span::styled(user_name, Style::default().fg(Color::Gray).italic());
        let status_line = Paragraph::new(name_span).style(Style::default().bg(Color::Black));
        frame.render_widget(status_line, layout[1]);

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
        frame.render_widget(message_line, layout[2]);
    }

    /// Get display name from current user state
    ///
    /// Pure function that computes display name based on available profile data.
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

impl Default for StatusBarComponent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::nostr::Profile;
    use nostr_sdk::prelude::*;

    fn create_test_state_with_profile(has_profile: bool) -> AppState {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        if has_profile {
            let metadata = Metadata::new()
                .name("Test User")
                .display_name("Test Display");
            let profile = Profile::new(keys.public_key(), Timestamp::now(), metadata);
            state.user.profiles.insert(keys.public_key(), profile);
        }

        state
    }

    #[test]
    fn test_status_bar_creation() {
        let status_bar = StatusBarComponent::new();
        let default_status_bar = StatusBarComponent;

        // Both should be equivalent (stateless)
        assert_eq!(format!("{status_bar:?}"), format!("{default_status_bar:?}"));
    }

    #[test]
    fn test_status_bar_is_stateless() {
        let status1 = StatusBarComponent::new();
        let status2 = StatusBarComponent::new();

        // Since it's stateless, all instances should be equivalent
        assert_eq!(format!("{status1:?}"), format!("{status2:?}"));
    }

    #[test]
    fn test_display_name_with_profile() {
        let state = create_test_state_with_profile(true);
        let status_bar = StatusBarComponent::new();

        let display_name = status_bar.get_display_name(&state);
        assert_eq!(display_name, "Test Display");
    }

    #[test]
    fn test_display_name_without_profile() {
        let state = create_test_state_with_profile(false);
        let status_bar = StatusBarComponent::new();

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
        let status = StatusBarComponent::get_connection_status(&state);
        assert_eq!(status, "Connecting...");

        // Test active state with message
        state.system.is_loading = false;
        state.system.status_message = Some("Connected to relay".to_string());
        let status = StatusBarComponent::get_connection_status(&state);
        assert_eq!(status, "Active");

        // Test ready state
        state.system.status_message = None;
        let status = StatusBarComponent::get_connection_status(&state);
        assert_eq!(status, "Ready");
    }

    #[test]
    fn test_has_profile_data() {
        let state_with_profile = create_test_state_with_profile(true);
        let state_without_profile = create_test_state_with_profile(false);

        assert!(StatusBarComponent::has_profile_data(&state_with_profile));
        assert!(!StatusBarComponent::has_profile_data(
            &state_without_profile
        ));
    }

    #[test]
    fn test_get_profile_timestamp() {
        let state_with_profile = create_test_state_with_profile(true);
        let state_without_profile = create_test_state_with_profile(false);

        let timestamp = StatusBarComponent::get_profile_timestamp(&state_with_profile);
        assert!(timestamp.is_some());

        let timestamp = StatusBarComponent::get_profile_timestamp(&state_without_profile);
        assert!(timestamp.is_none());
    }

    #[test]
    fn test_multiple_profiles() {
        let keys1 = Keys::generate();
        let keys2 = Keys::generate();
        let mut state = AppState::new(keys1.public_key());

        // Add profile for current user
        let metadata1 = Metadata::new().name("Current User");
        let profile1 = Profile::new(keys1.public_key(), Timestamp::now(), metadata1);
        state.user.profiles.insert(keys1.public_key(), profile1);

        // Add profile for another user
        let metadata2 = Metadata::new().name("Other User");
        let profile2 = Profile::new(keys2.public_key(), Timestamp::now(), metadata2);
        state.user.profiles.insert(keys2.public_key(), profile2);

        let status_bar = StatusBarComponent::new();
        let display_name = status_bar.get_display_name(&state);

        // Should show current user's name, not other user's
        assert_eq!(display_name, "@Current User");
    }
}
