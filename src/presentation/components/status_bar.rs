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
        let message = state.system.status_message().cloned().unwrap_or_default();
        let message_line = Paragraph::new(message);
        frame.render_widget(message_line, layout[2]);
    }

    /// Get display name from current user state
    ///
    /// Pure function that computes display name based on available profile data.
    pub fn get_display_name(&self, state: &AppState) -> String {
        // Try to get profile for current user
        if let Some(profile) = state.user.current_user() {
            profile.name()
        } else {
            // Fallback to shortened public key
            PublicKey::new(state.user.current_user_pubkey()).shortened()
        }
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
            state.user.insert_newer_profile(profile);
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
    fn test_multiple_profiles() {
        let keys1 = Keys::generate();
        let keys2 = Keys::generate();
        let mut state = AppState::new(keys1.public_key());

        // Add profile for current user
        let metadata1 = Metadata::new().name("Current User");
        let profile1 = Profile::new(keys1.public_key(), Timestamp::now(), metadata1);
        state.user.insert_newer_profile(profile1);

        // Add profile for another user
        let metadata2 = Metadata::new().name("Other User");
        let profile2 = Profile::new(keys2.public_key(), Timestamp::now(), metadata2);
        state.user.insert_newer_profile(profile2);

        let status_bar = StatusBarComponent::new();
        let display_name = status_bar.get_display_name(&state);

        // Should show current user's name, not other user's
        assert_eq!(display_name, "@Current User");
    }
}
