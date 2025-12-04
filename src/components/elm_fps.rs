use color_eyre::eyre::Result;
use ratatui::{prelude::*, widgets::*};

use crate::{state::AppState, tui::Frame};

/// Elm-architecture compatible FPS counter component
/// This component is purely functional - it receives state and renders FPS data
/// No internal state management or time calculations
#[derive(Debug, Clone)]
pub struct ElmFpsCounter;

impl ElmFpsCounter {
    pub fn new() -> Self {
        Self
    }

    /// Render FPS data from AppState
    /// This is a pure function that only handles rendering
    pub fn draw(&self, state: &AppState, f: &mut Frame<'_>, rect: Rect) -> Result<()> {
        let rects = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Length(1), // first row
                Constraint::Min(0),
            ])
            .split(rect);

        let rect = rects[0];

        let fps_data = &state.system.fps_data;
        let s = format!(
            "{:.2} ticks per sec (app) {:.2} frames per sec (render)",
            fps_data.app_fps, fps_data.render_fps
        );

        let block = Block::default().title_top(Line::from(s.dim()).right_aligned());
        f.render_widget(block, rect);

        Ok(())
    }
}

impl Default for ElmFpsCounter {
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
    fn test_elm_fps_counter_creation() {
        let fps_counter = ElmFpsCounter::new();
        let default_fps_counter = ElmFpsCounter;

        // Both should be equivalent (stateless)
        assert_eq!(
            format!("{:?}", fps_counter),
            format!("{:?}", default_fps_counter)
        );
    }

    #[test]
    fn test_fps_counter_is_stateless() {
        let fps1 = ElmFpsCounter::new();
        let fps2 = ElmFpsCounter::new();

        // Since it's stateless, all instances should be equivalent
        assert_eq!(format!("{:?}", fps1), format!("{:?}", fps2));
    }

    // Note: Drawing tests would require a mock terminal backend
    // For now, we focus on the stateless nature of the component
    #[test]
    fn test_fps_data_access() {
        let state = create_test_state_with_fps(60.0, 120.0);

        assert_eq!(state.system.fps_data.app_fps, 60.0);
        assert_eq!(state.system.fps_data.render_fps, 120.0);
    }
}
