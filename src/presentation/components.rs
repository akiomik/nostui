//! Component collection and management
//!
//! This module defines the component structure for the hybrid Tears pattern.
//! Components are stateless renderers that receive state as parameters.

use ratatui::prelude::*;

use crate::{
    application::state::AppState,
    presentation::widgets::status_bar::{StatusBarWidget, ViewContext as StatusBarViewContext},
};

pub mod home;

pub use home::HomeComponent;

/// Collection of all components
///
/// This struct holds instances of all components used in the application.
/// Components are stateless and receive state as parameters during render.
pub struct Components {
    pub home: HomeComponent,
}

impl Components {
    /// Create a new component collection
    pub fn new() -> Self {
        Self {
            home: HomeComponent::new(),
        }
    }

    /// Render all components
    ///
    /// This is the main rendering entry point that delegates to individual components.
    pub fn render(&mut self, frame: &mut Frame, state: &AppState) {
        let area = frame.area();

        // Create layout: [main area, status bar (2 rows)]
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Min(0),    // Main area (home)
                Constraint::Length(2), // Status bar (2 rows)
            ])
            .split(area);

        // Render home component in main area
        self.home.view(state, frame, layout[0]);

        // Render status bar at bottom
        let status_bar_ctx = StatusBarViewContext {
            user_pubkey: state.user.current_user_pubkey(),
            user_profile: state.user.current_user(),
        };
        let status_bar = StatusBarWidget::new(state.status_bar.clone(), status_bar_ctx);
        frame.render_widget(status_bar, layout[1]);
    }
}

impl Default for Components {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::model::status_bar::Message as StatusBarMessage;

    use super::*;

    /// #527 removed the FPS counter, and with it the one-line row it was given at
    /// the top of the screen. That line belongs to the timeline now, so the home
    /// component starts at row 0 and the status bar still ends at the bottom.
    ///
    /// Both halves matter: dropping the row re-indexed the layout under the status
    /// bar as well, so a test that only looked at the top would not notice the
    /// bottom pane moving.
    #[test]
    fn test_home_component_starts_at_the_top_row() {
        let mut terminal = Terminal::new(TestBackend::new(40, 8)).expect("test terminal");
        let mut components = Components::new();
        let mut state = AppState::default();
        state.status_bar.update(StatusBarMessage::MessageChanged {
            label: "Probe".to_owned(),
            message: "probe message".to_owned(),
        });

        terminal
            .draw(|frame| components.render(frame, &state))
            .expect("draw should not fail");

        let buffer = terminal.backend().buffer().clone();
        let row = |y: u16| -> String { (0..40).map(|x| buffer[(x, y)].symbol()).collect() };

        assert!(
            row(0).contains("Home"),
            "expected the tab bar on the top row, got: {}",
            row(0)
        );
        assert!(
            row(2).contains("No notes to display"),
            "expected the timeline below it, got: {}",
            row(2)
        );
        assert!(
            row(7).contains("[Probe] probe message"),
            "expected the status bar on the bottom row, got: {}",
            row(7)
        );
    }
}
