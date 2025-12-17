use color_eyre::eyre::Result;
use ratatui::{backend::TestBackend, Terminal};

pub struct TestTerminal {
    term: Terminal<TestBackend>,
    pub draws: usize,
}

impl TestTerminal {
    pub fn new(width: u16, height: u16) -> Result<Self> {
        let backend = TestBackend::new(width, height);
        let term = Terminal::new(backend)?;
        Ok(Self { term, draws: 0 })
    }

    pub fn draw<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut ratatui::Frame<'_>),
    {
        self.term.draw(f)?;
        self.draws += 1;
        Ok(())
    }

    pub fn resize(&mut self, area: ratatui::prelude::Rect) -> Result<()> {
        self.term.backend_mut().resize(area.width, area.height);
        Ok(())
    }
}
