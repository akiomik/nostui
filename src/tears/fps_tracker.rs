//! FPS tracking utility
//!
//! A simple utility for tracking frames per second.

use std::time::Instant;

/// Tracks frames per second over time
#[derive(Debug)]
pub struct FpsTracker {
    last_update: Instant,
    frame_count: u32,
}

impl FpsTracker {
    /// Create a new FPS tracker
    pub fn new() -> Self {
        Self {
            last_update: Instant::now(),
            frame_count: 0,
        }
    }

    /// Record a frame and optionally return the calculated FPS
    ///
    /// Returns `Some(fps)` if 1 second has elapsed since the last update,
    /// otherwise returns `None`.
    pub fn record_frame(&mut self) -> Option<f64> {
        self.frame_count += 1;
        let now = Instant::now();
        let elapsed = (now - self.last_update).as_secs_f64();

        if elapsed >= 1.0 {
            let fps = self.frame_count as f64 / elapsed;
            self.last_update = now;
            self.frame_count = 0;
            Some(fps)
        } else {
            None
        }
    }
}

impl Default for FpsTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fps_tracker_creation() {
        let tracker = FpsTracker::new();
        assert_eq!(tracker.frame_count, 0);
    }

    #[test]
    fn test_fps_tracker_frame_counting() {
        let mut tracker = FpsTracker::new();

        // Record frames quickly (should not return FPS yet)
        for _ in 0..10 {
            assert!(tracker.record_frame().is_none());
        }

        // Frame count should be 10
        assert_eq!(tracker.frame_count, 10);
    }

    #[test]
    fn test_fps_tracker_default() {
        let tracker1 = FpsTracker::new();
        let tracker2 = FpsTracker::default();

        assert_eq!(tracker1.frame_count, tracker2.frame_count);
    }
}
