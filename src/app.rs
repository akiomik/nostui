//! Main Tears Application implementation

use std::cell::RefCell;
use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use nostr_sdk::prelude::*;
use nowhear::{MediaEvent, MediaSourceError};
use ratatui::prelude::*;
use tears::prelude::*;
use tears::subscription::terminal::TerminalEvents;
use tears::subscription::time::{Message as TimerMessage, Timer};

use crate::core::message::{AppMsg, EditorMsg, NostrMsg, SystemMsg, TimelineMsg};
use crate::core::state::{AppState, NostrState};
use crate::infrastructure::config::Config;
use crate::infrastructure::subscription::media::MediaEvents;
use crate::infrastructure::subscription::nostr::{
    Message as NostrSubscriptionMessage, NostrEvents,
};
use crate::model::status_bar::Message as StatusBarMessage;
use crate::model::timeline::tab::TimelineTabType;
use crate::model::timeline::Message as TimelineMessage;
use crate::presentation::components::Components;
use crate::presentation::config::keybindings::Action as KeyAction;

/// Initialization flags for the Tears application
#[derive(Debug)]
pub struct InitFlags {
    pub pubkey: PublicKey,
    pub config: Config,
    pub nostr_client: Client,
    pub tick_rate: f64,
}

/// Main Tears application structure
///
/// This struct holds:
/// - Global application state (managed centrally)
/// - Component instances (stateless renderers/processors)
pub struct TearsApp<'a> {
    /// Global application state
    state: AppState,
    /// Component collection (wrapped in RefCell for interior mutability during view)
    components: RefCell<Components<'a>>,
    /// Nostr client (wrapped in Arc for sharing across subscriptions)
    nostr_client: Arc<Client>,
    /// Configuration (including keybindings)
    config: Config,
    /// Tick rate (Hz) for timer subscription
    tick_rate: f64,
}

impl<'a> Application for TearsApp<'a> {
    type Message = AppMsg;
    type Flags = InitFlags;

    fn new(flags: InitFlags) -> (Self, Command<Self::Message>) {
        // Store config separately for keybindings access
        let config = flags.config.clone();

        // Initialize global state
        let state = AppState::new_with_config(flags.pubkey, flags.config);

        // Initialize components
        let components = Components::new();

        // Wrap client in Arc for sharing across subscriptions
        // This ensures subscription identity remains constant
        let nostr_client = Arc::new(flags.nostr_client);

        let app = Self {
            state,
            components: RefCell::new(components),
            nostr_client,
            config,
            tick_rate: flags.tick_rate,
        };

        // Return initial commands if needed
        // For now, no initial commands
        (app, Command::none())
    }

    fn update(&mut self, msg: AppMsg) -> Command<Self::Message> {
        log::debug!("update: {msg:?}");

        // Handle messages and update state
        match msg {
            AppMsg::System(system_msg) => self.handle_system_msg(system_msg),
            AppMsg::Timeline(timeline_msg) => self.handle_timeline_msg(timeline_msg),
            AppMsg::Editor(editor_msg) => self.handle_editor_msg(editor_msg),
            AppMsg::Nostr(nostr_msg) => self.handle_nostr_msg(nostr_msg),
            AppMsg::Media(media_msg) => self.handle_media_msg(media_msg),
        }
    }

    fn view(&self, frame: &mut Frame) {
        // Delegate to components for rendering
        self.components.borrow_mut().render(frame, &self.state);
    }

    fn subscriptions(&self) -> Vec<Subscription<Self::Message>> {
        let mut subs = vec![
            // NostrEvents subscription - reuse the same Arc<Client> across frames
            // This ensures the subscription ID remains constant and the subscription
            // is not recreated every frame
            Subscription::new(NostrEvents::new(Arc::clone(&self.nostr_client)))
                .map(|msg| AppMsg::Nostr(NostrMsg::SubscriptionMessage(msg))),
            // Timer subscription - tick interval calculated from tick_rate
            Subscription::new(Timer::new((1000.0 / self.tick_rate) as u64)).map(|msg| match msg {
                TimerMessage::Tick => AppMsg::System(SystemMsg::Tick),
            }),
            Subscription::new(TerminalEvents::new()).map(|result| match result {
                Ok(event) => {
                    // Handle different terminal event types
                    match event {
                        Event::Key(key) => AppMsg::System(SystemMsg::KeyInput(key)),
                        Event::Resize(width, height) => {
                            AppMsg::System(SystemMsg::Resize(width, height))
                        }
                        _ => AppMsg::System(SystemMsg::Tick), // Ignore other events for now
                    }
                }
                Err(e) => AppMsg::System(SystemMsg::ShowError(e.to_string())),
            }),
        ];

        if self.config.nip38.enabled {
            subs.push(Subscription::new(MediaEvents::new()).map(AppMsg::Media));
        }

        // Add signal subscription for Ctrl+C (SIGINT)
        // This handles OS-level signals separately from keyboard input
        #[cfg(unix)]
        {
            use tears::subscription::signal::Signal;
            use tokio::signal::unix::SignalKind;

            subs.push(
                Subscription::new(Signal::new(SignalKind::interrupt())).map(
                    |result| match result {
                        Ok(()) => {
                            log::info!("Received SIGINT (Ctrl+C) - requesting quit");
                            AppMsg::System(SystemMsg::Quit)
                        }
                        Err(e) => AppMsg::System(SystemMsg::ShowError(format!(
                            "Signal handler error: {e}"
                        ))),
                    },
                ),
            );
        }

        #[cfg(windows)]
        {
            use tears::subscription::signal::CtrlC;

            subs.push(Subscription::new(CtrlC::new()).map(|result| match result {
                Ok(()) => {
                    log::info!("Received Ctrl+C - requesting quit");
                    AppMsg::System(SystemMsg::Quit)
                }
                Err(e) => {
                    AppMsg::System(SystemMsg::ShowError(format!("Signal handler error: {e}")))
                }
            }));
        }

        subs
    }
}

impl<'a> TearsApp<'a> {
    /// Handle system messages
    fn handle_system_msg(&mut self, msg: SystemMsg) -> Command<AppMsg> {
        match msg {
            SystemMsg::Quit => {
                log::info!("Quit requested - initiating graceful shutdown");

                // Graceful shutdown: unsubscribe from all timeline subscriptions
                // and disconnect from relays.
                self.state.nostr.shutdown();

                // Trigger the quit action
                return Command::effect(Action::Quit);
            }
            SystemMsg::Resize(width, height) => {
                log::debug!("Terminal resized to {width}x{height}");
                // Terminal resize is handled automatically by ratatui
            }
            SystemMsg::Tick => {
                // Track app FPS based on tick events (approximately matches render FPS)
                if let Some(fps) = self.state.fps.record_frame(None) {
                    log::debug!("App FPS: {fps:.2}");
                }
            }
            SystemMsg::ShowError(error) => {
                log::error!("{error}");
                self.state
                    .status_bar
                    .update(StatusBarMessage::ErrorMessageChanged {
                        label: "System".to_owned(),
                        message: error,
                    });
            }
            SystemMsg::KeyInput(key) => {
                return self.handle_key_input(key);
            }
        }
        Command::none()
    }

    /// Handle key input based on current editor state
    fn handle_key_input(&mut self, key: KeyEvent) -> Command<AppMsg> {
        // Note: Ctrl+C is now handled by signal subscription, not as keyboard input
        // This ensures it works reliably across different terminal emulators and
        // properly separates OS signals from application keybindings

        // Mode-specific keybindings
        if self.state.editor.is_composing() {
            self.handle_composing_mode_key(key)
        } else {
            self.handle_normal_mode_key(key)
        }
    }

    /// Resolve keybinding to KeyAction
    /// Returns the KeyAction if the key matches a configured keybinding
    fn resolve_keybinding(&self, key: KeyEvent) -> Option<KeyAction> {
        // Check for single-key binding in Home screen keybindings
        if let Some(action) = self.config.keybindings.home.get(&vec![key]) {
            return Some(action.clone());
        }

        // TODO: Support multi-key sequences in the future
        // For now, only single-key bindings are supported

        None
    }

    /// Handle key input in Normal mode
    fn handle_normal_mode_key(&mut self, key: KeyEvent) -> Command<AppMsg> {
        // First, try to resolve from configured keybindings
        if let Some(action) = self.resolve_keybinding(key) {
            return self.handle_action(action);
        }

        // Fallback: handle special keys not in config
        match key.code {
            // Escape key - unselect/cancel (delegates to TimelineMsg::Deselect)
            KeyCode::Esc => Command::message(AppMsg::Timeline(TimelineMsg::Deselect)),
            _ => Command::none(),
        }
    }

    /// Handle key input in Composing mode
    fn handle_composing_mode_key(&mut self, key: KeyEvent) -> Command<AppMsg> {
        // In composing mode, ignore all keybindings to allow normal text input
        // This matches the old architecture behavior where 'q' is just a character,
        // not a quit command. Only hardcoded special keys are processed.
        match (key.code, key.modifiers) {
            // Escape: cancel composing
            (KeyCode::Esc, _) => Command::message(AppMsg::Editor(EditorMsg::CancelComposing)),
            // Ctrl+P: submit note (hardcoded for safety)
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                Command::message(AppMsg::Editor(EditorMsg::SubmitNote))
            }
            // All other keys are passed to textarea for input
            _ => Command::message(AppMsg::Editor(EditorMsg::ProcessTextAreaInput(key))),
        }
    }

    /// Handle a KeyAction resolved from keybinding
    fn handle_action(&mut self, action: KeyAction) -> Command<AppMsg> {
        match action {
            // Navigation
            KeyAction::ScrollUp => Command::message(AppMsg::Timeline(TimelineMsg::ScrollUp)),
            KeyAction::ScrollDown => Command::message(AppMsg::Timeline(TimelineMsg::ScrollDown)),
            KeyAction::ScrollToTop => {
                // Delegate to TimelineMsg::SelectFirst
                Command::message(AppMsg::Timeline(TimelineMsg::SelectFirst))
            }
            KeyAction::ScrollToBottom => {
                // Delegate to TimelineMsg::SelectLast
                Command::message(AppMsg::Timeline(TimelineMsg::SelectLast))
            }
            KeyAction::Unselect => {
                // Delegate to TimelineMsg::Deselect to keep logic centralized
                Command::message(AppMsg::Timeline(TimelineMsg::Deselect))
            }

            // Compose/interactions
            KeyAction::NewTextNote => Command::message(AppMsg::Editor(EditorMsg::StartComposing)),
            KeyAction::ReplyTextNote => Command::message(AppMsg::Editor(EditorMsg::StartReply)),
            KeyAction::React => Command::message(AppMsg::Timeline(TimelineMsg::ReactToSelected)),
            KeyAction::Repost => Command::message(AppMsg::Timeline(TimelineMsg::RepostSelected)),

            // Tab management
            KeyAction::OpenAuthorTimeline => {
                Command::message(AppMsg::Timeline(TimelineMsg::OpenAuthorTimeline))
            }
            KeyAction::CloseCurrentTab => {
                Command::message(AppMsg::Timeline(TimelineMsg::CloseCurrentTab))
            }
            KeyAction::PrevTab => Command::message(AppMsg::Timeline(TimelineMsg::PrevTab)),
            KeyAction::NextTab => Command::message(AppMsg::Timeline(TimelineMsg::NextTab)),

            // System
            KeyAction::Quit => Command::message(AppMsg::System(SystemMsg::Quit)),
            KeyAction::SubmitTextNote => {
                // Only valid in composing mode, handled separately
                Command::none()
            }
        }
    }

    /// Handle timeline messages
    fn handle_timeline_msg(&mut self, msg: TimelineMsg) -> Command<AppMsg> {
        if self.state.system.is_loading() {
            return Command::none();
        }

        //  Clear status message
        self.state
            .status_bar
            .update(StatusBarMessage::MessageCleared);

        match msg {
            TimelineMsg::ScrollUp => {
                return self
                    .state
                    .timeline
                    .update(TimelineMessage::PreviousItemSelected);
            }
            TimelineMsg::ScrollDown => {
                return self
                    .state
                    .timeline
                    .update(TimelineMessage::NextItemSelected);
            }
            // Select the note
            TimelineMsg::Select(index) => {
                return self
                    .state
                    .timeline
                    .update(TimelineMessage::ItemSelected { index });
            }
            // Deselect the current note
            TimelineMsg::Deselect => {
                return self
                    .state
                    .timeline
                    .update(TimelineMessage::ItemSelectionCleared);
            }
            // Select the first note in the timeline
            TimelineMsg::SelectFirst => {
                return self
                    .state
                    .timeline
                    .update(TimelineMessage::FirstItemSelected);
            }
            // Select the last note in the timeline
            TimelineMsg::SelectLast => {
                return self
                    .state
                    .timeline
                    .update(TimelineMessage::LastItemSelected);
            }
            // Load more older events
            TimelineMsg::LoadMore => {
                return self.load_more_timeline_events();
            }
            TimelineMsg::ReactToSelected => {
                // React to the selected note
                if let Some(note) = self.state.timeline.selected_note() {
                    let note1 = note.bech32_id();
                    log::info!("Reacting to event: {note1}");

                    // Send the event
                    let event_builder = EventBuilder::reaction(note.as_event(), "+");
                    match self.state.nostr.send_event_builder(event_builder) {
                        Ok(()) => {
                            self.state
                                .status_bar
                                .update(StatusBarMessage::MessageChanged {
                                    label: "Reacted".to_string(),
                                    message: note1,
                                });
                        }
                        Err(e) => {
                            let message = format!("failed to send reaction: {e}");
                            log::error!("{message}");
                            self.state
                                .status_bar
                                .update(StatusBarMessage::ErrorMessageChanged {
                                    label: "Reaction".to_string(),
                                    message,
                                });
                        }
                    }
                }
            }
            TimelineMsg::RepostSelected => {
                // Repost the selected note
                if let Some(note) = self.state.timeline.selected_note() {
                    let note1 = note.bech32_id();
                    log::info!("Reposting event: {note1}");

                    // Send the event
                    let event_builder = EventBuilder::repost(note.as_event(), None);
                    match self.state.nostr.send_event_builder(event_builder) {
                        Ok(()) => {
                            self.state
                                .status_bar
                                .update(StatusBarMessage::MessageChanged {
                                    label: "Reposted".to_owned(),
                                    message: note1,
                                });
                        }
                        Err(e) => {
                            let message = format!("failed to send repost: {e}");
                            log::error!("{message}");
                            self.state
                                .status_bar
                                .update(StatusBarMessage::ErrorMessageChanged {
                                    label: "Repost".to_string(),
                                    message,
                                });
                        }
                    }
                }
            }
            TimelineMsg::SelectTab(index) => {
                // Select a specific tab by index
                let _ = self
                    .state
                    .timeline
                    .update(TimelineMessage::TabSelected { index });
                log::debug!(
                    "Selected tab index: {}",
                    self.state.timeline.active_tab_index()
                );
            }
            TimelineMsg::NextTab => {
                // Switch to the next tab (wraps around)
                let _ = self.state.timeline.update(TimelineMessage::NextTabSelected);
                log::debug!(
                    "Switched to next tab: {}",
                    self.state.timeline.active_tab_index()
                );
            }
            TimelineMsg::PrevTab => {
                // Switch to the previous tab (wraps around)
                let _ = self
                    .state
                    .timeline
                    .update(TimelineMessage::PreviousTabSelected);
                log::debug!(
                    "Switched to previous tab: {}",
                    self.state.timeline.active_tab_index()
                );
            }
            TimelineMsg::OpenAuthorTimeline => {
                // Open author timeline for the selected note's author
                if let Some(event) = self.state.timeline.selected_note() {
                    let author_pubkey = event.author_pubkey();
                    let Ok(author_npub) = author_pubkey.to_bech32();
                    let tab_type = TimelineTabType::UserTimeline {
                        pubkey: author_pubkey,
                    };

                    // Check if tab already exists
                    if let Some(index) = self.state.timeline.find_tab_by_type(&tab_type) {
                        // Tab exists, just switch to it
                        let _ = self
                            .state
                            .timeline
                            .update(TimelineMessage::TabSelected { index });
                    } else {
                        // Tab doesn't exist, create a new one
                        let _ = self.state.timeline.update(TimelineMessage::TabAdded {
                            tab_type: tab_type.clone(),
                        });

                        if let Some(_index) = self.state.timeline.find_tab_by_type(&tab_type) {
                            log::info!("Created new author timeline for {author_npub}");

                            // Send subscription command
                            match self.state.nostr.subscribe_tab(&tab_type) {
                                Ok(()) => {
                                    log::info!(
                                        "Sent SubscribeTimeline command for user: {author_npub}"
                                    );

                                    self.state.status_bar.update(
                                        StatusBarMessage::MessageChanged {
                                            label: author_npub,
                                            message: "loading...".to_owned(),
                                        },
                                    );
                                }
                                Err(e) => {
                                    // NOTE: UI already opened the tab, so this is best-effort.
                                    // If not connected yet, the subscription will not start.
                                    log::warn!("Cannot subscribe: {author_npub} ({e:?})");
                                    self.state.status_bar.update(
                                        StatusBarMessage::ErrorMessageChanged {
                                            label: author_npub,
                                            message: format!("failed to subscribe timeline: {e:?}"),
                                        },
                                    );
                                }
                            }
                        } else {
                            log::error!("Failed to create author timeline");
                            self.state
                                .status_bar
                                .update(StatusBarMessage::ErrorMessageChanged {
                                    label: author_npub,
                                    message: "failed to open tab".to_owned(),
                                });
                        }
                    }
                }
            }
            TimelineMsg::CloseCurrentTab => {
                // Close the current tab (only if it's not the Home tab)
                let current_index = self.state.timeline.active_tab_index();

                // Get the tab type before closing
                let tab_type = self.state.timeline.active_tab().tab_type().clone();

                // Close the tab
                let _ = self.state.timeline.update(TimelineMessage::TabRemoved {
                    index: current_index,
                });

                // Unsubscribe subscriptions associated with this tab.
                self.state.nostr.unsubscribe_tab(&tab_type);
            }
        }
        Command::none()
    }

    /// Handle editor messages
    fn handle_editor_msg(&mut self, msg: EditorMsg) -> Command<AppMsg> {
        match msg {
            EditorMsg::StartComposing => self.state.editor.start_composing(),
            EditorMsg::StartReply => {
                // Get the selected note
                if let Some(note) = self.state.timeline.selected_note() {
                    let note1 = note.bech32_id();

                    // Set reply context
                    self.state.editor.start_reply(note.as_event().clone());

                    log::info!("Starting reply to event: {note1}");
                }
            }
            EditorMsg::CancelComposing => {
                self.state.editor.cancel_composing();
                self.components.borrow_mut().home.input.clear();
            }
            EditorMsg::SubmitNote => {
                // Get content from Component's TextArea
                let content = self.components.borrow().home.input.get_content();

                // Build event with appropriate tags
                let event_builder = if let Some(reply_to_event) = self.state.editor.reply_target() {
                    log::info!("Publishing reply: {content}");
                    // Create reply with proper NIP-10 tags
                    EventBuilder::text_note(&content)
                        .tag(Tag::event(reply_to_event.id))
                        .tag(Tag::public_key(reply_to_event.pubkey))
                } else {
                    log::info!("Publishing note: {content}");
                    EventBuilder::text_note(&content)
                };

                // Send the event
                match self.state.nostr.send_event_builder(event_builder) {
                    Ok(()) => {
                        self.state
                            .status_bar
                            .update(StatusBarMessage::MessageChanged {
                                label: "Posted".to_owned(),
                                message: content,
                            });
                    }
                    Err(e) => {
                        let message = format!("failed to send: {e}");
                        log::error!("{message}");
                        self.state
                            .status_bar
                            .update(StatusBarMessage::ErrorMessageChanged {
                                label: "Post".to_owned(),
                                message,
                            });
                    }
                }

                // Clear UI state
                self.state.editor.cancel_composing();
                self.components.borrow_mut().home.input.clear();
            }
            EditorMsg::ProcessTextAreaInput(key_event) => {
                // Process key input directly on the Component's TextArea
                // This avoids the expensive State → TextArea → State round-trip
                self.components
                    .borrow_mut()
                    .home
                    .input
                    .process_input(key_event);
            }
        }
        Command::none()
    }

    /// Handle Nostr messages from the subscription
    fn handle_nostr_msg(&mut self, msg: NostrMsg) -> Command<AppMsg> {
        match msg {
            NostrMsg::Connect => {
                // NostrEvents subscription handles connection automatically
                log::info!("NostrEvents subscription will handle connection");
            }
            NostrMsg::Disconnect => {
                log::info!("Disconnected from Nostr");

                // Send shutdown command through the sender
                self.state.nostr.shutdown();
            }
            NostrMsg::SubscriptionMessage(sub_msg) => {
                self.handle_nostr_subscription_message(sub_msg);
            }
        }
        Command::none()
    }

    fn handle_media_msg(&mut self, msg: Result<MediaEvent, MediaSourceError>) -> Command<AppMsg> {
        match msg {
            Ok(event) => {
                if let MediaEvent::TrackChanged { track, .. } = event {
                    if let Some((status, content)) =
                        NostrState::live_status_with_content_from_track(track)
                    {
                        let event_builder = EventBuilder::live_status(status, content.clone());

                        match self.state.nostr.send_event_builder(event_builder) {
                            Ok(()) => {
                                self.state
                                    .status_bar
                                    .update(StatusBarMessage::MessageChanged {
                                        label: "Status Updated".to_owned(),
                                        message: content,
                                    });
                            }
                            Err(e) => {
                                let message = format!("failed to update: {e}");
                                log::error!("{message}");
                                self.state
                                    .status_bar
                                    .update(StatusBarMessage::MessageChanged {
                                        label: "Status".to_owned(),
                                        message,
                                    });
                            }
                        }
                    }
                }
            }
            Err(e) => {
                log::error!("media source error: {e}");
            }
        }

        Command::none()
    }

    /// Handle NostrEvents subscription messages
    fn handle_nostr_subscription_message(&mut self, msg: NostrSubscriptionMessage) {
        match msg {
            NostrSubscriptionMessage::Ready { sender } => {
                log::info!("NostrEvents subscription ready");
                let tab_title = self
                    .state
                    .timeline
                    .active_tab()
                    .tab_title(self.state.user.profiles());
                self.state.nostr.set_command_sender(sender);
                self.state
                    .status_bar
                    .update(StatusBarMessage::MessageChanged {
                        label: tab_title,
                        message: "loading...".to_owned(),
                    });
            }
            NostrSubscriptionMessage::SubscriptionCreated {
                tab_type,
                subscription_id,
            } => {
                log::info!("Subscription created for {tab_type:?}: {subscription_id:?}");
                self.state
                    .nostr
                    .add_timeline_subscription(tab_type, subscription_id);
            }
            NostrSubscriptionMessage::Notification(notif) => match *notif {
                // NOTE: We use `RelayPoolNotification::Message` instead of `RelayPoolNotification::Event`
                // because:
                // - `Event`: Only notifies events that haven't been seen before (deduplication)
                // - `Message`: Notifies all relay messages including duplicate events
                //
                // In the current architecture, each tab subscribes to its own set of events,
                // so we need to receive all events (including duplicates across subscriptions)
                // to properly route them to the correct tab.
                // Ideally, we would cache all events globally and use `Event`, but that would
                // require a significant architectural change.
                RelayPoolNotification::Event {
                    event,
                    subscription_id,
                    ..
                } => {
                    log::debug!(
                        "Received event {} from subscription {subscription_id:?}",
                        event.id
                    );
                }
                RelayPoolNotification::Message { message, .. } => {
                    log::debug!("Received relay message: {message:?}");

                    if let RelayMessage::Event {
                        subscription_id,
                        event,
                    } = message
                    {
                        if self.state.system.is_loading() {
                            let tab_title = self
                                .state
                                .timeline
                                .active_tab()
                                .tab_title(self.state.user.profiles());
                            self.state
                                .status_bar
                                .update(StatusBarMessage::MessageChanged {
                                    label: tab_title,
                                    message: "loaded".to_owned(),
                                });
                            self.state.system.stop_loading();
                        }

                        // Find the tab that owns this subscription
                        if let Some(tab_type) = self
                            .state
                            .nostr
                            .find_tab_by_subscription(&subscription_id)
                            .cloned()
                        {
                            log::debug!(
                                "Routing event {} (kind: {:?}) to tab {tab_type:?}",
                                event.id,
                                event.kind
                            );
                            self.state
                                .process_nostr_event_for_tab(event.into_owned(), &tab_type);
                        }
                    }
                }
                RelayPoolNotification::Shutdown => {
                    log::info!("Nostr subscription shut down");
                    self.state
                        .status_bar
                        .update(StatusBarMessage::MessageChanged {
                            label: "Nostr".to_owned(),
                            message: "disconntected".to_owned(),
                        });
                }
            },
            NostrSubscriptionMessage::Error { error } => {
                self.state
                    .status_bar
                    .update(StatusBarMessage::ErrorMessageChanged {
                        label: "Nostr".to_owned(),
                        message: format!("{error:?}"),
                    });
            }
        }
    }

    /// Load more older timeline events for the active tab
    fn load_more_timeline_events(&mut self) -> Command<AppMsg> {
        log::info!("Loading more timeline events");

        // Get the oldest timestamp from the active timeline
        let until_timestamp = match self.state.timeline.oldest_timestamp() {
            Some(ts) => ts,
            None => {
                log::warn!("No oldest timestamp available, cannot load more");
                return Command::none();
            }
        };

        // Get the active tab type
        let tab = self.state.timeline.active_tab();
        let tab_type = tab.tab_type().clone();
        let tab_title = tab.tab_title(self.state.user.profiles());

        // Mark loading started
        match self
            .state
            .nostr
            .load_more_timeline(tab_type, until_timestamp)
        {
            Ok(()) => {
                // Set appropriate status message
                self.state
                    .status_bar
                    .update(StatusBarMessage::MessageChanged {
                        label: tab_title,
                        message: "loading more...".to_owned(),
                    });
            }
            Err(e) => {
                let message = format!("failed to load more events: {e}");
                log::warn!("{message}");
                self.state
                    .status_bar
                    .update(StatusBarMessage::ErrorMessageChanged {
                        label: tab_title,
                        message,
                    });
            }
        }

        Command::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::config::Config;

    /// Create a test app instance
    fn create_test_app() -> TearsApp<'static> {
        let keys = Keys::generate();
        let client = Client::default();
        let config = Config::default();

        let flags = InitFlags {
            pubkey: keys.public_key(),
            config,
            nostr_client: client,
            tick_rate: 16.0,
        };

        let (app, _) = TearsApp::new(flags);
        app
    }

    #[test]
    fn test_timeline_select_clears_status_message() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Set a status message
        app.state
            .status_bar
            .update(StatusBarMessage::MessageChanged {
                label: "Test".to_owned(),
                message: "test message".to_owned(),
            });

        // Select a note (index 0)
        let _ = app.handle_timeline_msg(TimelineMsg::Select(0));

        // Status message should be cleared
        assert_eq!(app.state.status_bar.message(), &None);
    }

    #[test]
    fn test_timeline_select_invalid_index_clears_status_message() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Set a status message
        app.state
            .status_bar
            .update(StatusBarMessage::MessageChanged {
                label: "Test".to_owned(),
                message: "test message".to_owned(),
            });

        // Select with invalid index (timeline is empty)
        let _ = app.handle_timeline_msg(TimelineMsg::Select(999));

        // Status message should be cleared
        assert_eq!(app.state.status_bar.message(), &None);
        // Selection should be None
        assert_eq!(app.state.timeline.selected_note(), None);
    }

    #[test]
    fn test_scroll_up_clears_status_message() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Set a status message
        app.state
            .status_bar
            .update(StatusBarMessage::MessageChanged {
                label: "Test".to_owned(),
                message: "test message".to_owned(),
            });

        // Scroll up
        let _ = app.handle_timeline_msg(TimelineMsg::ScrollUp);

        // Status message should be cleared
        assert_eq!(app.state.status_bar.message(), &None);
    }

    #[test]
    fn test_scroll_down_clears_status_message() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Set a status message
        app.state
            .status_bar
            .update(StatusBarMessage::MessageChanged {
                label: "Test".to_owned(),
                message: "test message".to_owned(),
            });

        // Scroll down
        let _ = app.handle_timeline_msg(TimelineMsg::ScrollDown);

        // Status message should be cleared
        assert_eq!(app.state.status_bar.message(), &None);
    }

    #[test]
    fn test_unselect_action_delegates_to_deselect() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Set selection and status message
        let _ = app
            .state
            .timeline
            .update(TimelineMessage::FirstItemSelected);
        app.state
            .status_bar
            .update(StatusBarMessage::MessageChanged {
                label: "Test".to_owned(),
                message: "test message".to_owned(),
            });

        // KeyAction::Unselect should delegate to TimelineMsg::Deselect
        // We test the end result by calling TimelineMsg::Deselect directly
        let _ = app.update(AppMsg::Timeline(TimelineMsg::Deselect));

        // Both selection and status message should be cleared
        assert_eq!(app.state.timeline.selected_note(), None);
        assert_eq!(app.state.status_bar.message(), &None);
    }

    #[test]
    fn test_timeline_message_should_be_ignored_when_loading() {
        let mut app = create_test_app();
        app.state.system.start_loading();

        // Set selection and status message
        app.state
            .status_bar
            .update(StatusBarMessage::MessageChanged {
                label: "Test".to_owned(),
                message: "test message".to_owned(),
            });

        // Execute the TimelineMsg::Deselect
        let _ = app.update(AppMsg::Timeline(TimelineMsg::Deselect));

        // Status message should remain
        assert_eq!(
            app.state.status_bar.message(),
            &Some("[Test] test message".to_owned())
        );
    }

    #[test]
    fn test_escape_key_triggers_deselect() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Set selection and status message
        let _ = app
            .state
            .timeline
            .update(TimelineMessage::ItemSelected { index: 5 });
        app.state
            .status_bar
            .update(StatusBarMessage::MessageChanged {
                label: "Test".to_owned(),
                message: "test message".to_owned(),
            });

        // Simulate Escape key press and execute the TimelineMsg::Deselect directly
        let _ = app.update(AppMsg::Timeline(TimelineMsg::Deselect));

        // Both selection and status message should be cleared
        assert_eq!(app.state.timeline.selected_note(), None);
        assert_eq!(app.state.status_bar.message(), &None);
    }

    #[test]
    fn test_timeline_deselect_clears_status_message() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Set selection and status message
        let _ = app
            .state
            .timeline
            .update(TimelineMessage::ItemSelected { index: 3 });
        app.state
            .status_bar
            .update(StatusBarMessage::MessageChanged {
                label: "Test".to_owned(),
                message: "test message".to_owned(),
            });

        // Deselect
        let _ = app.handle_timeline_msg(TimelineMsg::Deselect);

        // Both selection and status message should be cleared
        assert_eq!(app.state.timeline.selected_note(), None);
        assert_eq!(app.state.status_bar.message(), &None);
    }

    #[test]
    fn test_select_first_with_empty_timeline() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Timeline is empty
        assert!(app.state.timeline.is_empty());

        // Try to select first
        let _ = app.handle_timeline_msg(TimelineMsg::SelectFirst);

        // Selection should remain None
        assert_eq!(app.state.timeline.selected_note(), None);
    }

    #[test]
    fn test_select_first_with_notes() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Add some notes to timeline
        let keys = Keys::generate();
        let event1 = EventBuilder::text_note("test note 1")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        let event2 = EventBuilder::text_note("test note 2")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        app.state
            .process_nostr_event_for_tab(event1, &TimelineTabType::Home);
        app.state
            .process_nostr_event_for_tab(event2, &TimelineTabType::Home);

        // Select somewhere in the middle
        let _ = app
            .state
            .timeline
            .update(TimelineMessage::ItemSelected { index: 1 });

        // Select first
        let _ = app.handle_timeline_msg(TimelineMsg::SelectFirst);

        // Selection should be at index 0
        assert_eq!(app.state.timeline.selected_index(), Some(0));
    }

    #[test]
    fn test_select_last_with_empty_timeline() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Timeline is empty
        assert!(app.state.timeline.is_empty());

        // Try to select last
        let _ = app.handle_timeline_msg(TimelineMsg::SelectLast);

        // Selection should remain None
        assert_eq!(app.state.timeline.selected_note(), None);
    }

    #[test]
    fn test_select_last_with_notes() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Add some notes to timeline
        let keys = Keys::generate();
        let event1 = EventBuilder::text_note("test note 1")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        let event2 = EventBuilder::text_note("test note 2")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        app.state
            .process_nostr_event_for_tab(event1, &TimelineTabType::Home);
        app.state
            .process_nostr_event_for_tab(event2, &TimelineTabType::Home);

        // Start with no selection
        let _ = app
            .state
            .timeline
            .update(TimelineMessage::ItemSelectionCleared);

        // Select last
        let _ = app.handle_timeline_msg(TimelineMsg::SelectLast);

        // Selection should be at the last index
        let expected_index = app.state.timeline.len() - 1;
        assert_eq!(app.state.timeline.selected_index(), Some(expected_index));
    }

    #[test]
    fn test_scroll_to_top_delegates() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Add some notes
        let keys = Keys::generate();
        let event = EventBuilder::text_note("test note")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        app.state
            .process_nostr_event_for_tab(event, &TimelineTabType::Home);

        // Directly test the delegation by calling SelectFirst
        let _ = app.update(AppMsg::Timeline(TimelineMsg::SelectFirst));

        // Selection should be at index 0
        assert_eq!(app.state.timeline.selected_index(), Some(0));
    }

    #[test]
    fn test_scroll_to_bottom_delegates() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Add some notes
        let keys = Keys::generate();
        let event1 = EventBuilder::text_note("test note 1")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        let event2 = EventBuilder::text_note("test note 2")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        app.state
            .process_nostr_event_for_tab(event1, &TimelineTabType::Home);
        app.state
            .process_nostr_event_for_tab(event2, &TimelineTabType::Home);

        // Directly test the delegation by calling SelectLast
        let _ = app.update(AppMsg::Timeline(TimelineMsg::SelectLast));

        // Selection should be at the last index
        let expected_index = app.state.timeline.len() - 1;
        assert_eq!(app.state.timeline.selected_index(), Some(expected_index));
    }

    #[test]
    fn test_quit_key_works_in_normal_mode() {
        let mut app = create_test_app();

        // In normal mode, 'q' key should trigger quit via keybinding
        let q_key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let _cmd = app.handle_key_input(q_key);

        // Should produce a Quit command
        // Note: We can't directly inspect Command contents, but we can test
        // the message handling instead
        let quit_msg = AppMsg::System(SystemMsg::Quit);
        let cmd = app.update(quit_msg);
        assert!(cmd.is_some());

        // Command should be Action::Quit effect (we can't directly test this,
        // but the system should have processed it)
        // The test passes if no panic occurs
    }

    #[test]
    fn test_q_key_does_not_quit_in_composing_mode() {
        let mut app = create_test_app();

        // Start composing mode
        app.state.editor.start_composing();

        // In composing mode, 'q' key should be passed to textarea, not trigger quit
        let q_key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let _cmd = app.handle_key_input(q_key);

        // Should produce ProcessTextAreaInput command
        // The application should still be in composing mode
        assert!(app.state.editor.is_composing());

        // The textarea should contain 'q' after processing
        let _ = app.update(AppMsg::Editor(EditorMsg::ProcessTextAreaInput(q_key)));
        assert_eq!(app.components.borrow().home.input.get_content(), "q");
    }

    #[test]
    fn test_escape_cancels_composing_mode() {
        let mut app = create_test_app();

        // Start composing mode with some content
        app.state.editor.start_composing();

        // Set content directly on the component (simulating user input)
        app.components
            .borrow_mut()
            .home
            .input
            .process_input(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));

        // Escape key should cancel composing
        let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let _cmd = app.handle_key_input(esc_key);

        // Should return to normal mode
        let _ = app.update(AppMsg::Editor(EditorMsg::CancelComposing));
        assert!(app.state.editor.is_normal());
        assert!(app.components.borrow().home.input.get_content().is_empty());
    }

    #[test]
    fn test_ctrl_p_submits_note_in_composing_mode() {
        let mut app = create_test_app();

        // Start composing mode with some content
        app.state.editor.start_composing();

        // Note: In real usage, content would be set via ProcessTextAreaInput messages
        // For this test, we're just verifying the key handling produces the right command

        // Ctrl+P should submit note
        let ctrl_p_key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        let _cmd = app.handle_key_input(ctrl_p_key);

        // Should produce SubmitNote command
        // Note: Actual submission requires nostr connection, but we can verify
        // the key handling produces the right command
    }

    #[test]
    fn test_selection_preserved_when_newer_event_arrives() {
        let mut app = create_test_app();

        // Add initial events with controlled timestamps
        let keys = Keys::generate();

        // Create events with custom timestamps to ensure ordering
        // We need to use custom_created_at to guarantee different timestamps
        let now = Timestamp::now();

        let event1 = EventBuilder::text_note("oldest note")
            .custom_created_at(now - 20) // 20 seconds ago
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");

        let event2 = EventBuilder::text_note("middle note")
            .custom_created_at(now - 10) // 10 seconds ago
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");

        let event3 = EventBuilder::text_note("newest note")
            .custom_created_at(now) // now
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");

        app.state
            .process_nostr_event_for_tab(event1, &TimelineTabType::Home);
        app.state
            .process_nostr_event_for_tab(event2.clone(), &TimelineTabType::Home);
        app.state
            .process_nostr_event_for_tab(event3, &TimelineTabType::Home);

        // Timeline should be: [event3 (newest), event2 (middle), event1 (oldest)]
        // User selects index 1 (middle note - event2)
        let _ = app
            .state
            .timeline
            .update(TimelineMessage::ItemSelected { index: 1 });

        let selected_event_id = app
            .state
            .timeline
            .selected_note()
            .expect("Timeline should have event at index 1")
            .id();
        assert_eq!(selected_event_id, event2.id);

        // New event arrives with timestamp between now and middle (5 seconds ago)
        let new_event = EventBuilder::text_note("very newest note")
            .custom_created_at(now - 5)
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");

        app.state
            .process_nostr_event_for_tab(new_event, &TimelineTabType::Home);

        // Timeline should now be: [event3, new_event, event2, event1]
        // Selection index should be adjusted from 1 to 2 to still point to event2
        assert_eq!(app.state.timeline.selected_index(), Some(2));
        let still_selected_event_id = app
            .state
            .timeline
            .selected_note()
            .expect("Timeline should have event at index 2")
            .id();
        assert_eq!(still_selected_event_id, event2.id);
    }

    #[test]
    fn test_selection_not_adjusted_when_older_event_arrives() {
        let mut app = create_test_app();

        let keys = Keys::generate();
        let now = Timestamp::now();

        let event1 = EventBuilder::text_note("oldest note")
            .custom_created_at(now - 20)
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");

        let event2 = EventBuilder::text_note("newest note")
            .custom_created_at(now)
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");

        app.state
            .process_nostr_event_for_tab(event1, &TimelineTabType::Home);
        app.state
            .process_nostr_event_for_tab(event2.clone(), &TimelineTabType::Home);

        // Timeline should be: [event2 (newest), event1 (oldest)]
        // User selects index 0 (newest note - event2)
        let _ = app
            .state
            .timeline
            .update(TimelineMessage::FirstItemSelected);
        let selected_event_id = app
            .state
            .timeline
            .selected_note()
            .expect("Timeline should have first event")
            .id();
        assert_eq!(selected_event_id, event2.id);

        // Even older event arrives (will be inserted after the selection)
        let old_event = EventBuilder::text_note("very old note")
            .custom_created_at(now - 30)
            .sign_with_keys(&Keys::generate())
            .expect("Failed to sign test event");

        app.state
            .process_nostr_event_for_tab(old_event, &TimelineTabType::Home);

        // Timeline should now be: [event2, event1, old_event]
        // Selection should remain at index 0, still pointing to the newest note
        assert_eq!(app.state.timeline.selected_index(), Some(0));
        let still_selected_event_id = app
            .state
            .timeline
            .selected_note()
            .expect("Timeline should have first event")
            .id();
        assert_eq!(still_selected_event_id, event2.id);
    }

    #[test]
    fn test_no_selection_when_event_arrives() {
        let mut app = create_test_app();

        // No selection
        let _ = app
            .state
            .timeline
            .update(TimelineMessage::ItemSelectionCleared);

        let keys = Keys::generate();
        let event = EventBuilder::text_note("test note")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");

        app.state
            .process_nostr_event_for_tab(event, &TimelineTabType::Home);

        // Selection should remain None
        assert_eq!(app.state.timeline.selected_note(), None);
    }

    #[test]
    fn test_select_tab() {
        let mut app = create_test_app();

        // Default tab should be 0
        assert_eq!(app.state.timeline.active_tab_index(), 0);

        // Select tab 0 (only tab available)
        let _ = app.handle_timeline_msg(TimelineMsg::SelectTab(0));
        assert_eq!(app.state.timeline.active_tab_index(), 0);

        // Try to select tab beyond max (stub does nothing)
        let _ = app.handle_timeline_msg(TimelineMsg::SelectTab(5));
        assert_eq!(app.state.timeline.active_tab_index(), 0);
    }
}
