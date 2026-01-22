use std::time::Instant;

pub enum Message {
    FrameRecorded { now: Option<Instant> },
}

/// FPS measurement data
#[derive(Debug, Clone, PartialEq)]
pub struct Fps {
    app_fps: Option<f64>,
    app_frames: u32,
    last_update: Instant,
}

impl Fps {
    /// Create a new FPS data tracker
    pub fn new() -> Self {
        Self {
            app_fps: None,
            app_frames: 0,
            last_update: Instant::now(),
        }
    }

    pub fn app_fps(&self) -> Option<f64> {
        self.app_fps
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::FrameRecorded { now } => {
                self.app_frames += 1;
                let now = now.unwrap_or_else(Instant::now);
                let elapsed = (now - self.last_update).as_secs_f64();

                if elapsed >= 1.0 {
                    let fps = self.app_frames as f64 / elapsed;
                    self.app_fps = Some(fps);
                    self.last_update = now;
                    self.app_frames = 0;
                }
            }
        }
    }
}

impl Default for Fps {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn test_fps_creation() {
        let fps = Fps::new();
        assert_eq!(fps.app_fps, None);
        assert_eq!(fps.app_frames, 0);
    }

    #[test]
    fn test_fps_default() {
        let fps1 = Fps::new();
        let fps2 = Fps::default();

        assert_eq!(fps1.app_fps, fps2.app_fps);
        assert_eq!(fps1.app_frames, fps2.app_frames);
    }

    #[test]
    fn test_fps_frame_counting() {
        let mut fps = Fps::new();

        // Record frames quickly
        for _ in 0..10 {
            fps.update(Message::FrameRecorded { now: None });
            assert_eq!(fps.app_fps, None);
        }

        // Frame count should be 10
        assert_eq!(fps.app_frames, 10);
    }

    #[test]
    fn test_fps_calculates_fps_correctly() {
        let mut fps = Fps::new();
        let start = Instant::now();

        // Record 59 frames over approximately 1 second (just before threshold)
        for i in 0..59 {
            let now = Some(start + Duration::from_millis(i * 1000 / 60));
            fps.update(Message::FrameRecorded { now });

            // Should not trigger FPS calculation yet
            assert_eq!(fps.app_fps(), None);
        }

        // 60th frame at exactly 1.0 second should trigger FPS calculation
        let now = Some(start + Duration::from_secs(1));
        fps.update(Message::FrameRecorded { now });
        assert!(fps.app_fps.is_some());
        let app_fps = fps.app_fps.expect("FPS calculation should succeed");

        // Should be approximately 60 FPS (60 frames / 1.0 second)
        // Allow small floating point error
        assert!(
            (app_fps - 60.0).abs() < 0.01,
            "Expected ~60 FPS, got {app_fps}"
        );
        // Frame count should be reset after calculation
        assert_eq!(fps.app_frames, 0);
    }

    #[test]
    fn test_fps_multiple_intervals() {
        let mut fps = Fps::new();
        let start = Instant::now();

        // First interval: 29 frames before 1 second
        for i in 0..29 {
            let now = Some(start + Duration::from_millis(i * 1000 / 30));
            fps.update(Message::FrameRecorded { now });
            assert_eq!(fps.app_fps, None);
        }

        // Trigger first FPS calculation at 1 second mark (30th frame)
        let now = Some(start + Duration::from_secs(1));
        fps.update(Message::FrameRecorded { now });
        let app_fps1 = fps.app_fps.expect("Should calculate FPS");
        assert!(
            (app_fps1 - 30.0).abs() < 0.01,
            "Expected ~30 FPS, got {app_fps1}"
        );
        assert_eq!(fps.app_frames, 0, "Frame count should be reset");

        // Second interval: 59 frames in next second
        for i in 0..59 {
            let now = Some(start + Duration::from_secs(1) + Duration::from_millis(i * 1000 / 60));
            fps.update(Message::FrameRecorded { now });
        }

        // Trigger second FPS calculation at 2 second mark (60th frame in this interval)
        let now = Some(start + Duration::from_secs(2));
        fps.update(Message::FrameRecorded { now });
        let app_fps2 = fps.app_fps.expect("Should calculate FPS");
        assert!(
            (app_fps2 - 60.0).abs() < 0.01,
            "Expected ~60 FPS, got {app_fps2}"
        );
        assert_eq!(fps.app_frames, 0, "Frame count should be reset");
    }
}
