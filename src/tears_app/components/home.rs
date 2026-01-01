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
        if state.ui.is_composing() {
            // When composing, split area for list and input
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![
                    Constraint::Min(0),    // List area (flexible)
                    Constraint::Length(5), // Input area (fixed 5 lines)
                ])
                .split(area);

            self.list.view(state, frame, layout[0]);
            self.input.view(state, frame, layout[1]);
        } else {
            // When not composing, use full area for list
            self.list.view(state, frame, area);
        }
    }
}

impl<'a> Default for HomeComponent<'a> {
    fn default() -> Self {
        Self::new()
    }
}
