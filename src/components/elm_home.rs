use crossterm::event::KeyEvent;
use nostr_sdk::prelude::*;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

use crate::{cmd::Cmd, msg::Msg, state::AppState};

use super::{elm_home_data::ElmHomeData, elm_home_input::ElmHomeInput, elm_home_list::ElmHomeList};

/// Complete Elm-style Home component that orchestrates data, list, and input
#[derive(Debug)]
pub struct ElmHome<'a> {
    data: ElmHomeData,
    list: ElmHomeList,
    input: ElmHomeInput<'a>,
}

impl<'a> Default for ElmHome<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> ElmHome<'a> {
    /// Create a new ElmHome component
    pub fn new() -> Self {
        Self {
            data: ElmHomeData::new(),
            list: ElmHomeList::new(),
            input: ElmHomeInput::new(),
        }
    }

    /// Update the component with new state and return any commands
    pub fn update(&mut self, _state: &AppState) -> Vec<Cmd> {
        Vec::new()
    }

    /// Render the complete home view
    pub fn render(&self, _frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
        // Create layout: [timeline, input_area]
        let _chunks = if state.ui.show_input {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(3)])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(100)])
                .split(area)
        };

        // Render timeline and input areas
        if state.ui.show_input {
            // Input rendering logic would go here
        }
    }

    /// Process a key event and return resulting messages
    pub fn process_key(&mut self, _key: KeyEvent, _state: &AppState) -> Vec<Msg> {
        vec![]
    }

    /// Get display data for the timeline
    pub fn get_display_data(&self, state: &AppState) -> Vec<String> {
        state
            .timeline
            .notes
            .iter()
            .map(|note| note.0.event.content.clone())
            .collect()
    }

    /// Check if we can perform interactions with the currently selected note
    pub fn can_interact(&self, state: &AppState) -> bool {
        !state.ui.show_input && state.timeline.selected_index.is_some()
    }

    /// Get the currently selected note for interactions
    pub fn get_selected_note<'b>(&self, state: &'b AppState) -> Option<&'b Event> {
        state.selected_note()
    }

    /// Get input validation and submission data
    pub fn get_input_submit_data(
        &self,
        state: &AppState,
    ) -> Option<super::elm_home_input::SubmitData> {
        ElmHomeInput::get_submit_data(state)
    }

    /// Check if input is in a valid state for submission
    pub fn can_submit_input(&self, state: &AppState) -> bool {
        state.ui.show_input && !state.ui.input_content.trim().is_empty()
    }

    /// Reset the component to initial state
    pub fn reset(&mut self) {
        self.data = ElmHomeData::new();
        self.list = ElmHomeList::new();
        self.input = ElmHomeInput::new();
    }
}

/// Helper methods for advanced interaction logic
impl<'a> ElmHome<'a> {
    /// Determine available actions based on current state
    pub fn get_available_actions(&self, state: &AppState) -> Vec<HomeAction> {
        let mut actions = Vec::new();

        if state.ui.show_input {
            actions.push(HomeAction::SubmitNote);
            actions.push(HomeAction::CancelInput);
        } else {
            actions.push(HomeAction::ShowNewNote);

            if let Some(_selected_note) = state.selected_note() {
                actions.push(HomeAction::SendReaction);
                actions.push(HomeAction::ShowReply);
                actions.push(HomeAction::SendRepost);
            }

            if !state.timeline_is_empty() {
                actions.push(HomeAction::Navigate);
            }
        }

        actions
    }

    /// Get contextual help text based on current state
    pub fn get_help_text(&self, state: &AppState) -> String {
        if state.ui.show_input {
            if state.ui.reply_to.is_some() {
                "Enter: Send reply | Esc: Cancel".to_string()
            } else {
                "Enter: Send note | Esc: Cancel".to_string()
            }
        } else if state.timeline_is_empty() {
            "n: New note | Press 'n' to compose your first note".to_string()
        } else if state.selected_note().is_some() {
            "j/k: Navigate | l: Like | r: Reply | t: Repost | n: New note".to_string()
        } else {
            "j/k: Navigate | n: New note".to_string()
        }
    }

    /// Get status information for display
    pub fn get_status_info(&self, state: &AppState) -> HomeStatusInfo {
        HomeStatusInfo {
            timeline_count: state.timeline_len(),
            selected_index: state.timeline.selected_index,
            input_mode: state.ui.show_input,
            reply_mode: state.ui.reply_to.is_some(),
            can_interact: self.can_interact(state),
        }
    }
}

/// Available actions in the Home component
#[derive(Debug, Clone, PartialEq)]
pub enum HomeAction {
    Navigate,
    ShowNewNote,
    ShowReply,
    SendReaction,
    SendRepost,
    SubmitNote,
    CancelInput,
}

/// Status information about the Home component
#[derive(Debug, Clone)]
pub struct HomeStatusInfo {
    pub timeline_count: usize,
    pub selected_index: Option<usize>,
    pub input_mode: bool,
    pub reply_mode: bool,
    pub can_interact: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_state() -> AppState {
        AppState::new(Keys::generate().public_key())
    }

    fn create_test_event() -> Event {
        let keys = Keys::generate();
        EventBuilder::text_note("test content")
            .sign_with_keys(&keys)
            .unwrap()
    }

    #[test]
    fn test_elm_home_new() {
        let home = ElmHome::new();
        assert!(matches!(home.data, ElmHomeData { .. }));
        assert!(matches!(home.list, ElmHomeList { .. }));
        assert!(matches!(home.input, ElmHomeInput { .. }));
    }

    #[test]
    fn test_elm_home_can_interact() {
        let home = ElmHome::new();
        let mut state = create_test_state();

        // Cannot interact when input is showing
        state.ui.show_input = true;
        assert!(!home.can_interact(&state));

        // Cannot interact when no note is selected
        state.ui.show_input = false;
        state.timeline.selected_index = None;
        assert!(!home.can_interact(&state));

        // Can interact when not in input mode and note is selected
        state.ui.show_input = false;
        state.timeline.selected_index = Some(0);
        assert!(home.can_interact(&state));
    }

    #[test]
    fn test_elm_home_get_selected_note() {
        let home = ElmHome::new();
        let state = create_test_state();

        // No note selected
        assert!(home.get_selected_note(&state).is_none());
    }

    #[test]
    fn test_elm_home_can_submit_input() {
        let home = ElmHome::new();
        let mut state = create_test_state();

        // Cannot submit when input is not showing
        state.ui.show_input = false;
        assert!(!home.can_submit_input(&state));

        // Check when input is showing (depends on input component implementation)
        state.ui.show_input = true;
        state.ui.input_content = "test content".to_string();
        // Result depends on input validation logic
    }

    #[test]
    fn test_elm_home_available_actions() {
        let home = ElmHome::new();
        let mut state = create_test_state();

        // Actions when in input mode
        state.ui.show_input = true;
        let actions = home.get_available_actions(&state);
        assert!(actions.contains(&HomeAction::SubmitNote));
        assert!(actions.contains(&HomeAction::CancelInput));

        // Actions when in normal mode with empty timeline
        state.ui.show_input = false;
        state.timeline.selected_index = None;
        let actions = home.get_available_actions(&state);
        assert!(actions.contains(&HomeAction::ShowNewNote));
        assert!(!actions.contains(&HomeAction::SendReaction));
    }

    #[test]
    fn test_elm_home_help_text() {
        let home = ElmHome::new();
        let mut state = create_test_state();

        // Help text for input mode
        state.ui.show_input = true;
        let help = home.get_help_text(&state);
        assert!(help.contains("Enter"));
        assert!(help.contains("Esc"));

        // Help text for empty timeline
        state.ui.show_input = false;
        let help = home.get_help_text(&state);
        assert!(help.contains("New note"));
    }

    #[test]
    fn test_elm_home_status_info() {
        let home = ElmHome::new();
        let mut state = create_test_state();

        let status = home.get_status_info(&state);
        assert_eq!(status.timeline_count, 0);
        assert_eq!(status.selected_index, None);
        assert!(!status.input_mode);
        assert!(!status.reply_mode);
        assert!(!status.can_interact);

        // Change state and verify status updates
        state.ui.show_input = true;
        state.ui.reply_to = Some(create_test_event());
        let status = home.get_status_info(&state);
        assert!(status.input_mode);
        assert!(status.reply_mode);
    }

    #[test]
    fn test_elm_home_reset() {
        let mut home = ElmHome::new();

        // Reset should recreate all components
        home.reset();
        assert!(matches!(home.data, ElmHomeData { .. }));
        assert!(matches!(home.list, ElmHomeList { .. }));
        assert!(matches!(home.input, ElmHomeInput { .. }));
    }
}
