use ratatui::prelude::*;
use ratatui::widgets::{Block, Widget};

use crate::model::fps::Fps;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FpsWidget {
    fps: Fps,
}

impl FpsWidget {
    pub fn new(fps: Fps) -> Self {
        Self { fps }
    }
}

impl Widget for FpsWidget {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        // Get FPS data from system state
        let fps_text = match &self.fps.app_fps() {
            Some(app_fps) => format!("{app_fps:.2} ticks per sec"),
            None => "".to_owned(),
        };

        // Render as a dimmed, right-aligned title
        let block = Block::default().title_top(Line::from(fps_text.dim()).right_aligned());
        block.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use crate::model::fps::Message;

    use super::*;

    #[test]
    fn test_render_none() {
        let fps = Fps::new();

        let widget = FpsWidget::new(fps);
        let area = Rect::new(0, 0, 80, 20);
        let mut buffer = Buffer::empty(area);

        widget.render(area, &mut buffer);
        let content: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(!content.contains("ticks per sec"));
    }

    #[test]
    fn test_render_some() {
        let mut fps = Fps::new();
        let now = Instant::now();
        fps.update(Message::FrameRecorded { now: Some(now) });
        fps.update(Message::FrameRecorded {
            now: Some(now + Duration::from_secs(1)),
        });

        let widget = FpsWidget::new(fps);
        let area = Rect::new(0, 0, 80, 20);
        let mut buffer = Buffer::empty(area);

        widget.render(area, &mut buffer);
        let content: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("2.00 ticks per sec"));
    }
}
