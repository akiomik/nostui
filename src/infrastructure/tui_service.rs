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

    /// Create a channel-driven TuiService like NostrService pattern.
    /// Returns (command_sender, service).
    pub fn new_with_channel(
        inner: Option<Arc<Mutex<tui::Tui>>>,
    ) -> (
        tokio::sync::mpsc::UnboundedSender<crate::core::cmd::TuiCommand>,
        Self,
    ) {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        (tx, Self { inner })
    }

    /// Run background loop consuming TuiCommand from the given receiver.
    pub fn run(
        self,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<crate::core::cmd::TuiCommand>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            use crate::core::cmd::TuiCommand;
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    TuiCommand::Resize { width, height } => {
                        if let Some(inner) = &self.inner {
                            let mut tui = inner.lock().await;
                            let _ = tui.resize(ratatui::prelude::Rect::new(0, 0, width, height));
                        }
                    }
                    TuiCommand::Render => {
                        // Rendering stays orchestrated by AppRunner; ignore here.
                    }
                }
            }
        })
    }

    pub fn is_available(&self) -> bool {
        self.inner.is_some()
    }

    pub async fn enter(&self) -> Result<()> {
        if let Some(inner) = &self.inner {
            let mut tui = inner.lock().await;
            tui.enter()?;
        }
        Ok(())
    }

    pub async fn exit(&self) -> Result<()> {
        if let Some(inner) = &self.inner {
            let mut tui = inner.lock().await;
            tui.exit()?;
        }
        Ok(())
    }

    pub async fn next_event(&self) -> Option<tui::Event> {
        if let Some(inner) = &self.inner {
            let mut tui = inner.lock().await;
            tui.next().await
        } else {
            None
        }
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
