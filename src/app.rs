use color_eyre::eyre::Result;
use crossterm::event::KeyEvent;
use nostr_sdk::prelude::*;
use ratatui::prelude::Rect;
use tokio::sync::mpsc;

use crate::{
    action::Action,
    components::{Component, FpsCounter, Home, StatusBar},
    config::Config,
    elm_integration::ElmRuntime,
    mode::Mode,
    nostr::Connection,
    nostr::ConnectionProcess,
    nostr_service::NostrService,
    state::AppState,
    tui,
};

pub struct App {
    pub config: Config,
    pub tick_rate: f64,
    pub frame_rate: f64,
    pub components: Vec<Box<dyn Component>>,
    pub should_quit: bool,
    pub should_suspend: bool,
    pub mode: Mode,
    pub last_tick_key_events: Vec<KeyEvent>,
    pub elm_runtime: Option<ElmRuntime>,
}

impl App {
    pub fn new(tick_rate: f64, frame_rate: f64) -> Result<Self> {
        let home = Home::new();
        let fps = FpsCounter::default();
        let config = Config::new()?;
        let pubkey = Keys::parse(config.privatekey.as_str())?.public_key();
        let status_bar = StatusBar::new(pubkey, None, None, true);
        let mode = Mode::Home;
        Ok(Self {
            tick_rate,
            frame_rate,
            components: vec![Box::new(home), Box::new(fps), Box::new(status_bar)],
            should_quit: false,
            should_suspend: false,
            config,
            mode,
            last_tick_key_events: Vec::new(),
            elm_runtime: None,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let (action_tx, mut action_rx) = mpsc::unbounded_channel();

        let mut tui = tui::Tui::new()?
            .tick_rate(self.tick_rate)
            .frame_rate(self.frame_rate);
        // tui.mouse(true);
        tui.enter()?;

        for component in self.components.iter_mut() {
            component.register_action_handler(action_tx.clone())?;
        }

        for component in self.components.iter_mut() {
            component.register_config_handler(self.config.clone())?;
        }

        for component in self.components.iter_mut() {
            let size = tui.size()?;
            component.init(Rect::new(0, 0, size.width, size.height))?;
        }

        let keys = Keys::parse(&self.config.privatekey)?;
        let conn = Connection::new(keys.clone(), self.config.relays.clone()).await?;
        let (mut req_rx, event_tx, terminate_tx, conn_wrapper) = ConnectionProcess::new(conn)?;
        conn_wrapper.run();

        // Initialize NostrService for Elm architecture
        let conn_for_service = Connection::new(keys.clone(), self.config.relays.clone()).await?;
        let (_nostr_event_rx, nostr_cmd_tx, nostr_terminate_tx, nostr_service) =
            NostrService::new(conn_for_service, keys.clone(), action_tx.clone())?;
        nostr_service.run();

        // Initialize ElmRuntime with NostrCommand support
        let initial_state = AppState::new(keys.public_key());
        let elm_runtime =
            ElmRuntime::new_with_nostr_executor(initial_state, action_tx.clone(), nostr_cmd_tx);
        self.elm_runtime = Some(elm_runtime);

        loop {
            if let Some(e) = tui.next().await {
                match e {
                    tui::Event::Quit => action_tx.send(Action::Quit)?,
                    tui::Event::Tick => action_tx.send(Action::Tick)?,
                    tui::Event::Render => action_tx.send(Action::Render)?,
                    tui::Event::Resize(x, y) => action_tx.send(Action::Resize(x, y))?,
                    tui::Event::Key(key) => {
                        action_tx.send(Action::Key(key))?;

                        if let Some(keymap) = self.config.keybindings.get(&self.mode) {
                            if let Some(action) = keymap.get(&vec![key]) {
                                log::info!("Got action: {action:?}");
                                action_tx.send(action.clone())?;
                            } else {
                                // If the key was not handled as a single key action,
                                // then consider it for multi-key combinations.
                                self.last_tick_key_events.push(key);

                                // Check for multi-key combinations
                                if let Some(action) = keymap.get(&self.last_tick_key_events) {
                                    log::info!("Got action: {action:?}");
                                    action_tx.send(action.clone())?;
                                }
                            }
                        };
                    }
                    _ => {}
                }
                for component in self.components.iter_mut() {
                    if let Some(action) = component.handle_events(Some(e.clone()))? {
                        action_tx.send(action)?;
                    }
                }
            }

            // Handle legacy Nostr events
            while let Ok(event) = req_rx.try_recv() {
                action_tx.send(Action::ReceiveEvent(event))?;
            }

            // Handle ElmRuntime message processing and command execution
            if let Some(ref mut runtime) = self.elm_runtime {
                if let Err(e) = runtime.run_update_cycle() {
                    log::error!("ElmRuntime error: {}", e);
                    action_tx.send(Action::Error(format!("ElmRuntime error: {}", e)))?;
                }
            }

            while let Ok(action) = action_rx.try_recv() {
                if action != Action::Tick && action != Action::Render {
                    log::debug!("{action:?}");
                }
                match action {
                    Action::Tick => {
                        self.last_tick_key_events.drain(..);
                    }
                    Action::Quit => self.should_quit = true,
                    Action::Suspend => self.should_suspend = true,
                    Action::Resume => self.should_suspend = false,
                    Action::Resize(w, h) => {
                        tui.resize(Rect::new(0, 0, w, h))?;
                        tui.draw(|f| {
                            for component in self.components.iter_mut() {
                                let r = component.draw(f, f.area());
                                if let Err(e) = r {
                                    action_tx
                                        .send(Action::Error(format!("Failed to draw: {:?}", e)))
                                        .unwrap();
                                }
                            }
                        })?;
                    }
                    Action::Render => {
                        tui.draw(|f| {
                            for component in self.components.iter_mut() {
                                let r = component.draw(f, f.area());
                                if let Err(e) = r {
                                    action_tx
                                        .send(Action::Error(format!("Failed to draw: {:?}", e)))
                                        .unwrap();
                                }
                            }
                        })?;
                    }
                    Action::ReceiveEvent(ref event) => {
                        log::info!("Got nostr event: {event:?}");
                    }
                    Action::SendReaction(ref target_event) => {
                        // Route through ElmRuntime if available, otherwise use legacy system
                        if let Some(ref mut runtime) = self.elm_runtime {
                            use crate::msg::Msg;
                            runtime.send_msg(Msg::SendReaction(target_event.clone()));
                        } else {
                            // Legacy fallback
                            let event =
                                EventBuilder::reaction(target_event, "+").sign_with_keys(&keys)?;
                            log::info!("Send reaction: {event:?}");
                            event_tx.send(event)?;
                            let note1 = target_event.id.to_bech32()?;
                            action_tx.send(Action::SystemMessage(format!("[Liked] {note1}")))?;
                        }
                    }
                    Action::SendRepost(ref target_event) => {
                        // Route through ElmRuntime if available, otherwise use legacy system
                        if let Some(ref mut runtime) = self.elm_runtime {
                            use crate::msg::Msg;
                            runtime.send_msg(Msg::SendRepost(target_event.clone()));
                        } else {
                            // Legacy fallback
                            let event =
                                EventBuilder::repost(target_event, None).sign_with_keys(&keys)?;
                            log::info!("Send repost: {event:?}");
                            event_tx.send(event)?;
                            let note1 = target_event.id.to_bech32()?;
                            action_tx.send(Action::SystemMessage(format!("[Reposted] {note1}")))?;
                        }
                    }
                    Action::SendTextNote(ref content, ref tags) => {
                        // Route through ElmRuntime if available, otherwise use legacy system
                        if let Some(ref mut runtime) = self.elm_runtime {
                            use crate::msg::Msg;
                            runtime.send_msg(Msg::SubmitNote);
                        } else {
                            // Legacy fallback
                            let event = EventBuilder::text_note(content)
                                .tags(tags.iter().cloned())
                                .sign_with_keys(&keys)?;
                            log::info!("Send text note: {event:?}");
                            event_tx.send(event)?;
                            action_tx.send(Action::SystemMessage(format!("[Posted] {content}")))?;
                        }
                    }
                    _ => {}
                }
                for component in self.components.iter_mut() {
                    if let Some(action) = component.update(action.clone())? {
                        action_tx.send(action)?
                    };
                }
            }
            if self.should_suspend {
                tui.suspend()?;
                action_tx.send(Action::Resume)?;
                tui = tui::Tui::new()?
                    .tick_rate(self.tick_rate)
                    .frame_rate(self.frame_rate);
                // tui.mouse(true);
                tui.enter()?;
            } else if self.should_quit {
                terminate_tx.send(())?;
                let _ = nostr_terminate_tx.send(()); // Terminate NostrService
                tui.stop()?;
                break;
            }
        }
        tui.exit()?;
        Ok(())
    }
}
