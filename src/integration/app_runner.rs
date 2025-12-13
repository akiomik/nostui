use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;
use tokio::sync::mpsc;

use crate::{
    core::{raw_msg::RawMsg, state::AppState},
    infrastructure::{config::Config, nostr_service::NostrService, tui},
    integration::elm_integration::ElmRuntime,
    integration::legacy::action::Action,
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
    action_rx: mpsc::UnboundedReceiver<Action>,
    // NOTE: In tests or non-interactive environments, TUI can be absent.
    // TODO: Prefer injecting a concrete TUI implementation (e.g., real or test backend)
    // rather than using Option. This avoids conditional logic in the runner and
    // makes dependencies explicit at the composition root.
    tui: Option<tui::Tui>,
    // Presentation components (stateless/pure rendering)
    home: ElmHome<'a>,
    status_bar: ElmStatusBar,
    fps: ElmFpsCounter,
    // For service termination
    nostr_terminate_tx: mpsc::UnboundedSender<()>,
    // Incoming events from Nostr network
    nostr_event_rx: mpsc::UnboundedReceiver<Event>,
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
        let (action_tx, action_rx) = mpsc::unbounded_channel::<Action>();

        // Initialize NostrService and start it in background
        let conn =
            crate::domain::nostr::Connection::new(keys.clone(), config.relays.clone()).await?;
        let (nostr_event_rx, nostr_cmd_tx, nostr_terminate_tx, nostr_service) =
            NostrService::new(conn, keys.clone(), action_tx.clone())?;
        nostr_service.run();

        let runtime = ElmRuntime::new_with_nostr_executor(initial_state, action_tx, nostr_cmd_tx);

        // Initialize TUI only when interactive
        let tui = if headless {
            None
        } else {
            Some(tui::Tui::new()?.tick_rate(tick_rate).frame_rate(frame_rate))
        };

        Ok(Self {
            headless,
            config,
            tick_rate,
            frame_rate,
            runtime,
            action_rx,
            tui,
            home: ElmHome::new(),
            status_bar: ElmStatusBar::new(),
            fps: ElmFpsCounter::new(),
            nostr_terminate_tx,
            nostr_event_rx,
        })
    }

    /// Run the main loop: handle TUI events, Nostr events, update Elm state and render.
    pub async fn run(&mut self) -> Result<()> {
        if !self.headless {
            if let Some(tui) = &mut self.tui {
                tui.enter()?;
            }
        }

        loop {
            // Drain Nostr events first to keep timeline responsive
            while let Ok(ev) = self.nostr_event_rx.try_recv() {
                self.runtime.send_raw_msg(RawMsg::ReceiveEvent(ev));
            }

            if !self.headless {
                if let Some(tui) = &mut self.tui {
                    if let Some(e) = tui.next().await {
                        match e {
                            tui::Event::Quit => {
                                self.runtime.send_raw_msg(RawMsg::Quit);
                            }
                            tui::Event::Tick => {
                                self.runtime.send_raw_msg(RawMsg::Tick);
                            }
                            tui::Event::Render => {
                                // Rendering will be handled below explicitly
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

            // Handle actions that require immediate host reaction (resize/render)
            while let Ok(action) = self.action_rx.try_recv() {
                match action {
                    Action::Resize(w, h) => {
                        if !self.headless {
                            if let Some(tui) = &mut self.tui {
                                tui.resize(ratatui::prelude::Rect::new(0, 0, w, h))?;
                                self.render()?;
                            }
                        }
                    }
                    Action::Render => {
                        if !self.headless {
                            self.render()?;
                        }
                    }
                    Action::Quit => {
                        // Also allow quitting via Action channel if sent
                        self.runtime.send_raw_msg(RawMsg::Quit);
                    }
                    _ => {
                        // Other actions are either handled inside Elm (translated to messages)
                        // or side effects already executed by CmdExecutor/NostrService.
                    }
                }
            }

            // Render at least once per loop (on high FPS this is cheap)
            if !self.headless {
                self.render()?;
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
                tui.exit()?;
            }
        }
        Ok(())
    }

    fn render(&mut self) -> Result<()> {
        let state = self.runtime.state().clone();
        if let Some(tui) = &mut self.tui {
            tui.draw(|f| {
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
