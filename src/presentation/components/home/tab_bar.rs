//! Tab bar component for displaying timeline tabs

use nostr_sdk::ToBech32;
use ratatui::prelude::*;

use crate::core::state::timeline::TimelineTabType;
use crate::core::state::AppState;
use crate::domain::text::shorten_npub;

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
            .map(|tab| match &tab.tab_type {
                TimelineTabType::Home => "Home".to_string(),
                TimelineTabType::UserTimeline { pubkey } => {
                    // Try to get the handle from profile, fallback to shortened npub
                    state
                        .user
                        .get_profile(pubkey)
                        .and_then(|profile| {
                            // Use handle if available (already includes @)
                            profile.handle()
                        })
                        .unwrap_or_else(|| {
                            // Fallback to shortened npub
                            let Ok(npub) = pubkey.to_bech32();
                            shorten_npub(npub)
                        })
                }
            })
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
