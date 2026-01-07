//! Component collection and management
//!
//! This module defines the component structure for the hybrid Tears pattern.
//! Components are stateless renderers that receive state as parameters.

use ratatui::prelude::*;

use crate::core::state::AppState;

pub mod fps;
pub mod home;
pub mod status_bar;

pub use fps::FpsComponent;
pub use home::HomeComponent;
pub use status_bar::StatusBarComponent;

/// Collection of all components
///
/// This struct holds instances of all components used in the application.
/// Components are stateless and receive state as parameters during render.
pub struct Components<'a> {
    pub home: HomeComponent<'a>,
    pub fps: FpsComponent,
    pub status_bar: StatusBarComponent,
}

impl<'a> Components<'a> {
    /// Create a new component collection
    pub fn new() -> Self {
        Self {
            home: HomeComponent::new(),
            fps: FpsComponent::new(),
            status_bar: StatusBarComponent::new(),
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
        self.status_bar.view(state, frame, layout[2]);
    }
}

impl<'a> Default for Components<'a> {
    fn default() -> Self {
        Self::new()
    }
}
