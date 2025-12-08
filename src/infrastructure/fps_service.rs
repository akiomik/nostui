use std::time::Instant;
use tokio::sync::mpsc;

use crate::core::raw_msg::RawMsg;

/// FPS calculation service that runs independently and sends updates
/// This handles the time-based calculations and sends raw messages to the runtime
pub struct FpsService {
    app_start_time: Instant,
    pub app_frames: u32,

    render_start_time: Instant,
    pub render_frames: u32,

    raw_msg_tx: mpsc::UnboundedSender<RawMsg>,
}

impl FpsService {
    pub fn new(raw_msg_tx: mpsc::UnboundedSender<RawMsg>) -> Self {
        Self {
            app_start_time: Instant::now(),
            app_frames: 0,
            render_start_time: Instant::now(),
            render_frames: 0,
            raw_msg_tx,
        }
    }

    /// Call this on each app tick
    pub fn on_app_tick(&mut self) {
        self.app_frames += 1;
        let now = Instant::now();
        let elapsed = (now - self.app_start_time).as_secs_f64();

        if elapsed >= 1.0 {
            let app_fps = self.app_frames as f64 / elapsed;
            self.app_start_time = now;
            self.app_frames = 0;

            // Send FPS update as a raw message
            let _ = self.raw_msg_tx.send(RawMsg::AppFpsUpdate(app_fps));
        }
    }

    /// Call this on each render
    pub fn on_render(&mut self) {
        self.render_frames += 1;
        let now = Instant::now();
        let elapsed = (now - self.render_start_time).as_secs_f64();

        if elapsed >= 1.0 {
            let render_fps = self.render_frames as f64 / elapsed;
            self.render_start_time = now;
            self.render_frames = 0;

            // Send FPS update as a raw message
            let _ = self.raw_msg_tx.send(RawMsg::RenderFpsUpdate(render_fps));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn test_fps_service_creation() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let service = FpsService::new(tx);

        assert_eq!(service.app_frames, 0);
        assert_eq!(service.render_frames, 0);
    }

    #[test]
    fn test_fps_service_app_tick() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut service = FpsService::new(tx);

        // Tick multiple times quickly (shouldn't send message yet)
        for _ in 0..5 {
            service.on_app_tick();
        }

        // Should not have sent any messages yet (less than 1 second)
        assert!(rx.try_recv().is_err());
        assert_eq!(service.app_frames, 5);
    }

    #[test]
    fn test_fps_service_render_tick() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut service = FpsService::new(tx);

        // Render multiple times quickly (shouldn't send message yet)
        for _ in 0..3 {
            service.on_render();
        }

        // Should not have sent any messages yet (less than 1 second)
        assert!(rx.try_recv().is_err());
        assert_eq!(service.render_frames, 3);
    }

    #[tokio::test]
    async fn test_fps_service_integration() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut service = FpsService::new(tx);

        // Simulate some activity
        service.on_app_tick();
        service.on_render();

        // Verify the service is working (frames incremented)
        assert_eq!(service.app_frames, 1);
        assert_eq!(service.render_frames, 1);

        // Messages won't be sent until 1 second passes
        assert!(rx.try_recv().is_err());
    }
}
