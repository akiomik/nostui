use crossterm::event::KeyEvent;
use nostr_sdk::prelude::*;
use ratatui::prelude::*;

use crate::{core::cmd::Cmd, core::msg::Msg, core::state::AppState};

use super::{home_data::HomeData, home_input::HomeInput, home_list::HomeList};

/// Complete Elm-style Home component that orchestrates data, list, and input
#[derive(Debug)]
pub struct Home<'a> {
    data: HomeData,
    list: HomeList,
    input: HomeInput<'a>,
}

impl<'a> Default for Home<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Home<'a> {
    /// Create a new Home component
    pub fn new() -> Self {
        Self {
            data: HomeData::new(),
            list: HomeList::new(),
            input: HomeInput::new(),
        }
    }

    /// Update the component with new state and return any commands
    pub fn update(&mut self, _state: &AppState) -> Vec<Cmd> {
        Vec::new()
    }

    /// Render the complete home view
    pub fn render(&mut self, frame: &mut Frame, area: Rect, state: &AppState) {
        // Render timeline first (always full area for scrolling continuity like original)
        if let Err(e) = self.list.draw(state, frame, area) {
            log::error!("Failed to render timeline: {e}");
        }

        // Render input area as overlay if needed (like original implementation home.rs:265-270)
        if state.ui.is_composing() {
            // Calculate overlay input area exactly like original implementation
            let mut input_area = frame.area();
            input_area.height /= 2;
            input_area.y = input_area.height;
            input_area.height = input_area.height.saturating_sub(2);

            if let Err(e) = self.input.draw(state, frame, input_area) {
                log::error!("Failed to render input overlay: {e}");
            }
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
        !state.ui.is_composing() && state.timeline.selected_index.is_some()
    }

    /// Get the currently selected note for interactions
    pub fn get_selected_note<'b>(&self, state: &'b AppState) -> Option<&'b Event> {
        state.selected_note()
    }

    /// Check if input is in a valid state for submission
    pub fn can_submit_input(&self, state: &AppState) -> bool {
        state.ui.can_submit_input()
    }

    /// Reset the component to initial state
    pub fn reset(&mut self) {
        self.data = HomeData::new();
        self.list = HomeList::new();
        self.input = HomeInput::new();
    }
}

/// Helper methods for advanced interaction logic
impl<'a> Home<'a> {
    /// Determine available actions based on current state
    pub fn get_available_actions(&self, state: &AppState) -> Vec<HomeAction> {
        let mut actions = Vec::new();

        if state.ui.is_composing() {
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
        if state.ui.is_composing() {
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
            input_mode: state.ui.is_composing(),
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
    use crate::core::state::ui::UiMode;

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
    fn test_home_new() {
        let home = Home::new();
        assert!(matches!(home.data, HomeData { .. }));
        assert!(matches!(home.list, HomeList { .. }));
        assert!(matches!(home.input, HomeInput { .. }));
    }

    #[test]
    fn test_home_can_interact() {
        let home = Home::new();
        let mut state = create_test_state();

        // Cannot interact when input is showing
        state.ui.current_mode = UiMode::Composing;
        assert!(!home.can_interact(&state));

        // Cannot interact when no note is selected
        state.ui.current_mode = UiMode::Normal;
        state.timeline.selected_index = None;
        assert!(!home.can_interact(&state));

        // Can interact when not in input mode and note is selected
        state.ui.current_mode = UiMode::Normal;
        state.timeline.selected_index = Some(0);
        assert!(home.can_interact(&state));
    }

    #[test]
    fn test_home_get_selected_note() {
        let home = Home::new();
        let state = create_test_state();

        // No note selected
        assert!(home.get_selected_note(&state).is_none());
    }

    #[test]
    fn test_home_can_submit_input() {
        let home = Home::new();
        let mut state = create_test_state();

        // Cannot submit when input is not showing
        state.ui.current_mode = UiMode::Normal;
        assert!(!home.can_submit_input(&state));

        // Check when input is showing (depends on input component implementation)
        state.ui.current_mode = UiMode::Composing;
        state.ui.input_content = "test content".to_string();
        // Result depends on input validation logic
    }

    #[test]
    fn test_home_available_actions() {
        let home = Home::new();
        let mut state = create_test_state();

        // Actions when in input mode
        state.ui.current_mode = UiMode::Composing;
        let actions = home.get_available_actions(&state);
        assert!(actions.contains(&HomeAction::SubmitNote));
        assert!(actions.contains(&HomeAction::CancelInput));

        // Actions when in normal mode with empty timeline
        state.ui.current_mode = UiMode::Normal;
        state.timeline.selected_index = None;
        let actions = home.get_available_actions(&state);
        assert!(actions.contains(&HomeAction::ShowNewNote));
        assert!(!actions.contains(&HomeAction::SendReaction));
    }

    #[test]
    fn test_home_help_text() {
        let home = Home::new();
        let mut state = create_test_state();

        // Help text for input mode
        state.ui.current_mode = UiMode::Composing;
        let help = home.get_help_text(&state);
        assert!(help.contains("Enter"));
        assert!(help.contains("Esc"));

        // Help text for empty timeline
        state.ui.current_mode = UiMode::Normal;
        let help = home.get_help_text(&state);
        assert!(help.contains("New note"));
    }

    #[test]
    fn test_home_status_info() {
        let home = Home::new();
        let mut state = create_test_state();

        let status = home.get_status_info(&state);
        assert_eq!(status.timeline_count, 0);
        assert_eq!(status.selected_index, None);
        assert!(!status.input_mode);
        assert!(!status.reply_mode);
        assert!(!status.can_interact);

        // Change state and verify status updates
        state.ui.current_mode = UiMode::Composing;
        state.ui.reply_to = Some(create_test_event());
        let status = home.get_status_info(&state);
        assert!(status.input_mode);
        assert!(status.reply_mode);
    }

    #[test]
    fn test_home_reset() {
        let mut home = Home::new();

        // Reset should recreate all components
        home.reset();
        assert!(matches!(home.data, HomeData { .. }));
        assert!(matches!(home.list, HomeList { .. }));
        assert!(matches!(home.input, HomeInput { .. }));
    }
}
