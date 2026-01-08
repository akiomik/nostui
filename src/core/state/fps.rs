use std::time::Instant;

/// FPS measurement data
#[derive(Debug)]
pub struct FpsState {
    app_fps: f64,
    app_frames: u32,
    last_update: Instant,
}

impl FpsState {
    /// Create a new FPS data tracker
    pub fn new() -> Self {
        Self {
            app_fps: 0.0,
            app_frames: 0,
            last_update: Instant::now(),
        }
    }

    /// Record a frame and optionally return the calculated FPS
    ///
    /// Returns `Some(fps)` if 1 second has elapsed since the last update,
    /// otherwise returns `None`.
    ///
    /// # Arguments
    /// * `now` - Optional current time for testing. Uses `Instant::now()` if `None`.
    pub fn record_frame(&mut self, now: Option<Instant>) -> Option<f64> {
        self.app_frames += 1;
        let now = now.unwrap_or_else(Instant::now);
        let elapsed = (now - self.last_update).as_secs_f64();

        if elapsed >= 1.0 {
            let fps = self.app_frames as f64 / elapsed;
            self.app_fps = fps;
            self.last_update = now;
            self.app_frames = 0;
            Some(fps)
        } else {
            None
        }
    }

    pub fn app_fps(&self) -> f64 {
        self.app_fps
    }
}

impl Default for FpsState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::FpsState;
    use std::time::{Duration, Instant};

    #[test]
    fn test_fps_state_creation() {
        let state = FpsState::new();
        assert_eq!(state.app_fps, 0.0);
        assert_eq!(state.app_frames, 0);
    }

    #[test]
    fn test_fps_state_default() {
        let state1 = FpsState::new();
        let state2 = FpsState::default();

        assert_eq!(state1.app_fps, state2.app_fps);
        assert_eq!(state1.app_frames, state2.app_frames);
    }

    #[test]
    fn test_fps_state_frame_counting() {
        let mut state = FpsState::new();

        // Record frames quickly (should not return FPS yet)
        for _ in 0..10 {
            assert!(state.record_frame(None).is_none());
        }

        // Frame count should be 10
        assert_eq!(state.app_frames, 10);
    }

    #[test]
    fn test_fps_state_calculates_fps_correctly() {
        let mut state = FpsState::new();
        let start = Instant::now();

        // Record 59 frames over approximately 1 second (just before threshold)
        for i in 0..59 {
            let now = start + Duration::from_millis(i * 1000 / 60);
            let result = state.record_frame(Some(now));
            // Should not trigger FPS calculation yet
            assert!(result.is_none(), "Frame {i} should not calculate FPS yet");
        }

        // 60th frame at exactly 1.0 second should trigger FPS calculation
        let now = start + Duration::from_secs(1);
        let result = state.record_frame(Some(now));
        assert!(result.is_some(), "Frame at 1.0s should calculate FPS");
        let fps = result.expect("FPS calculation should succeed");

        // Should be approximately 60 FPS (60 frames / 1.0 second)
        // Allow small floating point error
        assert!((fps - 60.0).abs() < 0.01, "Expected ~60 FPS, got {fps}");
        // app_fps should be updated
        assert_eq!(state.app_fps, fps);
        // Frame count should be reset after calculation
        assert_eq!(state.app_frames, 0);
    }

    #[test]
    fn test_fps_state_multiple_intervals() {
        let mut state = FpsState::new();
        let start = Instant::now();

        // First interval: 29 frames before 1 second
        for i in 0..29 {
            let now = start + Duration::from_millis(i * 1000 / 30);
            let result = state.record_frame(Some(now));
            assert!(result.is_none(), "Frame {i} should not calculate FPS yet");
        }

        // Trigger first FPS calculation at 1 second mark (30th frame)
        let now = start + Duration::from_secs(1);
        let fps1 = state.record_frame(Some(now)).expect("Should calculate FPS");
        assert!((fps1 - 30.0).abs() < 0.01, "Expected ~30 FPS, got {fps1}");
        assert_eq!(state.app_frames, 0, "Frame count should be reset");

        // Second interval: 59 frames in next second
        for i in 0..59 {
            let now = start + Duration::from_secs(1) + Duration::from_millis(i * 1000 / 60);
            let result = state.record_frame(Some(now));
            assert!(
                result.is_none(),
                "Frame {} should not calculate FPS yet",
                i + 30
            );
        }

        // Trigger second FPS calculation at 2 second mark (60th frame in this interval)
        let now = start + Duration::from_secs(2);
        let fps2 = state.record_frame(Some(now)).expect("Should calculate FPS");
        assert!((fps2 - 60.0).abs() < 0.01, "Expected ~60 FPS, got {fps2}");
        assert_eq!(state.app_frames, 0, "Frame count should be reset");
    }
}
