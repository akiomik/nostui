//! Component collection and management
//!
//! This module defines the component structure for the hybrid Tears pattern.
//! Components are stateless renderers that receive state as parameters.

use ratatui::prelude::*;

use crate::{
    core::state::AppState,
    presentation::widgets::status_bar::{StatusBarWidget, ViewContext as StatusBarViewContext},
};

pub mod fps;
pub mod home;

pub use fps::FpsComponent;
pub use home::HomeComponent;

/// Collection of all components
///
/// This struct holds instances of all components used in the application.
/// Components are stateless and receive state as parameters during render.
pub struct Components<'a> {
    pub home: HomeComponent<'a>,
    pub fps: FpsComponent,
}

impl<'a> Components<'a> {
    /// Create a new component collection
    pub fn new() -> Self {
        Self {
            home: HomeComponent::new(),
            fps: FpsComponent::new(),
        }
    }

    /// Render all components
    ///
    /// This is the main rendering entry point that delegates to individual components.
    pub fn render(&mut self, frame: &mut Frame, state: &AppState) {
        let area = frame.area();

        // Create layout: [FPS row, main area, status bar (2 rows)]
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Length(1), // FPS counter
                Constraint::Min(0),    // Main area (home)
                Constraint::Length(2), // Status bar (2 rows)
            ])
            .split(area);

        // Render FPS counter in top row
        self.fps.view(state, frame, layout[0]);

        // Render home component in main area
        self.home.view(state, frame, layout[1]);

        // Render status bar at bottom
        let status_bar_ctx = StatusBarViewContext {
            user_pubkey: state.user.current_user_pubkey(),
            user_profile: state.user.current_user(),
        };
        let status_bar = StatusBarWidget::new(state.status_bar.clone(), status_bar_ctx);
        frame.render_widget(status_bar, layout[2]);
    }
}

impl<'a> Default for Components<'a> {
    fn default() -> Self {
        Self::new()
    }
}
