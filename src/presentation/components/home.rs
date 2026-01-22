//! Home component
//!
//! Main view component that displays the timeline and handles user input.

use ratatui::prelude::*;

use crate::{
    core::state::AppState,
    presentation::widgets::tab_bar::{TabBarWidget, ViewContext as TabBarViewContext},
};

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
    pub(crate) input: HomeInputComponent<'a>,
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
    /// This renders the tabs, timeline list, and optionally the input area.
    pub fn view(&mut self, state: &AppState, frame: &mut Frame, area: Rect) {
        // Split the area into tabs and content
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Tab bar
                Constraint::Min(0),    // Content area
            ])
            .split(area);

        // Render tabs using the tab bar component
        let tab_bar_ctx = TabBarViewContext {
            profiles: state.user.profiles(),
        };
        let tab_bar = TabBarWidget::new(&state.timeline, tab_bar_ctx);
        frame.render_widget(tab_bar, chunks[0]);

        // Render timeline in the content area
        self.list.view(state, frame, chunks[1]);

        // Render input area as overlay if composing (matching old architecture)
        if state.editor.is_composing() {
            // Calculate overlay input area (take bottom half of the screen)
            let mut input_area = chunks[1];
            input_area.height /= 2;
            input_area.y += input_area.height;

            self.input.view(state, frame, input_area);
        }
    }
}

impl<'a> Default for HomeComponent<'a> {
    fn default() -> Self {
        Self::new()
    }
}
