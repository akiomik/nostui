use crate::core::{cmd::Cmd, msg::system::SystemMsg};

/// System-related state
#[derive(Debug, Clone)]
pub struct SystemState {
    pub should_quit: bool,
    pub should_suspend: bool,
    pub fps_data: FpsData,
    pub status_message: Option<String>,
    pub is_loading: bool,
}

/// FPS measurement data
#[derive(Debug, Clone)]
pub struct FpsData {
    pub app_fps: f64,
    pub render_fps: f64,
    pub app_frames: u32,
    pub render_frames: u32,
    // Only holds computed values since Instant is not Clone
}

impl Default for SystemState {
    fn default() -> Self {
        Self {
            should_quit: false,
            should_suspend: false,
            fps_data: FpsData::default(),
            status_message: None,
            is_loading: true,
        }
    }
}

impl Default for FpsData {
    fn default() -> Self {
        Self {
            app_fps: 0.0,
            render_fps: 0.0,
            app_frames: 0,
            render_frames: 0,
        }
    }
}

impl SystemState {
    /// System-specific update function
    /// Returns: Generated commands
    pub fn update(&mut self, msg: SystemMsg) -> Vec<Cmd> {
        match msg {
            // System control
            SystemMsg::Quit => {
                self.should_quit = true;
                vec![]
            }

            SystemMsg::Suspend => {
                self.should_suspend = true;
                vec![]
            }

            SystemMsg::Resume => {
                self.should_suspend = false;
                vec![]
            }

            SystemMsg::Resize(width, height) => {
                // Resize generates a TUI resize command
                vec![Cmd::Tui(crate::core::cmd::TuiCommand::Resize {
                    width,
                    height,
                })]
            }

            // Status management
            SystemMsg::UpdateStatusMessage(message) => {
                self.status_message = Some(message);
                vec![]
            }

            SystemMsg::ClearStatusMessage => {
                self.status_message = None;
                vec![]
            }

            SystemMsg::SetLoading(loading) => {
                self.is_loading = loading;
                vec![]
            }

            SystemMsg::ShowError(error) => {
                self.status_message = Some(format!("Error: {}", error));
                vec![]
            }

            // Performance tracking
            SystemMsg::UpdateAppFps(fps) => {
                self.fps_data.app_fps = fps;
                self.fps_data.app_frames += 1;
                vec![]
            }

            SystemMsg::UpdateRenderFps(fps) => {
                self.fps_data.render_fps = fps;
                self.fps_data.render_frames += 1;
                vec![]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Unit tests for SystemState isolation
    #[test]
    fn test_system_state_quit_isolated() {
        let mut system = SystemState::default();
        assert!(!system.should_quit);

        let cmds = system.update(SystemMsg::Quit);

        assert!(system.should_quit);
        assert!(cmds.is_empty());
        // No AppState needed! Unit testing is possible
    }

    #[test]
    fn test_system_state_status_message_isolated() {
        let mut system = SystemState::default();
        assert!(system.status_message.is_none());

        let cmds = system.update(SystemMsg::UpdateStatusMessage("Test".to_string()));

        assert_eq!(system.status_message, Some("Test".to_string()));
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_system_state_resize_generates_command() {
        let mut system = SystemState::default();

        let cmds = system.update(SystemMsg::Resize(80, 24));

        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::Tui(crate::core::cmd::TuiCommand::Resize { width, height }) => {
                assert_eq!(*width, 80);
                assert_eq!(*height, 24);
            }
            _ => panic!("Expected Resize command"),
        }
    }

    // tests/system_state_unit_tests.rs から移行
    #[test]
    fn test_status_message_flow_unit() {
        let mut system = SystemState::default();

        // Initial state should have no message
        assert!(system.status_message.is_none());

        // Update with status message
        let test_message = "Connected to relay".to_string();
        let cmds = system.update(SystemMsg::UpdateStatusMessage(test_message.clone()));
        assert!(cmds.is_empty());
        assert_eq!(system.status_message, Some(test_message));

        // Clear status message
        let cmds = system.update(SystemMsg::ClearStatusMessage);
        assert!(cmds.is_empty());
        assert!(system.status_message.is_none());
    }

    #[test]
    fn test_loading_state_flow_unit() {
        let mut system = SystemState::default();

        // Should start in loading state
        assert!(system.is_loading);

        // Set loading to false
        let cmds = system.update(SystemMsg::SetLoading(false));
        assert!(cmds.is_empty());
        assert!(!system.is_loading);

        // Set loading back to true
        let cmds = system.update(SystemMsg::SetLoading(true));
        assert!(cmds.is_empty());
        assert!(system.is_loading);
    }

    #[test]
    fn test_fps_updates_unit() {
        let mut system = SystemState::default();

        // Initial state
        assert_eq!(system.fps_data.app_fps, 0.0);
        assert_eq!(system.fps_data.app_frames, 0);
        assert_eq!(system.fps_data.render_fps, 0.0);
        assert_eq!(system.fps_data.render_frames, 0);

        // Test app FPS update
        let cmds = system.update(SystemMsg::UpdateAppFps(75.5));
        assert_eq!(system.fps_data.app_fps, 75.5);
        assert_eq!(system.fps_data.app_frames, 1);
        assert!(cmds.is_empty());

        // Test render FPS update
        let cmds = system.update(SystemMsg::UpdateRenderFps(144.0));
        assert_eq!(system.fps_data.render_fps, 144.0);
        assert_eq!(system.fps_data.render_frames, 1);
        assert!(cmds.is_empty());

        // Test multiple updates increment frames
        let cmds = system.update(SystemMsg::UpdateAppFps(80.0));
        assert_eq!(system.fps_data.app_fps, 80.0);
        assert_eq!(system.fps_data.app_frames, 2); // Incremented
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_error_handling_unit() {
        let mut system = SystemState::default();

        let error_message = "Connection failed";
        let cmds = system.update(SystemMsg::ShowError(error_message.to_string()));

        assert!(cmds.is_empty());
        assert_eq!(
            system.status_message,
            Some(format!("Error: {}", error_message))
        );
    }

    #[test]
    fn test_system_control_unit() {
        let mut system = SystemState::default();

        // Test quit
        assert!(!system.should_quit);
        let cmds = system.update(SystemMsg::Quit);
        assert!(system.should_quit);
        assert!(cmds.is_empty());

        // Test suspend/resume
        assert!(!system.should_suspend);
        let cmds = system.update(SystemMsg::Suspend);
        assert!(system.should_suspend);
        assert!(cmds.is_empty());

        let cmds = system.update(SystemMsg::Resume);
        assert!(!system.should_suspend);
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_combined_system_operations_unit() {
        let mut system = SystemState::default();

        // 1. Verify initial state
        assert!(system.is_loading);
        assert!(system.status_message.is_none());
        assert!(!system.should_quit);

        // 2. Set a status message
        let _cmds = system.update(SystemMsg::UpdateStatusMessage("Starting...".to_string()));
        assert_eq!(system.status_message, Some("Starting...".to_string()));

        // 3. Finish loading
        let _cmds = system.update(SystemMsg::SetLoading(false));
        assert!(!system.is_loading);

        // 4. Update FPS
        let _cmds = system.update(SystemMsg::UpdateAppFps(60.0));
        assert_eq!(system.fps_data.app_fps, 60.0);

        // 5. Trigger error (overwrites status message)
        let _cmds = system.update(SystemMsg::ShowError("Test error".to_string()));
        assert_eq!(system.status_message, Some("Error: Test error".to_string()));

        // 6. Clear status message
        let _cmds = system.update(SystemMsg::ClearStatusMessage);
        assert!(system.status_message.is_none());

        // 7. Quit
        let _cmds = system.update(SystemMsg::Quit);
        assert!(system.should_quit);
    }
}
