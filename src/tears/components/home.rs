//! Home component
//!
//! Main view component that displays the timeline and handles user input.

use ratatui::prelude::*;

use crate::core::state::AppState;

pub mod input;
pub mod list;

pub use input::HomeInputComponent;
pub use list::HomeListComponent;

/// Home component
///
/// This is the main view that contains the timeline list and input area.
/// It delegates to child components for rendering.
#[derive(Debug)]
pub struct HomeComponent<'a> {
    /// Input area component
    input: HomeInputComponent<'a>,
    /// Timeline list component
    list: HomeListComponent,
}

impl<'a> HomeComponent<'a> {
    /// Create a new home component
    pub fn new() -> Self {
        Self {
            input: HomeInputComponent::new(),
            list: HomeListComponent::new(),
        }
    }

    /// Render the home view
    ///
    /// This renders the timeline list and optionally the input area.
    pub fn view(&mut self, state: &AppState, frame: &mut Frame, area: Rect) {
        // Render timeline first (always full area for scrolling continuity)
        self.list.view(state, frame, area);

        // Render input area as overlay if composing (matching old architecture)
        if state.ui.is_composing() {
            // Calculate overlay input area exactly like original implementation
            // Take bottom half of the screen, minus 2 lines for margin
            let mut input_area = area;
            input_area.height /= 2;
            input_area.y = input_area.height;
            input_area.height = input_area.height.saturating_sub(2);

            self.input.view(state, frame, input_area);
        }
    }
}

impl<'a> Default for HomeComponent<'a> {
    fn default() -> Self {
        Self::new()
    }
}
