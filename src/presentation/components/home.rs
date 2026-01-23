//! Home component
//!
//! Main view component that displays the timeline and handles user input.

use ratatui::{prelude::*, widgets::Clear};

use crate::{
    core::state::AppState,
    presentation::widgets::{
        editor::EditorWidget,
        tab_bar::{TabBarWidget, ViewContext as TabBarViewContext},
    },
};

pub mod list;

pub use list::HomeListComponent;

/// Home component
///
/// This is the main view that contains the timeline list and input area.
/// It delegates to child components for rendering.
#[derive(Debug)]
pub struct HomeComponent {
    /// Timeline list component
    list: HomeListComponent,
}

impl HomeComponent {
    /// Create a new home component
    pub fn new() -> Self {
        Self {
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
        if state.editor.is_active() {
            // Calculate overlay input area (take bottom half of the screen)
            let mut input_area = chunks[1];
            input_area.height /= 2;
            input_area.y += input_area.height;

            let widget = EditorWidget::new(&state.editor);
            frame.render_widget(Clear, input_area);
            frame.render_widget(widget, input_area);
        }
    }
}

impl Default for HomeComponent {
    fn default() -> Self {
        Self::new()
    }
}
