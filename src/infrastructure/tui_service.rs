use color_eyre::eyre::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::infrastructure::tui;

/// Thin facade around the TUI backend to execute rendering-related commands.
/// When no TUI is available (headless), methods are no-ops.
#[derive(Clone)]
pub struct TuiService {
    inner: Arc<Mutex<dyn tui::TuiLike + Send>>,
}

impl TuiService {
    pub fn new(inner: Arc<Mutex<dyn tui::TuiLike + Send>>) -> Self {
        Self { inner }
    }

    pub fn from_shared(inner: Arc<Mutex<dyn tui::TuiLike + Send>>) -> Self {
        Self { inner }
    }

    /// Create a channel-driven TuiService like NostrService pattern.
    /// Returns (command_sender, service).
    pub fn new_with_channel(
        inner: Arc<Mutex<dyn tui::TuiLike + Send>>,
    ) -> (
        tokio::sync::mpsc::UnboundedSender<crate::core::cmd::TuiCommand>,
        tokio::sync::mpsc::UnboundedReceiver<crate::core::cmd::TuiCommand>,
        Self,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (tx, rx, Self { inner })
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
                        let mut tui = self.inner.lock().await;
                        let _ = tui.resize(ratatui::prelude::Rect::new(0, 0, width, height));
                    }
                    TuiCommand::Render => {
                        // Rendering stays orchestrated by AppRunner; ignore here.
                    }
                }
            }
        })
    }

    pub async fn enter(&self) -> Result<()> {
        let mut tui = self.inner.lock().await;
        tui.enter()?;
        Ok(())
    }

    pub async fn exit(&self) -> Result<()> {
        let mut tui = self.inner.lock().await;
        tui.exit()?;
        Ok(())
    }

    pub async fn next_event(&self) -> Option<tui::Event> {
        let mut tui = self.inner.lock().await;
        tui.next().await
    }

    pub async fn render<F>(&self, mut draw: F) -> Result<()>
    where
        F: FnMut(&mut ratatui::Frame<'_>) + Send + 'static,
    {
        let mut tui = self.inner.lock().await;
        let mut closure = |f: &mut ratatui::Frame<'_>| {
            draw(f);
        };
        tui.draw(&mut closure)?;
        Ok(())
    }

    pub async fn resize(&self, width: u16, height: u16) -> Result<()> {
        let mut tui = self.inner.lock().await;
        use ratatui::prelude::Rect;
        tui.resize(Rect::new(0, 0, width, height))?;
        Ok(())
    }
}
