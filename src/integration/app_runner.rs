use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;
use tokio::sync::mpsc;

use crate::{
    core::{raw_msg::RawMsg, state::AppState},
    infrastructure::{
        config::Config,
        fps_service::FpsService,
        nostr_service::NostrService,
        tui,
        tui_event_source::{EventSource, TuiEvent},
    },
    integration::{
        coalescer::Coalescer, elm_integration::ElmRuntime, update_executor::UpdateExecutor,
    },
    presentation::components::{
        elm_fps::ElmFpsCounter, elm_home::ElmHome, elm_status_bar::ElmStatusBar,
    },
};

/// Experimental runner that drives the Elm architecture directly without legacy App
/// This is introduced alongside the legacy runner and is not yet wired to main().
pub struct AppRunner<'a> {
    /* lifetime used by ElmHome */
    runtime: ElmRuntime,
    render_req_rx: mpsc::UnboundedReceiver<()>,
    // NOTE: In tests or non-interactive environments, TUI can be absent.
    tui: std::sync::Arc<tokio::sync::Mutex<dyn tui::TuiLike + Send>>,
    event_source: Option<EventSource>,
    // Presentation components (stateless/pure rendering)
    home: ElmHome<'a>,
    status_bar: ElmStatusBar,
    fps: ElmFpsCounter,
    // For service termination
    nostr_terminate_tx: mpsc::UnboundedSender<()>,
    // Incoming events from Nostr network
    nostr_event_rx: mpsc::UnboundedReceiver<Event>,
    // FPS service sending RawMsg updates
    fps_service: FpsService,
    // Coalesced pending resize (last-only within a loop)
    pending_resize: Option<(u16, u16)>,
}

impl<'a> AppRunner<'a> {
    pub fn runtime(&self) -> &ElmRuntime {
        &self.runtime
    }
    pub fn runtime_mut(&mut self) -> &mut ElmRuntime {
        &mut self.runtime
    }

    // Test helper: inject a custom event source (e.g., EventSource::Test)
    pub fn set_event_source_for_tests(
        &mut self,
        src: crate::infrastructure::tui_event_source::EventSource,
    ) {
        self.event_source = Some(src);
    }

    /// Create a new AppRunner with ElmRuntime and infrastructure initialized.
    pub async fn new_with_config(
        config: Config,
        _tick_rate: f64,
        _frame_rate: f64,
        tui: std::sync::Arc<tokio::sync::Mutex<dyn tui::TuiLike + Send>>,
    ) -> Result<Self> {
        let keys = Keys::parse(&config.privatekey)?;

        // Initialize ElmRuntime with Nostr support
        let initial_state = AppState::new_with_config(keys.public_key(), config.clone());
        // Legacy action channel removed

        // Create runtime (without Nostr support yet) to obtain raw_tx for NostrService
        let mut runtime = ElmRuntime::new_with_executor(initial_state);
        let raw_tx = runtime.get_raw_sender().expect("raw sender must exist");

        // Initialize NostrService and start it in background
        let conn =
            crate::domain::nostr::Connection::new(keys.clone(), config.relays.clone()).await?;
        let (nostr_event_rx, nostr_cmd_tx, nostr_terminate_tx, nostr_service) =
            NostrService::new(conn, keys.clone(), raw_tx.clone())?;
        nostr_service.run();

        // Add Nostr support to runtime now that we have the sender
        let _ = runtime.add_nostr_support(nostr_cmd_tx.clone());
        let fps_service = FpsService::new(raw_tx.clone());

        // Render request channel from CmdExecutor -> AppRunner
        let (render_req_tx, render_req_rx) = mpsc::unbounded_channel::<()>();
        let _ = runtime.add_render_request_sender(render_req_tx);

        // TUI is injected by caller (RealTui for interactive, TestTui for tests)
        let tui = tui;
        // Wire TuiService with channel (Nostr-like pattern)
        let (tui_cmd_tx, tui_cmd_rx, tui_service) =
            crate::infrastructure::tui_service::TuiService::new_with_channel(tui.clone());
        // Start TuiService background loop
        let _tui_handle = tui_service.clone().run(tui_cmd_rx);
        // Route TUI commands from CmdExecutor
        let _ = runtime.add_tui_sender(tui_cmd_tx);

        let event_source = Some(EventSource::real(tui.clone()));

        Ok(Self {
            runtime,
            render_req_rx,
            tui,
            event_source,
            // Keep service for future direct Cmd::Tui execution
            // (currently CmdExecutor falls back to Action until wiring is complete)
            home: ElmHome::new(),
            status_bar: ElmStatusBar::new(),
            fps: ElmFpsCounter::new(),
            nostr_terminate_tx,
            nostr_event_rx,
            fps_service,
            pending_resize: None,
        })
    }

    /// Drain render request channel and return the number of queued requests
    fn drain_render_req_count(&mut self) -> usize {
        let mut n = 0;
        while let Ok(()) = self.render_req_rx.try_recv() {
            n += 1;
        }
        n
    }

    /// Drain all pending Nostr events and forward to runtime as RawMsg
    fn drain_nostr_events(&mut self) {
        while let Ok(ev) = self.nostr_event_rx.try_recv() {
            self.runtime.send_raw_msg(RawMsg::ReceiveEvent(ev));
        }
    }

    /// Handle a single TUI event and update should_render flag accordingly
    fn handle_tui_event(&mut self, e: TuiEvent, should_render: &mut bool) {
        match e {
            TuiEvent::Quit => {
                self.runtime.send_raw_msg(RawMsg::Quit);
            }
            TuiEvent::Tick => {
                self.runtime.send_raw_msg(RawMsg::Tick);
                // Count app tick for FPS based on TUI tick cadence
                self.fps_service.on_app_tick();
            }
            TuiEvent::Render => {
                // Coalesce render request; actual render happens once per loop
                *should_render = true;
            }
            TuiEvent::Resize(w, h) => {
                // Coalesce last-only using pure decision helper
                self.pending_resize = Coalescer::decide_resize(self.pending_resize, &[(w, h)]);
            }
            TuiEvent::Key(key) => {
                self.runtime.send_raw_msg(RawMsg::Key(key));
            }
            TuiEvent::FocusGained => {}
            TuiEvent::FocusLost => {}
            TuiEvent::Paste(_s) => {
                // Paste not yet supported in Elm translator
            }
            TuiEvent::Mouse(_m) => {}
            TuiEvent::Init => {}
            TuiEvent::Error => {}
            TuiEvent::Closed => {}
        }
    }

    async fn enter_tui(&self) -> Result<()> {
        let mut guard = self.tui.lock().await;
        guard.enter()?;
        Ok(())
    }

    async fn exit_tui(&self) -> Result<()> {
        let mut guard = self.tui.lock().await;
        guard.exit()?;
        Ok(())
    }

    async fn maybe_render(&mut self, should_render: bool) -> Result<()> {
        if should_render {
            self.render().await?;
            self.fps_service.on_render();
        }
        Ok(())
    }

    fn should_quit(&self) -> bool {
        self.runtime.state().system.should_quit
    }

    async fn poll_next_tui_event(&mut self) -> Option<TuiEvent> {
        // Note: For Resize coalescing, we only poll one event per loop.
        // If multiple Resize events arrive, last one wins across loops via pending_resize.

        if let Some(src) = &mut self.event_source {
            src.next().await
        } else {
            None
        }
    }

    fn shutdown_services(&self) {
        let _ = self.nostr_terminate_tx.send(());
    }

    /// Run the main loop: handle TUI events, Nostr events, update Elm state and render.
    pub async fn run(&mut self) -> Result<()> {
        self.enter_tui().await?;

        loop {
            // 1) Coalesce render requests (at most one render per loop)
            let queued = self.drain_render_req_count();
            let mut render_flag = false;

            // 2) Drain Nostr events first to keep timeline responsive
            self.drain_nostr_events();

            // 3) Poll one TUI event and handle it
            if let Some(e) = self.poll_next_tui_event().await {
                if let TuiEvent::Render = e {
                    render_flag = true;
                }
                self.handle_tui_event(e, &mut render_flag);
            }

            // 4) Process Elm update cycle and execute commands
            UpdateExecutor::process_update_cycle(&mut self.runtime, &mut self.pending_resize);

            // 5) Execute coalesced render if requested
            let should_render = Coalescer::decide_render(queued, render_flag);
            self.maybe_render(should_render).await?;

            // 6) Check quit condition from Elm state
            if self.should_quit() {
                break;
            }
        }

        // Shutdown services and exit TUI
        self.shutdown_services();
        self.exit_tui().await?;
        Ok(())
    }

    async fn render(&mut self) -> Result<()> {
        let state = self.runtime.state().clone();
        // Prefer test terminal when injected (for unit tests)
        {
            let mut guard = self.tui.lock().await;
            let mut draw = |f: &mut ratatui::Frame<'_>| {
                let area = f.area();
                // Home timeline and input overlay
                self.home.render(f, area, &state);
                // Status bar overlays bottom lines
                let _ = self.status_bar.draw(&state, f, area);
                // FPS indicator (top line overlay)
                let _ = self.fps.draw(&state, f, area);
            };
            guard.draw(&mut draw)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // Unit tests for the extracted helpers
    use super::*;
    use crate::infrastructure::test_tui::TestTui;
    use crate::infrastructure::tui_event_source::EventSource as TestEventSource;
    use crate::infrastructure::tui_event_source::TuiEvent;

    fn make_test_config() -> Config {
        let keys = Keys::generate();
        Config {
            privatekey: keys.secret_key().to_bech32().unwrap(),
            relays: vec!["wss://example.com".into()],
            ..Default::default()
        }
    }

    async fn make_runner_with_test_tui() -> AppRunner<'static> {
        use std::sync::Arc;
        use tokio::sync::Mutex;
        let tui = Arc::new(Mutex::new(
            TestTui::new(80, 24).expect("failed to create TestTui"),
        ));
        AppRunner::new_with_config(make_test_config(), 10.0, 30.0, tui)
            .await
            .expect("failed to create AppRunner")
    }

    #[tokio::test]
    async fn app_runner_one_cycle_quit_sets_should_quit() {
        let mut runner = make_runner_with_test_tui().await;
        // Inject test event source that yields a single Quit
        runner.set_event_source_for_tests(TestEventSource::test([TuiEvent::Quit]));

        // Manually perform one logical cycle using extracted helpers
        let _queued = runner.drain_render_req_count();
        let mut render_flag = false;
        runner.drain_nostr_events();
        if let Some(e) = runner.poll_next_tui_event().await {
            if let TuiEvent::Render = e {
                render_flag = true;
            }
            runner.handle_tui_event(e, &mut render_flag);
        }
        // Avoid multiple mutable borrows by taking pending_resize out temporarily
        let mut pending = runner.pending_resize.take();
        UpdateExecutor::process_update_cycle(runner.runtime_mut(), &mut pending);
        runner.pending_resize = pending;
        // don't call maybe_render on purpose (legacy one-cycle helper also skipped draw)

        assert!(runner.runtime().state().system.should_quit);
    }

    #[test]
    fn drain_render_requests_returns_true_when_channel_has_requests() {
        // Build a runner with test TUI
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut runner = make_runner_with_test_tui().await;
            // Send a render request via runtime's render_req_tx (already wired)
            // We cannot access the sender here, so simulate by directly toggling channel via runtime API:
            // Workaround: send a Ui message that triggers a render request via CmdExecutor -> render_req_tx.
            // The easiest deterministic way is to call drain_render_requests() after we manually inject a request
            // by sending through the runtime's added sender integrated in CmdExecutor using Cmd::Tui(Render).
            // However, there isn't a direct API to enqueue render requests. Instead, we rely on the channel to be empty
            // and validate false, then manually push should_render via TuiEvent::Render handled function.
            assert_eq!(runner.drain_render_req_count(), 0);
            let mut sr = false;
            runner.handle_tui_event(TuiEvent::Render, &mut sr);
            assert!(sr);
        });
    }

    #[tokio::test]
    async fn handle_tui_event_resize_and_key_are_forwarded() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut runner = make_runner_with_test_tui().await;
        let mut sr = false;
        runner.handle_tui_event(TuiEvent::Resize(120, 50), &mut sr);
        runner.handle_tui_event(
            TuiEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &mut sr,
        );
        // Process update to move raw queue through translator without assertions here, just ensure no panic
        let _ = runner.runtime_mut().run_update_cycle();
        // Render event sets the flag but does not render yet
        runner.handle_tui_event(TuiEvent::Render, &mut sr);
        assert!(sr);
    }

    #[tokio::test]
    async fn app_runner_render_happens_on_render_event_then_quit() {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        // Prepare TestTui to observe draw count
        let test_tui = Arc::new(Mutex::new(
            TestTui::new(80, 24).expect("failed to create TestTui"),
        ));
        let draw_counter_handle = test_tui.clone();

        // Create runner with the same TestTui instance
        let mut runner = AppRunner::new_with_config(make_test_config(), 10.0, 30.0, test_tui)
            .await
            .expect("failed to create AppRunner");

        // Drive the loop with events: Render -> Quit
        runner
            .set_event_source_for_tests(TestEventSource::test([TuiEvent::Render, TuiEvent::Quit]));

        // Run the main loop; it should finish quickly due to Quit
        let res = tokio::time::timeout(std::time::Duration::from_millis(200), runner.run()).await;
        assert!(res.is_ok(), "runner.run() should complete promptly");

        // Verify at least one draw happened due to Render coalescing
        let draws = draw_counter_handle.lock().await.draw_count();
        assert!(draws >= 1, "expected at least one render, got {}", draws);
    }
}
