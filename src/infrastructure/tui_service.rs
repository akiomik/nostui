use color_eyre::eyre::Result;
use ratatui::prelude::*;
use std::sync::Arc;
use tokio::{
    sync::{mpsc, Mutex},
    task::JoinHandle,
};

use crate::{core::cmd::TuiCommand, infrastructure::tui};

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

    /// Create a channel-driven TuiService like NostrService pattern.
    /// Returns (command_sender, service).
    pub fn new_with_channel(
        inner: Arc<Mutex<dyn tui::TuiLike + Send>>,
    ) -> (
        mpsc::UnboundedSender<TuiCommand>,
        mpsc::UnboundedReceiver<TuiCommand>,
        Self,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        (tx, rx, Self { inner })
    }

    /// Run background loop consuming TuiCommand from the given receiver.
    pub fn run(self, mut rx: mpsc::UnboundedReceiver<TuiCommand>) -> JoinHandle<()> {
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    TuiCommand::Resize { width, height } => {
                        let mut tui = self.inner.lock().await;
                        let _ = tui.resize(Rect::new(0, 0, width, height));
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
        F: FnMut(&mut Frame<'_>) + Send + 'static,
    {
        let mut tui = self.inner.lock().await;
        let mut closure = |f: &mut Frame<'_>| {
            draw(f);
        };
        tui.draw(&mut closure)?;
        Ok(())
    }

    pub async fn resize(&self, width: u16, height: u16) -> Result<()> {
        let mut tui = self.inner.lock().await;
        tui.resize(Rect::new(0, 0, width, height))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::sleep;

    use super::*;
    use crate::infrastructure::tui::test::TestTui;

    fn make_service_with_test_tui(w: u16, h: u16) -> (TuiService, Arc<Mutex<TestTui>>) {
        let tui = TestTui::new(w, h).expect("failed to create TestTui");
        let concrete = Arc::new(Mutex::new(tui));
        let dyn_shared: Arc<Mutex<dyn tui::TuiLike + Send>> =
            Arc::<Mutex<TestTui>>::clone(&concrete);
        (TuiService::new(dyn_shared), concrete)
    }

    #[tokio::test]
    async fn test_enter_exit_ok() {
        let (svc, _t) = make_service_with_test_tui(80, 24);
        svc.enter().await.expect("enter should succeed");
        svc.exit().await.expect("exit should succeed");
    }

    #[tokio::test]
    async fn test_render_increments_draw_count() {
        let (svc, t) = make_service_with_test_tui(80, 24);
        svc.render(|_f| {}).await.expect("render should succeed");
        svc.render(|_f| {}).await.expect("render should succeed");
        let count = t.lock().await.draw_count();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_run_handles_resize_command() {
        let (svc, _t) = make_service_with_test_tui(80, 24);
        let (tx, rx) = mpsc::unbounded_channel();
        let _h = svc.clone().run(rx);
        tx.send(TuiCommand::Resize {
            width: 100,
            height: 40,
        })
        .expect("send should succeed");
        sleep(Duration::from_millis(10)).await;
        // sanity: render works
        let _ = svc.render(|_f| {}).await;
    }
}
