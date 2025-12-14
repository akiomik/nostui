use color_eyre::eyre::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::infrastructure::tui;

/// Thin facade around the TUI backend to execute rendering-related commands.
/// When no TUI is available (headless), methods are no-ops.
#[derive(Clone, Default)]
pub struct TuiService {
    inner: Option<Arc<Mutex<tui::Tui>>>,
}

impl TuiService {
    pub fn new(inner: Option<tui::Tui>) -> Self {
        Self {
            inner: inner.map(|t| Arc::new(Mutex::new(t))),
        }
    }

    pub fn from_shared(inner: Option<Arc<Mutex<tui::Tui>>>) -> Self {
        Self { inner }
    }

    pub fn is_available(&self) -> bool {
        self.inner.is_some()
    }

    pub async fn render<F>(&self, mut draw: F) -> Result<()>
    where
        F: FnMut(&mut ratatui::Frame<'_>) + Send + 'static,
    {
        if let Some(inner) = &self.inner {
            let mut tui = inner.lock().await;
            tui.draw(|f| {
                draw(f);
            })?;
        }
        Ok(())
    }

    pub async fn resize(&self, width: u16, height: u16) -> Result<()> {
        if let Some(inner) = &self.inner {
            let mut tui = inner.lock().await;
            use ratatui::prelude::Rect;
            tui.resize(Rect::new(0, 0, width, height))?;
        }
        Ok(())
    }
}
