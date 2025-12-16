use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;
use tokio::sync::mpsc;

use crate::{
    core::{raw_msg::RawMsg, state::AppState},
    infrastructure::{config::Config, fps_service::FpsService, nostr_service::NostrService, tui},
    integration::elm_integration::ElmRuntime,
    presentation::components::{
        elm_fps::ElmFpsCounter, elm_home::ElmHome, elm_status_bar::ElmStatusBar,
    },
};

/// Experimental runner that drives the Elm architecture directly without legacy App
/// This is introduced alongside the legacy runner and is not yet wired to main().
pub struct AppRunner<'a> {
    /* lifetime used by ElmHome */
    headless: bool,
    config: Config,
    tick_rate: f64,
    frame_rate: f64,
    runtime: ElmRuntime,
    render_req_rx: mpsc::UnboundedReceiver<()>, 
    // NOTE: In tests or non-interactive environments, TUI can be absent.
    // TODO: Prefer injecting a concrete TUI implementation (e.g., real or test backend)
    // rather than using Option. This avoids conditional logic in the runner and
    // makes dependencies explicit at the composition root.
    tui: Option<std::sync::Arc<tokio::sync::Mutex<tui::Tui>>>,
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
}

impl<'a> AppRunner<'a> {
    pub fn runtime(&self) -> &ElmRuntime {
        &self.runtime
    }
    pub fn runtime_mut(&mut self) -> &mut ElmRuntime {
        &mut self.runtime
    }
    /// Create a new AppRunner with ElmRuntime and infrastructure initialized.
    pub async fn new_with_config(
        config: Config,
        tick_rate: f64,
        frame_rate: f64,
        headless: bool,
    ) -> Result<Self> {
        let keys = Keys::parse(&config.privatekey)?;

        // Initialize ElmRuntime with Nostr support
        let initial_state = AppState::new_with_config(keys.public_key(), config.clone());
        // Legacy action channel removed

        // Create runtime (without Nostr support yet) to obtain raw_tx for NostrService
        let mut runtime = ElmRuntime::new_with_executor(initial_state, /* action_tx removed */ mpsc::unbounded_channel().0 );
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

        // Initialize TUI only when interactive
        let tui = if headless {
            None
        } else {
            Some(std::sync::Arc::new(tokio::sync::Mutex::new(
                tui::Tui::new()?.tick_rate(tick_rate).frame_rate(frame_rate),
            )))
        };
        // Wire TuiService with channel (Nostr-like pattern)
        let (tui_cmd_tx, tui_cmd_rx, tui_service) =
            crate::infrastructure::tui_service::TuiService::new_with_channel(tui.clone());
        // Start TuiService background loop
        let _tui_handle = tui_service.clone().run(tui_cmd_rx);
        // Route TUI commands from CmdExecutor
        let _ = runtime.add_tui_sender(tui_cmd_tx);

        Ok(Self {
            headless,
            config,
            tick_rate,
            frame_rate,
            runtime,
            render_req_rx,
            tui,
            // Keep service for future direct Cmd::Tui execution
            // (currently CmdExecutor falls back to Action until wiring is complete)
            home: ElmHome::new(),
            status_bar: ElmStatusBar::new(),
            fps: ElmFpsCounter::new(),
            nostr_terminate_tx,
            nostr_event_rx,
            fps_service,
        })
    }

    /// Run the main loop: handle TUI events, Nostr events, update Elm state and render.
    pub async fn run(&mut self) -> Result<()> {
        if !self.headless {
            if let Some(tui) = &mut self.tui {
                let mut guard = tui.lock().await;
                guard.enter()?;
            }
        }

        loop {
            // Coalesce render requests (at most one render per loop)
            let mut should_render = false;
            while let Ok(()) = self.render_req_rx.try_recv() {
                should_render = true;
            }

            // Drain Nostr events first to keep timeline responsive
            while let Ok(ev) = self.nostr_event_rx.try_recv() {
                self.runtime.send_raw_msg(RawMsg::ReceiveEvent(ev));
            }

            if !self.headless {
                if let Some(tui) = &mut self.tui {
                    let e_opt = {
                        let mut guard = tui.lock().await;
                        guard.next().await
                    };
                    if let Some(e) = e_opt {
                        match e {
                            tui::Event::Quit => {
                                self.runtime.send_raw_msg(RawMsg::Quit);
                            }
                            tui::Event::Tick => {
                                self.runtime.send_raw_msg(RawMsg::Tick);
                                // Count app tick for FPS based on TUI tick cadence
                                self.fps_service.on_app_tick();
                            }
                            tui::Event::Render => {
                                // Coalesce render request; actual render happens once per loop
                                should_render = true;
                            }
                            tui::Event::Resize(w, h) => {
                                self.runtime.send_raw_msg(RawMsg::Resize(w, h));
                            }
                            tui::Event::Key(key) => {
                                self.runtime.send_raw_msg(RawMsg::Key(key));
                            }
                            tui::Event::FocusGained => {}
                            tui::Event::FocusLost => {}
                            tui::Event::Paste(s) => {
                                // Paste not yet supported in Elm translator; can be forwarded via RawMsg::Error for now
                                let _ = s; // suppress unused warning
                            }
                            tui::Event::Mouse(_m) => {}
                            tui::Event::Init => {}
                            tui::Event::Error => {}
                            tui::Event::Closed => {}
                        }
                    }
                }
            }

            if self.headless {
                // In headless mode, yield briefly to avoid a busy loop
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }

            // Process Elm update cycle and execute commands
            if let Err(e) = self.runtime.run_update_cycle() {
                log::error!("ElmRuntime error: {}", e);
                // Fall back to showing error via RawMsg to avoid tight loop
                self.runtime
                    .send_raw_msg(RawMsg::Error(format!("ElmRuntime error: {}", e)));
            }


            // Execute coalesced render if requested
            if should_render && !self.headless {
                self.render().await?;
                self.fps_service.on_render();
            }

            // Check quit condition from Elm state
            if self.runtime.state().system.should_quit {
                break;
            }
        }

        // Shutdown services and exit TUI
        let _ = self.nostr_terminate_tx.send(());
        if !self.headless {
            if let Some(tui) = &mut self.tui {
                let mut guard = tui.lock().await;
                guard.exit()?;
            }
        }
        Ok(())
    }

    async fn render(&mut self) -> Result<()> {
        let state = self.runtime.state().clone();
        if let Some(tui) = &mut self.tui {
            let mut guard = tui.lock().await;
            guard.draw(|f| {
                let area = f.area();
                // Home timeline and input overlay
                self.home.render(f, area, &state);
                // Status bar overlays bottom lines
                let _ = self.status_bar.draw(&state, f, area);
                // FPS indicator (top line overlay)
                let _ = self.fps.draw(&state, f, area);
            })?;
        }
        Ok(())
    }
}
