//! FPS counter component
//!
//! A simple component that displays the current frames per second.
//! This is a pure, stateless component that renders FPS data from AppState.

use ratatui::{prelude::*, widgets::*};

use crate::core::state::AppState;

/// FPS counter component
///
/// This component displays the current FPS values (app and render) from the system state.
/// It's a stateless component following the Elm architecture pattern.
#[derive(Debug, Clone)]
pub struct FpsComponent;

impl FpsComponent {
    /// Create a new FPS component
    pub fn new() -> Self {
        Self
    }

    /// Render the FPS counter
    ///
    /// This component reads FPS values from system state and renders them
    /// as a right-aligned title in the top row of the given area.
    pub fn view(&self, state: &AppState, frame: &mut Frame, area: Rect) {
        // Split the area to get the top row for FPS display
        let rects = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Length(1), // First row for FPS
                Constraint::Min(0),    // Rest of the area
            ])
            .split(area);

        let fps_rect = rects[0];

        // Get FPS data from system state
        let fps_data = &state.system.fps_data;
        let fps_text = format!(
            "{:.2} ticks per sec (app) {:.2} frames per sec (render)",
            fps_data.app_fps, fps_data.render_fps
        );

        // Render as a dimmed, right-aligned title
        let block = Block::default().title_top(Line::from(fps_text.dim()).right_aligned());
        frame.render_widget(block, fps_rect);
    }
}

impl Default for FpsComponent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::*;

    fn create_test_state_with_fps(app_fps: f64, render_fps: f64) -> AppState {
        let mut state = AppState::new(Keys::generate().public_key());
        state.system.fps_data.app_fps = app_fps;
        state.system.fps_data.render_fps = render_fps;
        state
    }

    #[test]
    fn test_fps_component_creation() {
        let fps_component = FpsComponent::new();
        let default_fps_component = FpsComponent;

        // Both should be equivalent (stateless)
        assert_eq!(
            format!("{fps_component:?}"),
            format!("{default_fps_component:?}")
        );
    }

    #[test]
    fn test_fps_component_is_stateless() {
        let fps1 = FpsComponent::new();
        let fps2 = FpsComponent::new();

        // Since it's stateless, all instances should be equivalent
        assert_eq!(format!("{fps1:?}"), format!("{fps2:?}"));
    }

    #[test]
    fn test_fps_data_access() {
        let state = create_test_state_with_fps(60.0, 120.0);

        assert_eq!(state.system.fps_data.app_fps, 60.0);
        assert_eq!(state.system.fps_data.render_fps, 120.0);
    }
}
