//! Main Tears Application implementation

use std::cell::RefCell;
use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use nostr_sdk::prelude::*;
use ratatui::prelude::*;
use tears::prelude::*;
use tears::subscription::terminal::TerminalEvents;
use tears::subscription::time::{Message as TimerMessage, Timer};

use crate::core::message::{AppMsg, EditorMsg, NostrMsg, SystemMsg, TimelineMsg};
use crate::core::state::timeline::TimelineTabType;
use crate::core::state::AppState;
use crate::domain::nostr::Profile;
use crate::infrastructure::config::Config;
use crate::infrastructure::subscription::nostr::{
    Message as NostrSubscriptionMessage, NostrCommand, NostrEvents,
};
use crate::presentation::components::Components;
use crate::presentation::config::keybindings::Action as KeyAction;

/// Initialization flags for the Tears application
#[derive(Debug)]
pub struct InitFlags {
    pub pubkey: Option<PublicKey>,
    pub config: Config,
    pub nostr_client: Client,
    pub keys: Keys,
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
    /// User's keys for signing events
    keys: Keys,
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
        let state = if let Some(pubkey) = flags.pubkey {
            AppState::new_with_config(pubkey, flags.config)
        } else {
            // Use default state with config
            let mut state = AppState::default();
            state.config.config = flags.config;
            state
        };

        // Initialize components
        let components = Components::new();

        // Wrap client in Arc for sharing across subscriptions
        // This ensures subscription identity remains constant
        let nostr_client = Arc::new(flags.nostr_client);

        let app = Self {
            state,
            components: RefCell::new(components),
            keys: flags.keys,
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
                Err(e) => AppMsg::System(SystemMsg::TerminalError(e.to_string())),
            }),
        ];

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
                if let Some(sender) = &self.state.nostr.command_sender {
                    // Clone sender to avoid borrow conflicts
                    let sender = sender.clone();

                    // Collect all subscription IDs from all tabs
                    let all_subscription_ids: Vec<nostr_sdk::SubscriptionId> = self
                        .state
                        .timeline
                        .tabs()
                        .iter()
                        .flat_map(|tab| {
                            self.state.nostr.remove_timeline_subscription(&tab.tab_type)
                        })
                        .collect();

                    if !all_subscription_ids.is_empty() {
                        log::info!(
                            "Unsubscribing from {} subscriptions before shutdown",
                            all_subscription_ids.len()
                        );
                        let _ = sender.send(NostrCommand::Unsubscribe {
                            subscription_ids: all_subscription_ids,
                        });
                    }

                    // Send shutdown command to disconnect from relays
                    log::info!("Sending shutdown command to Nostr client");
                    let _ = sender.send(NostrCommand::Shutdown);
                } else {
                    log::warn!("No Nostr command sender available during shutdown");
                }

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
                self.state.system.set_status_message(error);
            }
            SystemMsg::KeyInput(key) => {
                return self.handle_key_input(key);
            }
            SystemMsg::TerminalError(error) => {
                log::error!("Terminal error: {error}");
                self.state
                    .system
                    .set_status_message(format!("Terminal error: {error}"));
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
        self.state.system.clear_status_message();

        match msg {
            TimelineMsg::ScrollUp => self.state.timeline.scroll_up(),
            TimelineMsg::ScrollDown => {
                // Check if at bottom before scrolling
                let was_at_bottom = self.state.timeline.is_at_bottom();
                self.state.timeline.scroll_down();

                // If we were at bottom and still at bottom (can't scroll further), load more
                if was_at_bottom && self.state.timeline.is_at_bottom() {
                    return Command::message(AppMsg::Timeline(TimelineMsg::LoadMore));
                }
            }
            // Select the note
            TimelineMsg::Select(index) => self.state.timeline.select(index),
            // Deselect the current note
            TimelineMsg::Deselect => self.state.timeline.deselect(),
            // Select the first note in the timeline
            TimelineMsg::SelectFirst => self.state.timeline.select_first(),
            // Select the last note in the timeline
            TimelineMsg::SelectLast => self.state.timeline.select_last(),
            // Load more older events
            TimelineMsg::LoadMore => {
                return self.load_more_timeline_events();
            }
            TimelineMsg::ReactToSelected => {
                // React to the selected note
                if let Some(note) = self.state.timeline.selected_note() {
                    let event_id = note.id;
                    let Ok(note1) = event_id.to_bech32();
                    log::info!("Reacting to event: {note1}");

                    let event_builder = EventBuilder::reaction(note, "+");
                    let success_msg = format!("[Reacted] note {note1}");
                    self.send_signed_event(event_builder, success_msg, "Failed to sign reaction");
                } else {
                    self.state.system.set_status_message("No note selected");
                }
            }
            TimelineMsg::RepostSelected => {
                // Repost the selected note
                if let Some(note) = self.state.timeline.selected_note() {
                    let event_id = note.id;
                    let Ok(note1) = event_id.to_bech32();
                    log::info!("Reposting event: {note1}");

                    let event_builder = EventBuilder::repost(note, None);
                    let success_msg = format!("[Reposted] {note1}");
                    self.send_signed_event(event_builder, success_msg, "Failed to sign repost");
                } else {
                    self.state.system.set_status_message("No note selected");
                }
            }
            TimelineMsg::SelectTab(index) => {
                // Select a specific tab by index
                // Delegate to TimelineState
                self.state.timeline.select_tab(index);
                log::debug!(
                    "Selected tab index: {}",
                    self.state.timeline.active_tab_index()
                );
            }
            TimelineMsg::NextTab => {
                // Switch to the next tab (wraps around)
                // Delegate to TimelineState
                self.state.timeline.next_tab();
                log::debug!(
                    "Switched to next tab: {}",
                    self.state.timeline.active_tab_index()
                );
            }
            TimelineMsg::PrevTab => {
                // Switch to the previous tab (wraps around)
                // Delegate to TimelineState
                self.state.timeline.prev_tab();
                log::debug!(
                    "Switched to previous tab: {}",
                    self.state.timeline.active_tab_index()
                );
            }
            TimelineMsg::OpenAuthorTimeline => {
                // Open author timeline for the selected note's author
                if let Some(event) = self.state.timeline.selected_note() {
                    let author_pubkey = event.pubkey;
                    let tab_type = TimelineTabType::UserTimeline {
                        pubkey: author_pubkey,
                    };

                    // Check if tab already exists
                    if let Some(index) = self.state.timeline.find_tab_by_type(&tab_type) {
                        // Tab exists, just switch to it
                        self.state.timeline.select_tab(index);
                        log::info!("Switched to existing author timeline for {author_pubkey}");
                        let short_hex = &author_pubkey.to_hex()[..8];
                        self.state
                            .system
                            .set_status_message(format!("Switched to timeline for {short_hex}"));
                    } else {
                        // Tab doesn't exist, create a new one
                        match self.state.timeline.add_tab(tab_type.clone()) {
                            Ok(new_index) => {
                                self.state.timeline.select_tab(new_index);
                                log::info!("Created new author timeline for {author_pubkey}");
                                let short_hex = &author_pubkey.to_hex()[..8];

                                // Send subscription command
                                if let Some(sender) = &self.state.nostr.command_sender {
                                    let _ = sender.send(NostrCommand::SubscribeTimeline {
                                        tab_type: tab_type.clone(),
                                    });
                                    log::info!(
                                        "Sent SubscribeTimeline command for user: {author_pubkey}"
                                    );
                                    self.state.system.set_status_message(format!(
                                        "Opening timeline for {short_hex}"
                                    ));
                                } else {
                                    log::warn!(
                                        "Cannot subscribe: Nostr command sender not available"
                                    );
                                    self.state.system.set_status_message(format!(
                                        "Opened timeline for {short_hex}"
                                    ));
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to create author timeline: {e}");
                                self.state
                                    .system
                                    .set_status_message(format!("Failed to create tab: {e}"));
                            }
                        }
                    }
                } else {
                    self.state.system.set_status_message("No note selected");
                }
            }
            TimelineMsg::CloseCurrentTab => {
                // Close the current tab (only if it's not the Home tab)
                let current_index = self.state.timeline.active_tab_index();

                // Get the tab type before closing
                let tab_type = self.state.timeline.tabs()[current_index].tab_type.clone();

                // Try to close the tab
                match self.state.timeline.remove_tab(current_index) {
                    Ok(()) => {
                        // Get all subscription IDs for this tab
                        let sub_ids = self.state.nostr.remove_timeline_subscription(&tab_type);

                        // Unsubscribe from all subscriptions
                        if !sub_ids.is_empty() {
                            if let Some(sender) = &self.state.nostr.command_sender {
                                let _ = sender.send(NostrCommand::Unsubscribe {
                                    subscription_ids: sub_ids.clone(),
                                });
                                log::info!(
                                    "Sent Unsubscribe command for {} subscriptions for tab {tab_type:?}: {:?}",
                                    sub_ids.len(),
                                    sub_ids
                                );
                            }
                        } else {
                            log::warn!("No subscriptions found for tab {tab_type:?} during close");
                        }

                        log::info!("Closed tab at index {current_index}");
                        self.state.system.set_status_message("Tab closed");
                    }
                    Err(e) => {
                        log::warn!("Cannot close tab: {e}");
                        self.state.system.set_status_message(e);
                    }
                }
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
                    let event_id = note.id;

                    // Set reply context
                    self.state.editor.start_reply(note.clone());

                    // Show status message
                    self.state.system.set_status_message(format!(
                        "Replying to note {}",
                        &event_id.to_hex()[..8]
                    ));

                    log::info!("Starting reply to event: {event_id}");
                } else {
                    self.state.system.set_status_message("No note selected");
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

                // Send the signed event
                self.send_signed_event(
                    event_builder,
                    format!(
                        "[Posted] {}",
                        content.lines().collect::<Vec<&str>>().join(" ")
                    ),
                    "Failed to sign event",
                );

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
                // Send shutdown command through the sender
                if let Some(sender) = &self.state.nostr.command_sender {
                    let _ = sender.send(NostrCommand::Shutdown);
                }
            }
            NostrMsg::EventReceived(event) => {
                log::debug!("Received event: {}", event.id);
                self.process_nostr_event(*event);
            }
            NostrMsg::SubscriptionMessage(sub_msg) => {
                self.handle_nostr_subscription_message(sub_msg);
            }
        }
        Command::none()
    }

    /// Handle NostrEvents subscription messages
    fn handle_nostr_subscription_message(&mut self, msg: NostrSubscriptionMessage) {
        match msg {
            NostrSubscriptionMessage::Ready { sender } => {
                log::info!("NostrEvents subscription ready");
                self.state.nostr.command_sender = Some(sender);
                self.state.system.set_status_message("[Home] Loading...");
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
                            self.state.system.set_status_message("[Home] Loaded");
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
                            self.process_nostr_event_for_tab(event.into_owned(), &tab_type);
                        } else {
                            // Fallback: use default processing (for backward compatibility)
                            log::debug!(
                            "Subscription {subscription_id:?} not found, using default processing"
                        );
                            self.process_nostr_event(event.into_owned());
                        }
                    }
                }
                RelayPoolNotification::Shutdown => {
                    log::info!("Nostr subscription shut down");
                    self.state.nostr.command_sender = None;
                    self.state
                        .system
                        .set_status_message("Disconnected from Nostr");
                }
            },
            NostrSubscriptionMessage::Error { error } => {
                log::error!("NostrEvents error: {error:?}");
                self.state
                    .system
                    .set_status_message(format!("Nostr error: {error:?}"));
            }
        }
    }

    /// Process a received Nostr event for a specific tab
    fn process_nostr_event_for_tab(&mut self, event: nostr_sdk::Event, tab_type: &TimelineTabType) {
        match event.kind {
            Kind::TextNote => {
                // Add note to the specific tab
                let (was_inserted, loading_completed) =
                    self.state.timeline.add_note_to_tab(event.clone(), tab_type);

                if was_inserted {
                    log::info!(
                        "Added event {} (created_at: {}) to tab {tab_type:?}",
                        event.id,
                        event.created_at
                    );
                } else {
                    log::debug!("Skipped duplicate event {} for tab {tab_type:?}", event.id);
                }
                if loading_completed {
                    log::info!("Load more completed for tab {tab_type:?}");
                    match tab_type {
                        TimelineTabType::Home => {
                            self.state.system.set_status_message("[Home] Loaded more");
                        }
                        TimelineTabType::UserTimeline { .. } => {
                            self.state.system.set_status_message("[User] Loaded more");
                        }
                    }
                }
            }
            Kind::Metadata => {
                // Metadata is shared across all tabs
                if let Ok(metadata) = Metadata::from_json(event.content.clone()) {
                    let profile = Profile::new(event.pubkey, event.created_at, metadata);
                    if self.state.user.insert_newer_profile(profile) {
                        log::debug!("Updated profile for pubkey: {}", event.pubkey);
                    }
                }
            }
            Kind::Repost => {
                // Reactions are shared (global_reactions)
                if let Some(event_id) = self.state.timeline.add_repost(event) {
                    log::debug!("Added repost for event: {event_id}");
                }
            }
            Kind::Reaction => {
                // Reactions are shared (global_reactions)
                if let Some(event_id) = self.state.timeline.add_reaction(event) {
                    log::debug!("Added reaction for event: {event_id}");
                }
            }
            Kind::ZapReceipt => {
                // Zap receipts are shared (global_zap_receipts)
                if let Some(event_id) = self.state.timeline.add_zap_receipt(event) {
                    log::debug!("Added zap receipt for event: {event_id}");
                }
            }
            _ => {
                log::debug!("Received unknown event type: {:?}", event.kind);
            }
        }
    }

    /// Process a received Nostr event
    fn process_nostr_event(&mut self, event: nostr_sdk::Event) {
        match event.kind {
            Kind::TextNote => {
                // Add text note to timeline
                let (was_inserted, loading_completed) = self.state.timeline.add_note(event);

                if was_inserted {
                    log::debug!("Added text note to timeline");
                }

                if loading_completed {
                    log::info!("Load more completed");
                    self.state.system.set_status_message("[Home] Loaded more");
                }
            }
            Kind::Metadata => {
                // Parse and store profile metadata
                if let Ok(metadata) = Metadata::from_json(event.content.clone()) {
                    let profile = Profile::new(event.pubkey, event.created_at, metadata);
                    if self.state.user.insert_newer_profile(profile) {
                        log::debug!("Updated profile for pubkey: {}", event.pubkey);
                    }
                } else {
                    log::warn!("Failed to parse metadata for pubkey: {}", event.pubkey);
                    self.state
                        .system
                        .set_status_message("Failed to parse profile metadata");
                }
            }
            Kind::Reaction => {
                // Add reaction to timeline engagement data
                if let Some(event_id) = self.state.timeline.add_reaction(event) {
                    log::debug!("Added reaction for event: {event_id}");
                }
            }
            Kind::Repost => {
                // Add repost to timeline engagement data
                if let Some(event_id) = self.state.timeline.add_repost(event) {
                    log::debug!("Added repost for event: {event_id}");
                }
            }
            Kind::ZapReceipt => {
                // Add zap receipt to timeline engagement data
                if let Some(event_id) = self.state.timeline.add_zap_receipt(event) {
                    log::debug!("Added zap receipt for event: {event_id}");
                }
            }
            _ => {
                // Unknown event types are logged but not processed
                log::debug!("Received unknown event type: {:?}", event.kind);
                self.state
                    .system
                    .set_status_message(format!("Received unknown event type: {}", event.kind));
            }
        }
    }

    /// Send a signed Nostr event through the command sender
    /// Returns true if the event was sent successfully, false otherwise
    fn send_signed_event(
        &mut self,
        event_builder: EventBuilder,
        success_message: String,
        error_prefix: &str,
    ) -> bool {
        if let Some(sender) = &self.state.nostr.command_sender {
            match event_builder.sign_with_keys(&self.keys) {
                Ok(event) => {
                    let _ = sender.send(NostrCommand::SendEvent {
                        event: event.clone(),
                    });
                    // Process the sent event locally for optimistic update
                    self.process_nostr_event(event);
                    self.state.system.set_status_message(success_message);
                    true
                }
                Err(e) => {
                    log::error!("{error_prefix}: {e}");
                    self.state
                        .system
                        .set_status_message(format!("{error_prefix}: {e}"));
                    false
                }
            }
        } else {
            false
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
        let active_tab_index = self.state.timeline.active_tab_index();
        let tab_type = self.state.timeline.tabs()[active_tab_index]
            .tab_type
            .clone();

        // Get the command sender
        if let Some(sender) = &self.state.nostr.command_sender {
            // Mark loading started
            self.state.timeline.start_loading_more();

            // Send command to NostrEvents to load more timeline events
            let _ = sender.send(NostrCommand::LoadMoreTimeline {
                tab_type: tab_type.clone(),
                until: until_timestamp,
            });

            // Set appropriate status message
            let status_msg = match tab_type {
                TimelineTabType::Home => "[Home] Loading more ...".to_string(),
                TimelineTabType::UserTimeline { pubkey } => {
                    format!("[User {}] Loading more ...", &pubkey.to_hex()[..8])
                }
            };
            self.state.system.set_status_message(status_msg);
        } else {
            log::warn!("No Nostr command sender available");
            self.state
                .system
                .set_status_message("Not connected to Nostr");
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
            pubkey: Some(keys.public_key()),
            config,
            nostr_client: client,
            keys,
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
        app.state.system.set_status_message("Test message");

        // Select a note (index 0)
        app.handle_timeline_msg(TimelineMsg::Select(0));

        // Status message should be cleared
        assert_eq!(app.state.system.status_message(), None);
    }

    #[test]
    fn test_timeline_select_invalid_index_clears_status_message() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Set a status message
        app.state.system.set_status_message("Test message");

        // Select with invalid index (timeline is empty)
        app.handle_timeline_msg(TimelineMsg::Select(999));

        // Status message should be cleared
        assert_eq!(app.state.system.status_message(), None);
        // Selection should be None
        assert_eq!(app.state.timeline.selected_note(), None);
    }

    #[test]
    fn test_scroll_up_clears_status_message() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Set a status message
        app.state.system.set_status_message("Test message");

        // Scroll up
        app.handle_timeline_msg(TimelineMsg::ScrollUp);

        // Status message should be cleared
        assert_eq!(app.state.system.status_message(), None);
    }

    #[test]
    fn test_scroll_down_clears_status_message() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Set a status message
        app.state.system.set_status_message("Test message");

        // Scroll down
        app.handle_timeline_msg(TimelineMsg::ScrollDown);

        // Status message should be cleared
        assert_eq!(app.state.system.status_message(), None);
    }

    #[test]
    fn test_unselect_action_delegates_to_deselect() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Set selection and status message
        app.state.timeline.select_first();
        app.state.system.set_status_message("Test message");

        // KeyAction::Unselect should delegate to TimelineMsg::Deselect
        // We test the end result by calling TimelineMsg::Deselect directly
        app.update(AppMsg::Timeline(TimelineMsg::Deselect));

        // Both selection and status message should be cleared
        assert_eq!(app.state.timeline.selected_note(), None);
        assert_eq!(app.state.system.status_message(), None);
    }

    #[test]
    fn test_timeline_message_should_be_ignored_when_loading() {
        let mut app = create_test_app();
        app.state.system.start_loading();

        // Set selection and status message
        app.state.system.set_status_message("Test message");

        // Execute the TimelineMsg::Deselect
        app.update(AppMsg::Timeline(TimelineMsg::Deselect));

        // Status message should remain
        assert_eq!(
            app.state.system.status_message(),
            Some(&"Test message".to_owned())
        );
    }

    #[test]
    fn test_escape_key_triggers_deselect() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Set selection and status message
        app.state.timeline.select(5);
        app.state.system.set_status_message("Test message");

        // Simulate Escape key press and execute the TimelineMsg::Deselect directly
        app.update(AppMsg::Timeline(TimelineMsg::Deselect));

        // Both selection and status message should be cleared
        assert_eq!(app.state.timeline.selected_note(), None);
        assert_eq!(app.state.system.status_message(), None);
    }

    #[test]
    fn test_timeline_deselect_clears_status_message() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Set selection and status message
        app.state.timeline.select(3);
        app.state.system.set_status_message("Test message");

        // Deselect
        app.handle_timeline_msg(TimelineMsg::Deselect);

        // Both selection and status message should be cleared
        assert_eq!(app.state.timeline.selected_note(), None);
        assert_eq!(app.state.system.status_message(), None);
    }

    #[test]
    fn test_select_first_with_empty_timeline() {
        let mut app = create_test_app();
        app.state.system.stop_loading();

        // Timeline is empty
        assert!(app.state.timeline.is_empty());

        // Try to select first
        app.handle_timeline_msg(TimelineMsg::SelectFirst);

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
        app.process_nostr_event(event1);
        app.process_nostr_event(event2);

        // Select somewhere in the middle
        app.state.timeline.select(1);

        // Select first
        app.handle_timeline_msg(TimelineMsg::SelectFirst);

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
        app.handle_timeline_msg(TimelineMsg::SelectLast);

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
        app.process_nostr_event(event1);
        app.process_nostr_event(event2);

        // Start with no selection
        app.state.timeline.deselect();

        // Select last
        app.handle_timeline_msg(TimelineMsg::SelectLast);

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
        app.process_nostr_event(event);

        // Directly test the delegation by calling SelectFirst
        app.update(AppMsg::Timeline(TimelineMsg::SelectFirst));

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
        app.process_nostr_event(event1);
        app.process_nostr_event(event2);

        // Directly test the delegation by calling SelectLast
        app.update(AppMsg::Timeline(TimelineMsg::SelectLast));

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
        let _cmd = app.update(quit_msg);

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
        app.update(AppMsg::Editor(EditorMsg::ProcessTextAreaInput(q_key)));
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
        app.update(AppMsg::Editor(EditorMsg::CancelComposing));
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

        app.process_nostr_event(event1);
        app.process_nostr_event(event2.clone());
        app.process_nostr_event(event3);

        // Timeline should be: [event3 (newest), event2 (middle), event1 (oldest)]
        // User selects index 1 (middle note - event2)
        app.state.timeline.select(1);

        let selected_event_id = app
            .state
            .timeline
            .selected_note()
            .expect("Timeline should have event at index 1")
            .id;
        assert_eq!(selected_event_id, event2.id);

        // New event arrives with timestamp between now and middle (5 seconds ago)
        let new_event = EventBuilder::text_note("very newest note")
            .custom_created_at(now - 5)
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");

        app.process_nostr_event(new_event);

        // Timeline should now be: [event3, new_event, event2, event1]
        // Selection index should be adjusted from 1 to 2 to still point to event2
        assert_eq!(app.state.timeline.selected_index(), Some(2));
        let still_selected_event_id = app
            .state
            .timeline
            .selected_note()
            .expect("Timeline should have event at index 2")
            .id;
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

        app.process_nostr_event(event1);
        app.process_nostr_event(event2.clone());

        // Timeline should be: [event2 (newest), event1 (oldest)]
        // User selects index 0 (newest note - event2)
        app.state.timeline.select_first();
        let selected_event_id = app
            .state
            .timeline
            .selected_note()
            .expect("Timeline should have first event")
            .id;
        assert_eq!(selected_event_id, event2.id);

        // Even older event arrives (will be inserted after the selection)
        let old_event = EventBuilder::text_note("very old note")
            .custom_created_at(now - 30)
            .sign_with_keys(&Keys::generate())
            .expect("Failed to sign test event");

        app.process_nostr_event(old_event);

        // Timeline should now be: [event2, event1, old_event]
        // Selection should remain at index 0, still pointing to the newest note
        assert_eq!(app.state.timeline.selected_index(), Some(0));
        let still_selected_event_id = app
            .state
            .timeline
            .selected_note()
            .expect("Timeline should have first event")
            .id;
        assert_eq!(still_selected_event_id, event2.id);
    }

    #[test]
    fn test_no_selection_when_event_arrives() {
        let mut app = create_test_app();

        // No selection
        app.state.timeline.deselect();

        let keys = Keys::generate();
        let event = EventBuilder::text_note("test note")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");

        app.process_nostr_event(event);

        // Selection should remain None
        assert_eq!(app.state.timeline.selected_note(), None);
    }

    #[test]
    fn test_select_tab() {
        let mut app = create_test_app();

        // Default tab should be 0
        assert_eq!(app.state.timeline.active_tab_index(), 0);

        // Select tab 0 (only tab available)
        app.handle_timeline_msg(TimelineMsg::SelectTab(0));
        assert_eq!(app.state.timeline.active_tab_index(), 0);

        // Try to select tab beyond max (stub does nothing)
        app.handle_timeline_msg(TimelineMsg::SelectTab(5));
        assert_eq!(app.state.timeline.active_tab_index(), 0);
    }
}
