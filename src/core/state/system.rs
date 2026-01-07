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
#[derive(Debug, Clone, Default)]
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
