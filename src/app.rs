use std::sync::Arc;

use color_eyre::eyre::Result;
use crossterm::event::KeyEvent;
use nostr_sdk::database::memory::MemoryDatabase;
use nostr_sdk::database::DatabaseOptions;
use nostr_sdk::prelude::*;
use ratatui::prelude::Rect;
use tokio::sync::{mpsc, Mutex};

use crate::{
    action::Action,
    components::{Component, FpsCounter, Home, StatusBar},
    config::Config,
    mode::Mode,
    nostr::{Connection, ConnectionAction, NostrActionHandler},
    repositories::EventRepository,
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
}

impl App {
    pub fn new(tick_rate: f64, frame_rate: f64) -> Result<Self> {
        let home = Home::new();
        let fps = FpsCounter::default();
        let config = Config::new()?;
        let pubkey = Keys::from_sk_str(config.privatekey.as_str())?.public_key();
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
        })
    }

    async fn build_nostr_client(&self, keys: &Keys) -> Result<Client> {
        let client = ClientBuilder::new().signer(keys).build();
        client.add_relays(self.config.relays.clone()).await?;
        Ok(client)
    }

    fn build_cache(&self) -> Arc<Mutex<MemoryDatabase>> {
        let mut opts = DatabaseOptions::new();
        opts.events = true;
        Arc::new(Mutex::new(MemoryDatabase::new(opts)))
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
            component.init(tui.size()?)?;
        }

        let cache = self.build_cache();
        let keys = Keys::from_sk_str(&self.config.privatekey.clone())?;
        let client = self.build_nostr_client(&keys).await?;
        let conn = Connection::new(client, cache.clone());
        let (connection_tx, mut event_rx) = conn.run();
        let event_repository = EventRepository::new(keys, cache, connection_tx.clone());
        let nostr_action_handler = NostrActionHandler::new(event_repository, action_tx.clone());

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

            while let Ok(event) = event_rx.try_recv() {
                action_tx.send(Action::ReceiveNostrEvent(event))?;
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
                                let r = component.draw(f, f.size());
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
                                let r = component.draw(f, f.size());
                                if let Err(e) = r {
                                    action_tx
                                        .send(Action::Error(format!("Failed to draw: {:?}", e)))
                                        .unwrap();
                                }
                            }
                        })?;
                    }
                    Action::ReceiveNostrEvent(ref event) => {
                        log::info!("Got nostr event: {event:?}");
                    }
                    Action::SendNostrAction(ref action) => {
                        nostr_action_handler.handle(action.clone())?;
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
                connection_tx.send(ConnectionAction::Shutdown)?;
                tui.stop()?;
                break;
            }
        }
        tui.exit()?;
        Ok(())
    }
}
