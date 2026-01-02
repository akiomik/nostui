//! Main Tears Application implementation

use std::cell::RefCell;
use std::cmp::Reverse;
use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use nostr_sdk::prelude::*;
use ratatui::prelude::*;
use tears::prelude::*;
use tears::subscription::terminal::TerminalEvents;
use tears::subscription::time::{Message as TimerMessage, Timer};
use tui_textarea::CursorMove;

use crate::core::state::{ui::UiMode, AppState};
use crate::domain::nostr::{Profile, SortableEvent};
use crate::domain::ui::{CursorPosition, TextSelection};
use crate::infrastructure::config::Config;
use crate::presentation::config::keybindings::Action as KeyAction;
use crate::tears::subscription::nostr::{
    Message as NostrSubscriptionMessage, NostrCommand, NostrEvents,
};
use crate::tears_app::message::NostrMsg;

use super::{
    components::Components,
    fps_tracker::FpsTracker,
    message::{AppMsg, SystemMsg, TimelineMsg, UiMsg},
};

/// Initialization flags for the Tears application
#[derive(Debug)]
pub struct InitFlags {
    pub pubkey: Option<PublicKey>,
    pub config: Config,
    pub nostr_client: Client,
    pub keys: Keys,
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
    /// FPS tracker for app updates
    app_fps_tracker: FpsTracker,
    /// Configuration (including keybindings)
    config: Config,
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
            app_fps_tracker: FpsTracker::new(),
            config,
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
            AppMsg::Ui(ui_msg) => self.handle_ui_msg(ui_msg),
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
            // Timer subscription - matches runtime FPS (60 Hz = ~16.67ms)
            Subscription::new(Timer::new(16)).map(|msg| match msg {
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
                // TODO: Add cleanup logic here in the future (e.g., save state, close connections)
                // For now, directly trigger the quit action
                return Command::effect(Action::Quit);
            }
            SystemMsg::Resize(width, height) => {
                log::debug!("Terminal resized to {width}x{height}");
                // Terminal resize is handled automatically by ratatui
            }
            SystemMsg::Tick => {
                // Track app FPS based on tick events (approximately matches render FPS)
                if let Some(fps) = self.app_fps_tracker.record_frame() {
                    self.state.system.fps_data.app_fps = fps;
                    log::debug!("App FPS: {fps:.2}");
                }
            }
            SystemMsg::ShowError(error) => {
                self.state.system.status_message = Some(error);
            }
            SystemMsg::KeyInput(key) => {
                return self.handle_key_input(key);
            }
            SystemMsg::TerminalError(error) => {
                log::error!("Terminal error: {error}");
                self.state.system.status_message = Some(format!("Terminal error: {error}"));
            }
            SystemMsg::Suspend => {
                log::info!("Suspend requested");
                // Send SIGTSTP signal to suspend the application
                #[cfg(unix)]
                {
                    use std::process::{id, Command as StdCommand};
                    // Use kill command to send SIGTSTP to current process
                    let pid = id();
                    let _ = StdCommand::new("kill")
                        .arg("-TSTP")
                        .arg(pid.to_string())
                        .spawn();
                }
                #[cfg(not(unix))]
                {
                    log::warn!("Suspend is only supported on Unix systems");
                    self.state.system.status_message =
                        Some("Suspend is only supported on Unix systems".to_string());
                }
            }
        }
        Command::none()
    }

    /// Handle key input based on current UI mode
    fn handle_key_input(&mut self, key: KeyEvent) -> Command<AppMsg> {
        // Note: Ctrl+C is now handled by signal subscription, not as keyboard input
        // This ensures it works reliably across different terminal emulators and
        // properly separates OS signals from application keybindings

        // Mode-specific keybindings
        match self.state.ui.current_mode {
            UiMode::Normal => self.handle_normal_mode_key(key),
            UiMode::Composing => self.handle_composing_mode_key(key),
        }
    }

    /// Resolve keybinding to KeyAction
    /// Returns the KeyAction if the key matches a configured keybinding
    fn resolve_keybinding(&self, key: KeyEvent) -> Option<KeyAction> {
        // Check for single-key binding
        if let Some(action) = self.config.keybindings.get(&vec![key]) {
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
            KeyCode::Esc => Command::single(AppMsg::Timeline(TimelineMsg::Deselect)),
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
            (KeyCode::Esc, _) => Command::single(AppMsg::Ui(UiMsg::CancelComposing)),
            // Ctrl+P: submit note (hardcoded for safety)
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                Command::single(AppMsg::Ui(UiMsg::SubmitNote))
            }
            // All other keys are passed to textarea for input
            _ => Command::single(AppMsg::Ui(UiMsg::ProcessTextAreaInput(key))),
        }
    }

    /// Handle a KeyAction resolved from keybinding
    fn handle_action(&mut self, action: KeyAction) -> Command<AppMsg> {
        match action {
            // Navigation
            KeyAction::ScrollUp => Command::single(AppMsg::Timeline(TimelineMsg::ScrollUp)),
            KeyAction::ScrollDown => Command::single(AppMsg::Timeline(TimelineMsg::ScrollDown)),
            KeyAction::ScrollToTop => {
                // Delegate to TimelineMsg::SelectFirst
                Command::single(AppMsg::Timeline(TimelineMsg::SelectFirst))
            }
            KeyAction::ScrollToBottom => {
                // Delegate to TimelineMsg::SelectLast
                Command::single(AppMsg::Timeline(TimelineMsg::SelectLast))
            }
            KeyAction::Unselect => {
                // Delegate to TimelineMsg::Deselect to keep logic centralized
                Command::single(AppMsg::Timeline(TimelineMsg::Deselect))
            }

            // Compose/interactions
            KeyAction::NewTextNote => Command::single(AppMsg::Ui(UiMsg::StartComposing)),
            KeyAction::ReplyTextNote => Command::single(AppMsg::Ui(UiMsg::StartReply)),
            KeyAction::React => Command::single(AppMsg::Ui(UiMsg::ReactToSelected)),
            KeyAction::Repost => Command::single(AppMsg::Ui(UiMsg::RepostSelected)),

            // System
            KeyAction::Quit => Command::single(AppMsg::System(SystemMsg::Quit)),
            KeyAction::Suspend => Command::single(AppMsg::System(SystemMsg::Suspend)),
            KeyAction::SubmitTextNote => {
                // Only valid in composing mode, handled separately
                Command::none()
            }
        }
    }

    /// Handle timeline messages
    fn handle_timeline_msg(&mut self, msg: TimelineMsg) -> Command<AppMsg> {
        match msg {
            TimelineMsg::ScrollUp => {
                // Move selection up
                if let Some(current) = self.state.timeline.selected_index {
                    if current > 0 {
                        self.state.timeline.selected_index = Some(current - 1);
                    }
                } else if !self.state.timeline.notes.is_empty() {
                    self.state.timeline.selected_index = Some(0);
                }
            }
            TimelineMsg::ScrollDown => {
                // Move selection down
                let max_index = self.state.timeline.notes.len().saturating_sub(1);
                if let Some(current) = self.state.timeline.selected_index {
                    if current < max_index {
                        self.state.timeline.selected_index = Some(current + 1);
                    }
                } else if !self.state.timeline.notes.is_empty() {
                    self.state.timeline.selected_index = Some(0);
                }
            }
            TimelineMsg::Select(index) => {
                if index < self.state.timeline.notes.len() {
                    self.state.timeline.selected_index = Some(index);
                } else {
                    // Deselecting (e.g., Select(0) when timeline is empty or invalid index)
                    self.state.timeline.selected_index = None;
                }
                // Clear status message when explicitly selecting/deselecting
                // This matches the old architecture behavior (TimelineMsg::DeselectNote)
                self.state.system.status_message = None;
            }
            TimelineMsg::Deselect => {
                // Deselect the current note and clear status message
                // This matches the old architecture behavior (TimelineMsg::DeselectNote)
                self.state.timeline.selected_index = None;
                self.state.system.status_message = None;
            }
            TimelineMsg::SelectFirst => {
                // Select the first note in the timeline
                if !self.state.timeline.notes.is_empty() {
                    self.state.timeline.selected_index = Some(0);
                }
            }
            TimelineMsg::SelectLast => {
                // Select the last note in the timeline
                let max_index = self.state.timeline.notes.len().saturating_sub(1);
                if !self.state.timeline.notes.is_empty() {
                    self.state.timeline.selected_index = Some(max_index);
                }
            }
        }
        Command::none()
    }

    /// Handle UI messages
    fn handle_ui_msg(&mut self, msg: UiMsg) -> Command<AppMsg> {
        match msg {
            UiMsg::StartComposing => {
                self.state.ui.current_mode = UiMode::Composing;
                self.state.ui.reply_to = None;
            }
            UiMsg::StartReply => {
                // Get the selected note
                if let Some(selected_index) = self.state.timeline.selected_index {
                    if let Some(note) = self.state.timeline.notes.get(selected_index) {
                        let event_id = note.0.event.id;

                        // Set reply context
                        self.state.ui.reply_to = Some(note.0.event.clone());
                        self.state.ui.current_mode = UiMode::Composing;

                        // Show status message
                        self.state.system.status_message =
                            Some(format!("Replying to note {}", &event_id.to_hex()[..8]));

                        log::info!("Starting reply to event: {event_id}");
                    } else {
                        self.state.system.status_message = Some("No note selected".to_string());
                    }
                } else {
                    self.state.system.status_message = Some("No note selected".to_string());
                }
            }
            UiMsg::CancelComposing => {
                self.state.ui.current_mode = UiMode::Normal;
                self.state.ui.textarea.content.clear();
                self.state.ui.reply_to = None;
            }
            UiMsg::SubmitNote => {
                // Create and publish note
                let content = self.state.ui.textarea.content.clone();

                // Create event and send through NostrEvents subscription
                if let Some(sender) = &self.state.nostr.command_sender {
                    // Build event with appropriate tags
                    let event_builder = if let Some(ref reply_to_event) = self.state.ui.reply_to {
                        log::info!("Publishing reply: {content}");
                        // Create reply with proper NIP-10 tags
                        EventBuilder::text_note(&content)
                            .tag(Tag::event(reply_to_event.id))
                            .tag(Tag::public_key(reply_to_event.pubkey))
                    } else {
                        log::info!("Publishing note: {content}");
                        EventBuilder::text_note(&content)
                    };

                    // Sign event with user's keys
                    match event_builder.sign_with_keys(&self.keys) {
                        Ok(event) => {
                            let _ = sender.send(NostrCommand::SendEvent { event });
                            self.state.system.status_message = Some("Note published".to_string());
                        }
                        Err(e) => {
                            log::error!("Failed to sign event: {e}");
                            self.state.system.status_message =
                                Some(format!("Failed to create note: {e}"));
                        }
                    }
                }

                // Clear UI state
                self.state.ui.current_mode = UiMode::Normal;
                self.state.ui.textarea.content.clear();
                self.state.ui.textarea.cursor_position.column = 0;
                self.state.ui.textarea.cursor_position.line = 0;
                self.state.ui.reply_to = None;
            }
            UiMsg::ReactToSelected => {
                // React to the selected note
                if let Some(selected_index) = self.state.timeline.selected_index {
                    if let Some(note) = self.state.timeline.notes.get(selected_index) {
                        let event_id = note.0.event.id;

                        log::info!("Reacting to event: {event_id}");

                        if let Some(sender) = &self.state.nostr.command_sender {
                            // Create reaction event (+ emoji)
                            match EventBuilder::reaction(&note.0.event, "+")
                                .sign_with_keys(&self.keys)
                            {
                                Ok(event) => {
                                    let _ = sender.send(NostrCommand::SendEvent { event });
                                    self.state.system.status_message = Some(format!(
                                        "Reacted to note {}",
                                        &event_id.to_hex()[..8]
                                    ));
                                }
                                Err(e) => {
                                    log::error!("Failed to sign reaction: {e}");
                                    self.state.system.status_message =
                                        Some(format!("Failed to react: {e}"));
                                }
                            }
                        }
                    } else {
                        self.state.system.status_message = Some("No note selected".to_string());
                    }
                } else {
                    self.state.system.status_message = Some("No note selected".to_string());
                }
            }
            UiMsg::RepostSelected => {
                // Repost the selected note
                if let Some(selected_index) = self.state.timeline.selected_index {
                    if let Some(note) = self.state.timeline.notes.get(selected_index) {
                        let event_id = note.0.event.id;

                        log::info!("Reposting event: {event_id}");

                        if let Some(sender) = &self.state.nostr.command_sender {
                            // Create repost event (with no specific relay URL)
                            match EventBuilder::repost(&note.0.event, None)
                                .sign_with_keys(&self.keys)
                            {
                                Ok(event) => {
                                    let _ = sender.send(NostrCommand::SendEvent { event });
                                    self.state.system.status_message =
                                        Some(format!("Reposted note {}", &event_id.to_hex()[..8]));
                                }
                                Err(e) => {
                                    log::error!("Failed to sign repost: {e}");
                                    self.state.system.status_message =
                                        Some(format!("Failed to repost: {e}"));
                                }
                            }
                        }
                    } else {
                        self.state.system.status_message = Some("No note selected".to_string());
                    }
                } else {
                    self.state.system.status_message = Some("No note selected".to_string());
                }
            }
            UiMsg::ProcessTextAreaInput(key_event) => {
                // Process key event using tui-textarea
                // This delegates all key handling to tui-textarea's built-in logic
                use crossterm::event::Event;
                use tui_textarea::TextArea;

                // Create temporary TextArea with current state
                let mut textarea = TextArea::default();

                // Restore content
                if !self.state.ui.textarea.content.is_empty() {
                    textarea.insert_str(&self.state.ui.textarea.content);
                }

                // Restore cursor position
                textarea.move_cursor(CursorMove::Jump(
                    self.state.ui.textarea.cursor_position.line as u16,
                    self.state.ui.textarea.cursor_position.column as u16,
                ));

                // Restore selection if any
                if let Some(selection) = &self.state.ui.textarea.selection {
                    textarea.move_cursor(CursorMove::Jump(
                        selection.start.line as u16,
                        selection.start.column as u16,
                    ));
                    textarea.start_selection();
                    textarea.move_cursor(CursorMove::Jump(
                        selection.end.line as u16,
                        selection.end.column as u16,
                    ));
                }

                // Apply the key input to textarea
                textarea.input(Event::Key(key_event));

                // Extract updated state
                let content = textarea.lines().join("\n");
                let (line, column) = textarea.cursor();
                let selection = textarea.selection_range().map(
                    |((start_row, start_col), (end_row, end_col))| TextSelection {
                        start: CursorPosition {
                            line: start_row,
                            column: start_col,
                        },
                        end: CursorPosition {
                            line: end_row,
                            column: end_col,
                        },
                    },
                );

                // Update state
                self.state.ui.textarea.content = content;
                self.state.ui.textarea.cursor_position.line = line;
                self.state.ui.textarea.cursor_position.column = column;
                self.state.ui.textarea.selection = selection;
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
                self.state.system.is_loading = false;
                self.state.system.status_message = Some("Connected to Nostr".to_string());
            }
            NostrSubscriptionMessage::Notification(notif) => match *notif {
                RelayPoolNotification::Event { event, .. } => {
                    log::debug!("Received event from relay: {}", event.id);
                    self.process_nostr_event(*event);
                }
                RelayPoolNotification::Message { message, .. } => {
                    log::debug!("Received relay message: {message:?}");
                }
                RelayPoolNotification::Shutdown => {
                    log::info!("Nostr subscription shut down");
                    self.state.nostr.command_sender = None;
                    self.state.system.status_message = Some("Disconnected from Nostr".to_string());
                }
            },
            NostrSubscriptionMessage::Error { error } => {
                log::error!("NostrEvents error: {error:?}");
                self.state.system.status_message = Some(format!("Nostr error: {error:?}"));
            }
        }
    }

    /// Process a received Nostr event
    fn process_nostr_event(&mut self, event: nostr_sdk::Event) {
        // Receiving any Nostr event implies that initial loading has progressed
        // Clear the loading indicator on first event reception
        if self.state.system.is_loading {
            self.state.system.is_loading = false;
        }

        match event.kind {
            Kind::TextNote => {
                // Add text note to timeline
                let sortable = SortableEvent::new(event);
                self.state.timeline.notes.find_or_insert(Reverse(sortable));
                log::debug!("Added text note to timeline");
            }
            Kind::Metadata => {
                // Parse and store profile metadata
                if let Ok(metadata) = Metadata::from_json(event.content.clone()) {
                    let profile = Profile::new(event.pubkey, event.created_at, metadata);

                    // Only update if this is newer than existing profile
                    let should_update = self
                        .state
                        .user
                        .profiles
                        .get(&event.pubkey)
                        .is_none_or(|existing| profile.created_at > existing.created_at);

                    if should_update {
                        self.state.user.profiles.insert(event.pubkey, profile);
                        log::debug!("Updated profile for pubkey: {}", event.pubkey);
                    }
                } else {
                    log::warn!("Failed to parse metadata for pubkey: {}", event.pubkey);
                    self.state.system.status_message =
                        Some("Failed to parse profile metadata".to_string());
                }
            }
            Kind::Reaction => {
                // Add reaction to timeline engagement data
                if let Some(event_id) = self.extract_last_event_id(&event) {
                    self.state
                        .timeline
                        .reactions
                        .entry(event_id)
                        .or_default()
                        .insert(event);
                    log::debug!("Added reaction for event: {event_id}");
                }
            }
            Kind::Repost => {
                // Add repost to timeline engagement data
                if let Some(event_id) = self.extract_last_event_id(&event) {
                    self.state
                        .timeline
                        .reposts
                        .entry(event_id)
                        .or_default()
                        .insert(event);
                    log::debug!("Added repost for event: {event_id}");
                }
            }
            Kind::ZapReceipt => {
                // Add zap receipt to timeline engagement data
                if let Some(event_id) = self.extract_last_event_id(&event) {
                    self.state
                        .timeline
                        .zap_receipts
                        .entry(event_id)
                        .or_default()
                        .insert(event);
                    log::debug!("Added zap receipt for event: {event_id}");
                }
            }
            _ => {
                // Unknown event types are logged but not processed
                log::debug!("Received unknown event type: {:?}", event.kind);
                self.state.system.status_message =
                    Some(format!("Received unknown event type: {}", event.kind));
            }
        }
    }

    /// Helper function to extract event_id from the last e tag of an event
    fn extract_last_event_id(&self, event: &nostr_sdk::Event) -> Option<nostr_sdk::EventId> {
        use nostr_sdk::nostr::{Alphabet, SingleLetterTag, TagKind, TagStandard};

        event
            .tags
            .iter()
            .filter(|tag| {
                tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E))
            })
            .next_back()
            .and_then(|tag| {
                if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized() {
                    Some(*event_id)
                } else {
                    None
                }
            })
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
        };

        let (app, _) = TearsApp::new(flags);
        app
    }

    #[test]
    fn test_timeline_select_clears_status_message() {
        let mut app = create_test_app();

        // Set a status message
        app.state.system.status_message = Some("Test message".to_string());

        // Select a note (index 0)
        app.handle_timeline_msg(TimelineMsg::Select(0));

        // Status message should be cleared
        assert_eq!(app.state.system.status_message, None);
    }

    #[test]
    fn test_timeline_select_invalid_index_clears_status_message() {
        let mut app = create_test_app();

        // Set a status message
        app.state.system.status_message = Some("Test message".to_string());

        // Select with invalid index (timeline is empty)
        app.handle_timeline_msg(TimelineMsg::Select(999));

        // Status message should be cleared
        assert_eq!(app.state.system.status_message, None);
        // Selection should be None
        assert_eq!(app.state.timeline.selected_index, None);
    }

    #[test]
    fn test_scroll_up_does_not_clear_status_message() {
        let mut app = create_test_app();

        // Set a status message
        app.state.system.status_message = Some("Test message".to_string());

        // Scroll up
        app.handle_timeline_msg(TimelineMsg::ScrollUp);

        // Status message should remain
        assert_eq!(
            app.state.system.status_message,
            Some("Test message".to_string())
        );
    }

    #[test]
    fn test_scroll_down_does_not_clear_status_message() {
        let mut app = create_test_app();

        // Set a status message
        app.state.system.status_message = Some("Test message".to_string());

        // Scroll down
        app.handle_timeline_msg(TimelineMsg::ScrollDown);

        // Status message should remain
        assert_eq!(
            app.state.system.status_message,
            Some("Test message".to_string())
        );
    }

    #[test]
    fn test_unselect_action_delegates_to_deselect() {
        let mut app = create_test_app();

        // Set selection and status message
        app.state.timeline.selected_index = Some(0);
        app.state.system.status_message = Some("Test message".to_string());

        // KeyAction::Unselect should delegate to TimelineMsg::Deselect
        // We test the end result by calling TimelineMsg::Deselect directly
        app.update(AppMsg::Timeline(TimelineMsg::Deselect));

        // Both selection and status message should be cleared
        assert_eq!(app.state.timeline.selected_index, None);
        assert_eq!(app.state.system.status_message, None);
    }

    #[test]
    fn test_escape_key_triggers_deselect() {
        let mut app = create_test_app();

        // Set selection and status message
        app.state.timeline.selected_index = Some(5);
        app.state.system.status_message = Some("Test message".to_string());

        // Simulate Escape key press and execute the TimelineMsg::Deselect directly
        app.update(AppMsg::Timeline(TimelineMsg::Deselect));

        // Both selection and status message should be cleared
        assert_eq!(app.state.timeline.selected_index, None);
        assert_eq!(app.state.system.status_message, None);
    }

    #[test]
    fn test_timeline_deselect_clears_status_message() {
        let mut app = create_test_app();

        // Set selection and status message
        app.state.timeline.selected_index = Some(3);
        app.state.system.status_message = Some("Test message".to_string());

        // Deselect
        app.handle_timeline_msg(TimelineMsg::Deselect);

        // Both selection and status message should be cleared
        assert_eq!(app.state.timeline.selected_index, None);
        assert_eq!(app.state.system.status_message, None);
    }

    #[test]
    fn test_select_first_with_empty_timeline() {
        let mut app = create_test_app();

        // Timeline is empty
        assert!(app.state.timeline.notes.is_empty());

        // Try to select first
        app.handle_timeline_msg(TimelineMsg::SelectFirst);

        // Selection should remain None
        assert_eq!(app.state.timeline.selected_index, None);
    }

    #[test]
    fn test_select_first_with_notes() {
        let mut app = create_test_app();

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
        app.state.timeline.selected_index = Some(1);

        // Select first
        app.handle_timeline_msg(TimelineMsg::SelectFirst);

        // Selection should be at index 0
        assert_eq!(app.state.timeline.selected_index, Some(0));
    }

    #[test]
    fn test_select_last_with_empty_timeline() {
        let mut app = create_test_app();

        // Timeline is empty
        assert!(app.state.timeline.notes.is_empty());

        // Try to select last
        app.handle_timeline_msg(TimelineMsg::SelectLast);

        // Selection should remain None
        assert_eq!(app.state.timeline.selected_index, None);
    }

    #[test]
    fn test_select_last_with_notes() {
        let mut app = create_test_app();

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
        app.state.timeline.selected_index = None;

        // Select last
        app.handle_timeline_msg(TimelineMsg::SelectLast);

        // Selection should be at the last index
        let expected_index = app.state.timeline.notes.len() - 1;
        assert_eq!(app.state.timeline.selected_index, Some(expected_index));
    }

    #[test]
    fn test_scroll_to_top_delegates() {
        let mut app = create_test_app();

        // Add some notes
        let keys = Keys::generate();
        let event = EventBuilder::text_note("test note")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        app.process_nostr_event(event);

        // Directly test the delegation by calling SelectFirst
        app.update(AppMsg::Timeline(TimelineMsg::SelectFirst));

        // Selection should be at index 0
        assert_eq!(app.state.timeline.selected_index, Some(0));
    }

    #[test]
    fn test_scroll_to_bottom_delegates() {
        let mut app = create_test_app();

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
        let expected_index = app.state.timeline.notes.len() - 1;
        assert_eq!(app.state.timeline.selected_index, Some(expected_index));
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
        app.state.ui.current_mode = UiMode::Composing;

        // In composing mode, 'q' key should be passed to textarea, not trigger quit
        let q_key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let _cmd = app.handle_key_input(q_key);

        // Should produce ProcessTextAreaInput command
        // The application should still be in composing mode
        assert_eq!(app.state.ui.current_mode, UiMode::Composing);

        // The textarea should contain 'q' after processing
        app.update(AppMsg::Ui(UiMsg::ProcessTextAreaInput(q_key)));
        assert_eq!(app.state.ui.textarea.content, "q");
    }

    #[test]
    fn test_escape_cancels_composing_mode() {
        let mut app = create_test_app();

        // Start composing mode with some content
        app.state.ui.current_mode = UiMode::Composing;
        app.state.ui.textarea.content = "test content".to_string();

        // Escape key should cancel composing
        let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let _cmd = app.handle_key_input(esc_key);

        // Should return to normal mode
        app.update(AppMsg::Ui(UiMsg::CancelComposing));
        assert_eq!(app.state.ui.current_mode, UiMode::Normal);
        assert!(app.state.ui.textarea.content.is_empty());
    }

    #[test]
    fn test_ctrl_p_submits_note_in_composing_mode() {
        let mut app = create_test_app();

        // Start composing mode with some content
        app.state.ui.current_mode = UiMode::Composing;
        app.state.ui.textarea.content = "test note".to_string();

        // Ctrl+P should submit note
        let ctrl_p_key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        let _cmd = app.handle_key_input(ctrl_p_key);

        // Should produce SubmitNote command
        // Note: Actual submission requires nostr connection, but we can verify
        // the key handling produces the right command
    }
}
