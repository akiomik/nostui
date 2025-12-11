use crate::core::msg::timeline::TimelineMsg;
use crate::core::msg::ui::UiMsg;
use color_eyre::eyre::Result;
use ratatui::{prelude::Rect, Frame};
use tokio::sync::mpsc;

use crate::{
    core::raw_msg::RawMsg, core::state::AppState, integration::elm_integration::ElmRuntime,
    integration::legacy::action::Action, integration::legacy::Component,
    presentation::components::elm_home::ElmHome,
};

/// Adapter that bridges the legacy Component trait with the new Elm architecture
/// This allows gradual migration from legacy Home component to ElmHome
pub struct ElmHomeAdapter {
    elm_home: ElmHome<'static>,
    elm_runtime: Option<ElmRuntime>,
    action_tx: Option<mpsc::UnboundedSender<Action>>,
}

impl ElmHomeAdapter {
    /// Create a new ElmHomeAdapter
    pub fn new() -> Self {
        Self {
            elm_home: ElmHome::new(),
            elm_runtime: None,
            action_tx: None,
        }
    }

    /// Set the ElmRuntime for this adapter
    pub fn set_runtime(&mut self, runtime: ElmRuntime) {
        self.elm_runtime = Some(runtime);
    }

    /// Convert Action to RawMsg for ElmRuntime processing
    fn action_to_raw_msg(&self, action: Action) -> Option<RawMsg> {
        match action {
            Action::Key(key) => Some(RawMsg::Key(key)),
            Action::Tick => Some(RawMsg::Tick),
            Action::Resize(width, height) => Some(RawMsg::Resize(width, height)),
            Action::ReceiveEvent(event) => Some(RawMsg::ReceiveEvent(event)),
            // Send/React actions should be processed by ElmRuntime directly
            Action::SendReaction(_) | Action::SendRepost(_) | Action::SendTextNote(_, _) => {
                // These will be handled by the legacy routing in app.rs
                None
            }
            // Other actions are handled differently or not converted
            _ => None,
        }
    }

    /// Get the current state from ElmRuntime
    pub fn get_current_state(&self) -> Option<&AppState> {
        self.elm_runtime.as_ref().map(|runtime| runtime.state())
    }
}

impl Default for ElmHomeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for ElmHomeAdapter {
    fn register_action_handler(&mut self, tx: mpsc::UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn register_config_handler(
        &mut self,
        _config: crate::infrastructure::config::Config,
    ) -> Result<()> {
        // ElmHome doesn't need direct config access since it uses AppState
        Ok(())
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        // Check if we're in input mode and handle it specially
        if let Some(runtime) = &mut self.elm_runtime {
            let state = runtime.state();
            if state.ui.show_input {
                // In input mode, only allow specific actions
                match &action {
                    Action::Key(key) => {
                        // Process all keys uniformly through pending_keys approach
                        // This replaces the previous navigation key special handling
                        use crate::core::raw_msg::RawMsg;
                        runtime.send_raw_msg(RawMsg::Key(*key));
                        if let Err(e) = runtime.run_update_cycle() {
                            log::error!("ElmRuntime error in ElmHomeAdapter: {}", e);
                            return Ok(Some(Action::Error(format!("ElmRuntime error: {}", e))));
                        }
                        return Ok(None);
                    }
                    Action::Unselect => {
                        // Allow Escape to close input
                        use crate::core::msg::Msg;
                        runtime.send_msg(Msg::Ui(UiMsg::CancelInput));
                        if let Err(e) = runtime.run_update_cycle() {
                            log::error!("ElmRuntime error in ElmHomeAdapter: {}", e);
                            return Ok(Some(Action::Error(format!("ElmRuntime error: {}", e))));
                        }
                        return Ok(None);
                    }
                    Action::SubmitTextNote => {
                        log::info!("ElmHomeAdapter: Submitting text note");
                        use crate::core::msg::Msg;
                        runtime.send_msg(Msg::Ui(UiMsg::SubmitNote));
                        if let Err(e) = runtime.run_update_cycle() {
                            log::error!("ElmRuntime error in ElmHomeAdapter: {}", e);
                            return Ok(Some(Action::Error(format!("ElmRuntime error: {}", e))));
                        }
                        return Ok(None);
                    }
                    Action::Quit | Action::Suspend | Action::Resume | Action::Resize(_, _) => {
                        // Allow system actions even in input mode
                    }
                    _ => {
                        // Block all other actions when in input mode
                        log::debug!(
                            "ElmHomeAdapter: Blocking action {:?} - input mode active",
                            action
                        );
                        return Ok(None);
                    }
                }
            }
        }

        // Handle specific actions that need special processing
        match &action {
            Action::Unselect => {
                // Handle Unselect in normal mode (deselect note and clear status)
                if let Some(runtime) = &mut self.elm_runtime {
                    use crate::core::msg::Msg;
                    runtime.send_msg(Msg::Timeline(TimelineMsg::DeselectNote));
                    if let Err(e) = runtime.run_update_cycle() {
                        log::error!("ElmRuntime error in ElmHomeAdapter: {}", e);
                        return Ok(Some(Action::Error(format!("ElmRuntime error: {}", e))));
                    }
                }
                return Ok(None);
            }
            // Convert React/Repost actions to Send actions with selected event
            Action::React => {
                if let Some(runtime) = &mut self.elm_runtime {
                    let state = runtime.state();
                    if let Some(selected_event) = state.selected_note() {
                        use crate::core::msg::Msg;
                        runtime.send_msg(Msg::SendReaction(selected_event.clone()));
                        if let Err(e) = runtime.run_update_cycle() {
                            log::error!("ElmRuntime error in ElmHomeAdapter: {}", e);
                            return Ok(Some(Action::Error(format!("ElmRuntime error: {}", e))));
                        }
                    } else {
                        log::warn!("ElmHomeAdapter: React action but no event selected");
                    }
                }
                return Ok(None);
            }
            Action::Repost => {
                if let Some(runtime) = &mut self.elm_runtime {
                    let state = runtime.state();
                    if let Some(selected_event) = state.selected_note() {
                        use crate::core::msg::Msg;
                        runtime.send_msg(Msg::SendRepost(selected_event.clone()));
                        if let Err(e) = runtime.run_update_cycle() {
                            log::error!("ElmRuntime error in ElmHomeAdapter: {}", e);
                            return Ok(Some(Action::Error(format!("ElmRuntime error: {}", e))));
                        }
                    } else {
                        log::warn!("ElmHomeAdapter: Repost action but no event selected");
                    }
                }
                return Ok(None);
            }
            // Send actions should be processed by ElmRuntime via messages
            Action::SendReaction(event) => {
                if let Some(runtime) = &mut self.elm_runtime {
                    use crate::core::msg::Msg;
                    runtime.send_msg(Msg::SendReaction(event.clone()));
                    if let Err(e) = runtime.run_update_cycle() {
                        log::error!("ElmRuntime error in ElmHomeAdapter: {}", e);
                        return Ok(Some(Action::Error(format!("ElmRuntime error: {}", e))));
                    }
                }
                return Ok(None);
            }
            Action::SendRepost(event) => {
                if let Some(runtime) = &mut self.elm_runtime {
                    use crate::core::msg::Msg;
                    runtime.send_msg(Msg::SendRepost(event.clone()));
                    if let Err(e) = runtime.run_update_cycle() {
                        log::error!("ElmRuntime error in ElmHomeAdapter: {}", e);
                        return Ok(Some(Action::Error(format!("ElmRuntime error: {}", e))));
                    }
                }
                return Ok(None);
            }
            Action::NewTextNote => {
                if let Some(runtime) = &mut self.elm_runtime {
                    use crate::core::msg::Msg;
                    runtime.send_msg(Msg::Ui(UiMsg::ShowNewNote));
                    if let Err(e) = runtime.run_update_cycle() {
                        log::error!("ElmRuntime error in ElmHomeAdapter: {}", e);
                        return Ok(Some(Action::Error(format!("ElmRuntime error: {}", e))));
                    }
                }
                return Ok(None);
            }
            Action::ReplyTextNote => {
                if let Some(runtime) = &mut self.elm_runtime {
                    let state = runtime.state();
                    if let Some(selected_event) = state.selected_note() {
                        use crate::core::msg::Msg;
                        runtime.send_msg(Msg::Ui(UiMsg::ShowReply(selected_event.clone())));
                        if let Err(e) = runtime.run_update_cycle() {
                            log::error!("ElmRuntime error in ElmHomeAdapter: {}", e);
                            return Ok(Some(Action::Error(format!("ElmRuntime error: {}", e))));
                        }
                    } else {
                        log::warn!("ElmHomeAdapter: Reply action but no event selected");
                    }
                }
                return Ok(None);
            }
            Action::SubmitTextNote => {
                if let Some(runtime) = &mut self.elm_runtime {
                    use crate::core::msg::Msg;
                    runtime.send_msg(Msg::Ui(UiMsg::SubmitNote));
                    if let Err(e) = runtime.run_update_cycle() {
                        log::error!("ElmRuntime error in ElmHomeAdapter: {}", e);
                        return Ok(Some(Action::Error(format!("ElmRuntime error: {}", e))));
                    }
                }
                return Ok(None);
            }
            Action::SendTextNote(content, tags) => {
                if let Some(runtime) = &mut self.elm_runtime {
                    use crate::core::msg::Msg;
                    runtime.send_msg(Msg::SendTextNote(content.clone(), tags.clone()));
                    if let Err(e) = runtime.run_update_cycle() {
                        log::error!("ElmRuntime error in ElmHomeAdapter: {}", e);
                        return Ok(Some(Action::Error(format!("ElmRuntime error: {}", e))));
                    }
                } else {
                    log::error!("ElmHomeAdapter: No ElmRuntime available for SendTextNote");
                }
                return Ok(None);
            }
            _ => {}
        }

        // Convert Action to RawMsg and process through ElmRuntime
        let raw_msg = self.action_to_raw_msg(action);

        if let Some(runtime) = &mut self.elm_runtime {
            if let Some(raw_msg) = raw_msg {
                runtime.send_raw_msg(raw_msg);

                // Process all pending messages and execute commands
                if let Err(e) = runtime.run_update_cycle() {
                    log::error!("ElmRuntime error in ElmHomeAdapter: {}", e);
                    return Ok(Some(Action::Error(format!("ElmRuntime error: {}", e))));
                }
            }
        }

        // Return None since ElmRuntime handles state management internally
        Ok(None)
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) -> Result<()> {
        log::debug!("ElmHomeAdapter: draw() called");

        if self.elm_runtime.is_none() {
            log::error!("ElmHomeAdapter: ElmRuntime is None!");

            // Render error message
            use ratatui::{
                style::{Color, Style},
                text::Text,
                widgets::Paragraph,
            };
            let text = Text::from("ElmHome Error: No runtime available")
                .style(Style::default().fg(Color::Red));
            let paragraph = Paragraph::new(text);
            f.render_widget(paragraph, area);
            return Ok(());
        }

        if let Some(runtime) = &mut self.elm_runtime {
            let state = runtime.state();
            log::debug!(
                "ElmHomeAdapter: Rendering with state - timeline_len: {}",
                state.timeline_len()
            );
            self.elm_home.render(f, area, state);
        } else {
            log::error!("ElmHomeAdapter: State is None despite runtime existing!");
        }
        Ok(())
    }

    fn init(&mut self, _area: Rect) -> Result<()> {
        Ok(())
    }

    fn handle_events(
        &mut self,
        _event: Option<crate::infrastructure::tui::Event>,
    ) -> Result<Option<Action>> {
        // Events are handled through the standard Action flow, so no special handling needed
        Ok(None)
    }

    fn is_elm_home_adapter(&self) -> bool {
        true
    }

    fn as_elm_home_adapter(&mut self) -> Option<&mut ElmHomeAdapter> {
        Some(self)
    }

    fn as_elm_home_adapter_ref(&self) -> Option<&ElmHomeAdapter> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn create_test_adapter() -> ElmHomeAdapter {
        ElmHomeAdapter::new()
    }

    fn create_test_action_tx() -> mpsc::UnboundedSender<Action> {
        let (tx, _rx) = mpsc::unbounded_channel();
        tx
    }

    #[test]
    fn test_adapter_creation() {
        let adapter = create_test_adapter();
        assert!(adapter.elm_runtime.is_none());
        assert!(adapter.action_tx.is_none());
    }

    #[test]
    fn test_action_to_raw_msg_conversion() {
        let adapter = create_test_adapter();

        // Test key conversion
        let key_event = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let action = Action::Key(key_event);
        let raw_msg = adapter.action_to_raw_msg(action);
        assert!(matches!(raw_msg, Some(RawMsg::Key(_))));

        // Test tick conversion
        let action = Action::Tick;
        let raw_msg = adapter.action_to_raw_msg(action);
        assert!(matches!(raw_msg, Some(RawMsg::Tick)));

        // Test resize conversion
        let action = Action::Resize(80, 24);
        let raw_msg = adapter.action_to_raw_msg(action);
        assert!(matches!(raw_msg, Some(RawMsg::Resize(80, 24))));

        // Test unsupported action
        let action = Action::Quit;
        let raw_msg = adapter.action_to_raw_msg(action);
        assert!(raw_msg.is_none());
    }

    #[test]
    fn test_register_action_handler() {
        let mut adapter = create_test_adapter();
        let tx = create_test_action_tx();

        let result = adapter.register_action_handler(tx);
        assert!(result.is_ok());
        assert!(adapter.action_tx.is_some());
    }

    // TODO: this test fails on CI due to no configuration file
    // #[test]
    // fn test_register_config_handler() {
    //     let mut adapter = create_test_adapter();
    //     let config = crate::config::Config::new().unwrap();
    //
    //     let result = adapter.register_config_handler(config);
    //     assert!(result.is_ok());
    // }

    #[test]
    fn test_init() {
        let mut adapter = create_test_adapter();
        let area = Rect::new(0, 0, 80, 24);

        let result = adapter.init(area);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_events() {
        let mut adapter = create_test_adapter();
        let event = None;

        let result = adapter.handle_events(event);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_get_current_state_without_runtime() {
        let adapter = create_test_adapter();
        assert!(adapter.get_current_state().is_none());
    }

    #[test]
    fn test_update_without_runtime() {
        let mut adapter = create_test_adapter();
        let action = Action::Tick;

        let result = adapter.update(action);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
