use color_eyre::eyre::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    core::state::AppState,
    infrastructure::tui,
    presentation::components::{
        elm_fps::ElmFpsCounter, elm_home::ElmHome, elm_status_bar::ElmStatusBar,
    },
};

#[derive(Debug, Default)]
pub struct Renderer<'a> {
    home: ElmHome<'a>,
    status_bar: ElmStatusBar,
    fps: ElmFpsCounter,
}

impl<'a> Renderer<'a> {
    pub fn new() -> Self {
        Self {
            home: ElmHome::new(),
            status_bar: ElmStatusBar::new(),
            fps: ElmFpsCounter::new(),
        }
    }

    pub async fn render(
        &mut self,
        tui: &Arc<Mutex<dyn tui::TuiLike + Send>>,
        state: &AppState,
    ) -> Result<()> {
        let mut guard = tui.lock().await;
        let mut draw = |f: &mut ratatui::Frame<'_>| {
            let area = f.area();
            self.home.render(f, area, state);
            let _ = self.status_bar.draw(state, f, area);
            let _ = self.fps.draw(state, f, area);
        };
        guard.draw(&mut draw)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::test_tui::TestTui;

    #[tokio::test]
    async fn renderer_renders_with_test_tui() {
        let tui: Arc<Mutex<dyn tui::TuiLike + Send>> = Arc::new(Mutex::new(
            TestTui::new(80, 24).expect("failed to create TestTui"),
        ));
        let mut r: Renderer<'_> = Renderer::new();
        // minimal state
        use nostr_sdk::prelude::*;
        let keys = Keys::generate();
        let cfg = crate::infrastructure::config::Config {
            privatekey: keys.secret_key().to_bech32().unwrap(),
            relays: vec!["wss://example.com".into()],
            ..Default::default()
        };
        let state = AppState::new_with_config(keys.public_key(), cfg);
        r.render(&tui, &state).await.expect("render should succeed");
    }
}
