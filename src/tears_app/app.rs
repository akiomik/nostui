//! Main Tears Application implementation

use std::cell::RefCell;
use std::sync::Arc;

use nostr_sdk::prelude::*;
use ratatui::prelude::*;
use tears::prelude::*;
use tears::subscription::time::{Message as TimerMessage, Timer};

use crate::core::state::{ui::UiMode, AppState};
use crate::domain::ui::{CursorPosition, TextSelection};
use crate::infrastructure::config::Config;
use crate::tears::subscription::nostr::{
    Message as NostrSubscriptionMessage, NostrCommand, NostrEvents,
};

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
}

impl<'a> Application for TearsApp<'a> {
    type Message = AppMsg;
    type Flags = InitFlags;

    fn new(flags: InitFlags) -> (Self, Command<Self::Message>) {
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
        };

        // Return initial commands if needed
        // For now, no initial commands
        (app, Command::none())
    }

    fn update(&mut self, msg: AppMsg) -> Command<Self::Message> {
        log::debug!("update: {msg:?}");

        // Track app FPS (every update call counts as a frame)
        if let Some(fps) = self.app_fps_tracker.record_frame() {
            self.state.system.fps_data.app_fps = fps;
        }

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
        vec![
            // NostrEvents subscription - reuse the same Arc<Client> across frames
            // This ensures the subscription ID remains constant and the subscription
            // is not recreated every frame
            Subscription::new(NostrEvents::new(Arc::clone(&self.nostr_client)))
                .map(|msg| AppMsg::Nostr(super::message::NostrMsg::SubscriptionMessage(msg))),
            // Timer subscription - matches runtime FPS (60 Hz = ~16.67ms)
            Subscription::new(Timer::new(16)).map(|msg| match msg {
                TimerMessage::Tick => AppMsg::System(SystemMsg::Tick),
            }),
            // TODO: Add TerminalEvents subscription
            // Currently disabled due to crossterm version conflicts
            // - TerminalEvents for keyboard/mouse input
        ]
    }
}

impl<'a> TearsApp<'a> {
    /// Handle system messages
    fn handle_system_msg(&mut self, msg: SystemMsg) -> Command<AppMsg> {
        match msg {
            SystemMsg::Quit => {
                // Set a flag or handle quit
                // For now, just log
                log::info!("Quit requested");
            }
            SystemMsg::Resize(width, height) => {
                log::debug!("Terminal resized to {width}x{height}");
                // Terminal resize is handled automatically by ratatui
            }
            SystemMsg::Tick => {
                // Tick events are used for periodic operations
                // FPS tracking is done in update() method
            }
            SystemMsg::ShowError(error) => {
                self.state.system.status_message = Some(error);
            }
        }
        Command::none()
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
            }
            UiMsg::CancelComposing => {
                self.state.ui.current_mode = UiMode::Normal;
                self.state.ui.textarea.content.clear();
            }
            UiMsg::SubmitNote => {
                // Create and publish note
                let content = self.state.ui.textarea.content.clone();
                log::info!("Publishing note: {content}");

                // Create event and send through NostrEvents subscription
                if let Some(sender) = &self.state.nostr.command_sender {
                    // Sign event with user's keys
                    match EventBuilder::text_note(&content).sign_with_keys(&self.keys) {
                        Ok(event) => {
                            let _ = sender.send(NostrCommand::SendEvent { event });
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
                textarea.move_cursor(tui_textarea::CursorMove::Jump(
                    self.state.ui.textarea.cursor_position.line as u16,
                    self.state.ui.textarea.cursor_position.column as u16,
                ));

                // Restore selection if any
                if let Some(selection) = &self.state.ui.textarea.selection {
                    textarea.move_cursor(tui_textarea::CursorMove::Jump(
                        selection.start.line as u16,
                        selection.start.column as u16,
                    ));
                    textarea.start_selection();
                    textarea.move_cursor(tui_textarea::CursorMove::Jump(
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
    fn handle_nostr_msg(&mut self, msg: super::message::NostrMsg) -> Command<AppMsg> {
        match msg {
            super::message::NostrMsg::Connect => {
                // NostrEvents subscription handles connection automatically
                log::info!("NostrEvents subscription will handle connection");
            }
            super::message::NostrMsg::Disconnect => {
                // Send shutdown command through the sender
                if let Some(sender) = &self.state.nostr.command_sender {
                    let _ = sender.send(NostrCommand::Shutdown);
                }
            }
            super::message::NostrMsg::EventReceived(event) => {
                log::debug!("Received event: {}", event.id);
                self.process_nostr_event(*event);
            }
            super::message::NostrMsg::SubscriptionMessage(sub_msg) => {
                self.handle_nostr_subscription_message(sub_msg);
            }
        }
        Command::none()
    }

    /// Handle NostrEvents subscription messages
    fn handle_nostr_subscription_message(&mut self, msg: NostrSubscriptionMessage) {
        use nostr_sdk::RelayPoolNotification;

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
        use crate::domain::nostr::{Profile, SortableEvent};
        use std::cmp::Reverse;

        // Receiving any Nostr event implies that initial loading has progressed
        // Clear the loading indicator on first event reception
        if self.state.system.is_loading {
            self.state.system.is_loading = false;
        }

        match event.kind {
            nostr_sdk::Kind::TextNote => {
                // Add text note to timeline
                let sortable = SortableEvent::new(event);
                self.state.timeline.notes.find_or_insert(Reverse(sortable));
                log::debug!("Added text note to timeline");
            }
            nostr_sdk::Kind::Metadata => {
                // Parse and store profile metadata
                if let Ok(metadata) = nostr_sdk::Metadata::from_json(event.content.clone()) {
                    let profile = Profile::new(event.pubkey, event.created_at, metadata);
                    
                    // Only update if this is newer than existing profile
                    let should_update = self.state.user.profiles.get(&event.pubkey).is_none_or(|existing| {
                        profile.created_at > existing.created_at
                    });
                    
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
            nostr_sdk::Kind::Reaction => {
                // Add reaction to timeline engagement data
                if let Some(event_id) = self.extract_last_event_id(&event) {
                    self.state.timeline.reactions
                        .entry(event_id)
                        .or_default()
                        .insert(event);
                    log::debug!("Added reaction for event: {event_id}");
                }
            }
            nostr_sdk::Kind::Repost => {
                // Add repost to timeline engagement data
                if let Some(event_id) = self.extract_last_event_id(&event) {
                    self.state.timeline.reposts
                        .entry(event_id)
                        .or_default()
                        .insert(event);
                    log::debug!("Added repost for event: {event_id}");
                }
            }
            nostr_sdk::Kind::ZapReceipt => {
                // Add zap receipt to timeline engagement data
                if let Some(event_id) = self.extract_last_event_id(&event) {
                    self.state.timeline.zap_receipts
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
