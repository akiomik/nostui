use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;

use color_eyre::eyre::Result;
use futures::future;
use ratatui::backend::TestBackend;
use ratatui::prelude::*;

use crate::infrastructure::tui::{Event, Frame, TuiLike};

/// Test-oriented TUI implementation backed by ratatui::backend::TestBackend.
/// - enter/exit are no-ops (no raw mode / alternate screen).
/// - next() returns events from an internal queue (non-blocking, immediate).
/// - draw() increments an internal counter for assertions.
pub struct TestTui {
    term: Terminal<TestBackend>,
    events: VecDeque<Event>,
    draws: usize,
}

impl TestTui {
    pub fn new(width: u16, height: u16) -> Result<Self> {
        let backend = TestBackend::new(width, height);
        let term = Terminal::new(backend)?;
        Ok(Self {
            term,
            events: VecDeque::new(),
            draws: 0,
        })
    }

    pub fn with_events(
        width: u16,
        height: u16,
        events: impl IntoIterator<Item = Event>,
    ) -> Result<Self> {
        let mut this = Self::new(width, height)?;
        this.events.extend(events);
        Ok(this)
    }

    /// Expose draw count for tests.
    pub fn draw_count(&self) -> usize {
        self.draws
    }

    /// Enqueue a single event for tests.
    pub fn enqueue_event(&mut self, ev: Event) {
        self.events.push_back(ev);
    }

    fn resize_impl(&mut self, area: Rect) -> Result<()> {
        self.term.backend_mut().resize(area.width, area.height);
        Ok(())
    }
}

impl TuiLike for TestTui {
    fn enter(&mut self) -> Result<()> {
        // no-op for test UI
        Ok(())
    }

    fn exit(&mut self) -> Result<()> {
        // no-op for test UI
        Ok(())
    }

    fn draw(&mut self, f: &mut dyn FnMut(&mut Frame<'_>)) -> Result<()> {
        self.term.draw(|frame| f(frame))?;
        self.draws += 1;
        Ok(())
    }

    fn resize(&mut self, area: Rect) -> Result<()> {
        self.resize_impl(area)
    }

    fn next(&mut self) -> Pin<Box<dyn Future<Output = Option<Event>> + Send + '_>> {
        let ev = self.events.pop_front();
        Box::pin(future::ready(ev))
    }
}
