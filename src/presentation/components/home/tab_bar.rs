//! Tab bar component for displaying timeline tabs

use ratatui::prelude::*;

use crate::core::state::AppState;

/// Tab bar component
///
/// Displays the timeline tabs at the top of the home view
#[derive(Debug)]
pub struct TabBarComponent;

impl TabBarComponent {
    /// Create a new tab bar component
    pub fn new() -> Self {
        Self
    }

    /// Render the tab bar
    pub fn view(&self, state: &AppState, frame: &mut Frame, area: Rect) {
        let tab_titles: Vec<String> = state
            .timeline
            .tabs()
            .iter()
            .map(|tab| tab.tab_title(state.user.profiles()))
            .collect();

        let tabs = ratatui::widgets::Tabs::new(tab_titles)
            .select(state.timeline.active_tab_index())
            .style(Style::default().bg(Color::Black))
            .highlight_style(Style::default().reversed());

        frame.render_widget(tabs, area);
    }
}

impl Default for TabBarComponent {
    fn default() -> Self {
        Self::new()
    }
}
