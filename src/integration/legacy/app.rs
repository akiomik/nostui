use color_eyre::eyre::Result;
use crossterm::event::KeyEvent;
use nostr_sdk::prelude::*;
use ratatui::prelude::Rect;
use tokio::sync::mpsc;

use crate::{
    core::state::AppState,
    domain::nostr::Connection,
    domain::nostr::ConnectionProcess,
    infrastructure::config::Config,
    infrastructure::nostr_service::NostrService,
    infrastructure::tui,
    integration::elm_home_adapter::ElmHomeAdapter,
    integration::elm_integration::ElmRuntime,
    integration::legacy::action::Action,
    integration::legacy::mode::Mode,
    integration::legacy::{
        components::{fps::FpsCounter, home::Home, status_bar::StatusBar},
        Component,
    },
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
        let config = Config::new()?;
        let pubkey = Keys::parse(config.privatekey.as_str())?.public_key();

        // Create home component based on configuration
        let home: Box<dyn Component> = if config.experimental.use_elm_home {
            log::info!("Using experimental Elm Home component");
            Box::new(ElmHomeAdapter::new())
        } else {
            log::info!("Using legacy Home component");
            Box::new(Home::new())
        };

        let fps = FpsCounter::default();
        let status_bar = StatusBar::new(pubkey, None, None, true);
        let mode = Mode::Home;
        Ok(Self {
            tick_rate,
            frame_rate,
            components: vec![home, Box::new(fps), Box::new(status_bar)],
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

        log::info!("Initializing TUI...");
        let mut tui = tui::Tui::new()?
            .tick_rate(self.tick_rate)
            .frame_rate(self.frame_rate);
        log::info!("✅ TUI initialized successfully");
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

        // If using Elm Home, set the runtime in the adapter
        if self.config.experimental.use_elm_home {
            log::info!("Setting ElmRuntime in ElmHomeAdapter");
            if let Some(home_component) = self.components.get_mut(0) {
                log::info!("Found home component, checking if it's ElmHomeAdapter...");
                if home_component.is_elm_home_adapter() {
                    log::info!("Confirmed: component is ElmHomeAdapter");
                    if let Some(elm_adapter) = home_component.as_elm_home_adapter() {
                        elm_adapter.set_runtime(elm_runtime);
                        log::info!("✅ ElmRuntime successfully set in ElmHomeAdapter");
                    } else {
                        log::error!("❌ Failed to cast to ElmHomeAdapter despite is_elm_home_adapter() == true");
                    }
                } else {
                    log::error!("❌ Component is NOT ElmHomeAdapter! Got different component type");
                    // Fallback: set runtime directly
                    log::info!("Falling back to direct ElmRuntime setting");
                    self.elm_runtime = Some(elm_runtime);
                }
            } else {
                log::error!("❌ No home component found at index 0");
            }
        } else {
            log::info!("Setting ElmRuntime directly (legacy mode)");
            self.elm_runtime = Some(elm_runtime);
        }

        loop {
            if let Some(e) = tui.next().await {
                match e {
                    tui::Event::Quit => action_tx.send(Action::Quit)?,
                    tui::Event::Tick => action_tx.send(Action::Tick)?,
                    tui::Event::Render => action_tx.send(Action::Render)?,
                    tui::Event::Resize(x, y) => action_tx.send(Action::Resize(x, y))?,
                    tui::Event::Key(key) => {
                        action_tx.send(Action::Key(key))?;

                        // Check if we're in input mode and should block keyindings
                        let should_block_keybindings = if self.config.experimental.use_elm_home {
                            log::debug!("App.rs: Using elm_home, checking input mode");
                            if let Some(home_component) = self.components.first() {
                                log::debug!("App.rs: Found home component at index 0");
                                if home_component.is_elm_home_adapter() {
                                    log::debug!("App.rs: Component is ElmHomeAdapter");
                                    if let Some(adapter) = home_component.as_elm_home_adapter_ref()
                                    {
                                        log::debug!("App.rs: Successfully got adapter reference");
                                        if let Some(state) = adapter.get_current_state() {
                                            log::debug!(
                                                "App.rs: Got state, show_input = {}",
                                                state.ui.show_input
                                            );
                                            if state.ui.show_input {
                                                log::info!("App.rs: Blocking keybindings - input mode active");
                                                true
                                            } else {
                                                log::debug!("App.rs: Not blocking keybindings - input mode inactive");
                                                false
                                            }
                                        } else {
                                            log::warn!("App.rs: Failed to get state from adapter");
                                            false
                                        }
                                    } else {
                                        log::warn!("App.rs: Failed to get adapter reference");
                                        false
                                    }
                                } else {
                                    log::debug!("App.rs: Component is NOT ElmHomeAdapter");
                                    false
                                }
                            } else {
                                log::warn!("App.rs: No home component found at index 0");
                                false
                            }
                        } else {
                            log::debug!("App.rs: Not using elm_home");
                            false
                        };

                        if !should_block_keybindings {
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
                        } else {
                            // Even in input mode, allow certain critical keybindings
                            if let Some(keymap) = self.config.keybindings.get(&self.mode) {
                                if let Some(action) = keymap.get(&vec![key]) {
                                    match action {
                                        Action::Unselect
                                        | Action::Suspend
                                        | Action::SubmitTextNote => {
                                            log::info!(
                                                "Got critical action in input mode: {action:?}"
                                            );
                                            action_tx.send(action.clone())?;
                                        }
                                        Action::Quit => {
                                            // Allow Quit only for Ctrl+C, not 'q' key in input mode
                                            if key
                                                .modifiers
                                                .contains(crossterm::event::KeyModifiers::CONTROL)
                                                && key.code == crossterm::event::KeyCode::Char('c')
                                            {
                                                log::info!(
                                                    "Got critical action in input mode: {action:?}"
                                                );
                                                action_tx.send(action.clone())?;
                                            } else {
                                                log::debug!("Blocked quit action in input mode (use Ctrl+C instead): {action:?}");
                                            }
                                        }
                                        _ => {
                                            log::debug!("Blocked action in input mode: {action:?}");
                                        }
                                    }
                                }
                            }
                        }
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
                // Reduce event logging
                // log::debug!("Received event: {:?}", event.id);
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
                    Action::ReceiveEvent(ref _event) => {
                        // log::debug!("Got nostr event: {}", event.id);
                    }
                    Action::SendReaction(ref target_event) => {
                        log::info!("App.rs: Received SendReaction action");
                        // When using ElmHome, actions are already processed by ElmHomeAdapter
                        if self.config.experimental.use_elm_home {
                            log::info!("App.rs: Using ElmHome - action already processed by adapter, skipping");
                            // ElmHomeAdapter has already processed this action
                        } else if let Some(ref mut runtime) = self.elm_runtime {
                            log::info!("App.rs: Using ElmRuntime - sending SendReaction message");
                            use crate::core::msg::Msg;
                            runtime.send_msg(Msg::SendReaction(target_event.clone()));
                        } else {
                            log::info!("App.rs: Using legacy system");
                            // Legacy fallback
                            let event =
                                EventBuilder::reaction(target_event, "+").sign_with_keys(&keys)?;
                            // log::debug!("Send reaction: {}", event.id);
                            event_tx.send(event)?;
                            let note1 = target_event.id.to_bech32()?;
                            action_tx.send(Action::SystemMessage(format!("[Liked] {note1}")))?;
                        }
                    }
                    Action::SendRepost(ref target_event) => {
                        log::info!("App.rs: Received SendRepost action");
                        // When using ElmHome, actions are already processed by ElmHomeAdapter
                        if self.config.experimental.use_elm_home {
                            log::info!("App.rs: Using ElmHome - action already processed by adapter, skipping");
                            // ElmHomeAdapter has already processed this action
                        } else if let Some(ref mut runtime) = self.elm_runtime {
                            log::info!("App.rs: Using ElmRuntime - sending SendRepost message");
                            use crate::core::msg::Msg;
                            runtime.send_msg(Msg::SendRepost(target_event.clone()));
                        } else {
                            log::info!("App.rs: Using legacy system");
                            // Legacy fallback
                            let event =
                                EventBuilder::repost(target_event, None).sign_with_keys(&keys)?;
                            // log::debug!("Send repost: {}", event.id);
                            event_tx.send(event)?;
                            let note1 = target_event.id.to_bech32()?;
                            action_tx.send(Action::SystemMessage(format!("[Reposted] {note1}")))?;
                        }
                    }
                    Action::SendTextNote(ref content, ref tags) => {
                        log::info!(
                            "App.rs: Received SendTextNote action - content: '{}', tags: {:?}",
                            content,
                            tags
                        );
                        // When using ElmHome, actions are already processed by ElmHomeAdapter
                        if self.config.experimental.use_elm_home {
                            log::info!("App.rs: Using ElmHome - action already processed by adapter, skipping");
                            // ElmHomeAdapter has already processed this action
                            // No need to process again to avoid duplication
                        } else if let Some(ref mut runtime) = self.elm_runtime {
                            log::info!("App.rs: Using ElmRuntime - sending SubmitNote message");
                            use crate::core::msg::Msg;
                            runtime.send_msg(Msg::SubmitNote);
                        } else {
                            log::info!("App.rs: Using legacy system");
                            // Legacy fallback
                            let event = EventBuilder::text_note(content)
                                .tags(tags.iter().cloned())
                                .sign_with_keys(&keys)?;
                            // log::debug!("Send text note: {}", event.id);
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
