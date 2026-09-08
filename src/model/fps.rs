use std::time::Instant;

/// Follow-ups the FPS tracker asks the application to perform.
///
/// Like the rest of `model`, `Fps` is side-effect free: `update` mutates state
/// and returns `Some(outcome)`, or `None` when nothing is required of the
/// application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpsOutcome {
    /// The rate the widget displays was recomputed.
    ///
    /// Reported once per measurement interval; the ticks in between leave the
    /// displayed value untouched, which is what lets the application decline the
    /// redraw they would otherwise cost (#510). A recomputed rate can be
    /// numerically identical to the one already on screen — this reports that the
    /// value was measured again, not that the rendered text differs, which is the
    /// widget's business and not the model's.
    DisplayUpdated,
}

pub enum Message {
    FrameRecorded {
        /// When the tick was handled.
        ///
        /// Expected not to precede an instant already recorded.
        /// `AppState::record_tick` satisfies that by stamping at dispatch, on the
        /// one update loop; a caller that stamped where the tick was produced would
        /// not, because independent subscription tasks push into a shared queue and
        /// can be reordered by it. `update` measures saturatingly rather than trust
        /// the caller, so violating this skews a debug counter instead of panicking
        /// the update loop on an `Instant` subtraction.
        now: Instant,
    },
}

/// FPS measurement data
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fps {
    app_fps: Option<f64>,
    app_frames: u32,
    /// Start of the interval being measured, set by the first tick.
    ///
    /// `None` until then. Reading the clock in `new` instead would anchor the
    /// first interval to construction rather than to the ticks it measures, so
    /// the first reading would depend on how long startup took and no test could
    /// drive a whole interval without also racing the wall clock.
    interval_started_at: Option<Instant>,
}

impl Fps {
    /// Create a new FPS data tracker
    pub fn new() -> Self {
        Self {
            app_fps: None,
            app_frames: 0,
            interval_started_at: None,
        }
    }

    pub fn app_fps(&self) -> Option<f64> {
        self.app_fps
    }

    pub fn update(&mut self, message: Message) -> Option<FpsOutcome> {
        match message {
            Message::FrameRecorded { now } => {
                let Some(started_at) = self.interval_started_at else {
                    // The first tick starts the first interval rather than being
                    // counted in it: there is no earlier tick to measure from.
                    self.interval_started_at = Some(now);
                    return None;
                };

                self.app_frames += 1;
                let elapsed = now.saturating_duration_since(started_at).as_secs_f64();

                if elapsed < 1.0 {
                    return None;
                }

                self.app_fps = Some(f64::from(self.app_frames) / elapsed);
                self.interval_started_at = Some(now);
                self.app_frames = 0;

                Some(FpsOutcome::DisplayUpdated)
            }
        }
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

        assert_eq!(fps1, fps2);
    }

    #[test]
    fn test_fps_frame_counting() {
        let mut fps = Fps::new();
        let start = Instant::now();

        // The first tick only starts the interval
        assert_eq!(fps.update(Message::FrameRecorded { now: start }), None);
        assert_eq!(fps.app_frames, 0);

        // Record frames quickly
        for i in 1..=10 {
            let outcome = fps.update(Message::FrameRecorded {
                now: start + Duration::from_millis(i * 10),
            });

            // Nothing is displayed yet, so nothing is asked of the application
            assert_eq!(outcome, None);
            assert_eq!(fps.app_fps, None);
        }

        // Frame count should be 10
        assert_eq!(fps.app_frames, 10);
    }

    #[test]
    fn test_fps_calculates_fps_correctly() {
        let mut fps = Fps::new();
        let start = Instant::now();

        // The first tick starts the interval the rest are measured against
        assert_eq!(fps.update(Message::FrameRecorded { now: start }), None);

        // Record 59 frames over approximately 1 second (just before threshold)
        for i in 1..60 {
            let now = start + Duration::from_millis(i * 1000 / 60);
            let outcome = fps.update(Message::FrameRecorded { now });

            // Should not trigger FPS calculation yet
            assert_eq!(outcome, None);
            assert_eq!(fps.app_fps(), None);
        }

        // 60th frame at exactly 1.0 second should trigger FPS calculation
        let now = start + Duration::from_secs(1);
        let outcome = fps.update(Message::FrameRecorded { now });
        assert_eq!(outcome, Some(FpsOutcome::DisplayUpdated));
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

        // The first tick starts the interval the rest are measured against
        assert_eq!(fps.update(Message::FrameRecorded { now: start }), None);

        // First interval: 29 frames before 1 second
        for i in 1..30 {
            let now = start + Duration::from_millis(i * 1000 / 30);
            assert_eq!(fps.update(Message::FrameRecorded { now }), None);
            assert_eq!(fps.app_fps, None);
        }

        // Trigger first FPS calculation at 1 second mark (30th frame)
        let now = start + Duration::from_secs(1);
        assert_eq!(
            fps.update(Message::FrameRecorded { now }),
            Some(FpsOutcome::DisplayUpdated)
        );
        let app_fps1 = fps.app_fps.expect("Should calculate FPS");
        assert!(
            (app_fps1 - 30.0).abs() < 0.01,
            "Expected ~30 FPS, got {app_fps1}"
        );
        assert_eq!(fps.app_frames, 0, "Frame count should be reset");

        // Second interval: 59 frames in next second
        for i in 1..60 {
            let now = start + Duration::from_secs(1) + Duration::from_millis(i * 1000 / 60);
            assert_eq!(
                fps.update(Message::FrameRecorded { now }),
                None,
                "the interval that has not elapsed yet displays nothing new"
            );
        }

        // Trigger second FPS calculation at 2 second mark (60th frame in this interval)
        let now = start + Duration::from_secs(2);
        assert_eq!(
            fps.update(Message::FrameRecorded { now }),
            Some(FpsOutcome::DisplayUpdated)
        );
        let app_fps2 = fps.app_fps.expect("Should calculate FPS");
        assert!(
            (app_fps2 - 60.0).abs() < 0.01,
            "Expected ~60 FPS, got {app_fps2}"
        );
        assert_eq!(fps.app_frames, 0, "Frame count should be reset");
    }

    /// A rate identical to the one already on screen is still reported: the model
    /// knows the value was recomputed, not how the widget renders it. Reporting
    /// costs one redraw per second, which is the cadence #510 asks for anyway.
    #[test]
    fn test_an_unchanged_rate_is_still_reported() {
        let mut fps = Fps::new();
        let start = Instant::now();

        assert_eq!(fps.update(Message::FrameRecorded { now: start }), None);

        for second in 1..=2 {
            let now = start + Duration::from_secs(second);
            assert_eq!(
                fps.update(Message::FrameRecorded { now }),
                Some(FpsOutcome::DisplayUpdated)
            );
            assert_eq!(fps.app_fps, Some(1.0));
        }
    }
}
