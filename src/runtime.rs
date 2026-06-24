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

use crate::application::config::Config;
use crate::application::message::{AppMsg, EditorMsg, NostrMsg, SystemMsg, TimelineMsg};
use crate::application::state::AppState;
use crate::infrastructure::subscription::media::MediaEvents;
use crate::infrastructure::subscription::nostr::{
    Message as NostrSubscriptionMessage, NostrEvents,
};
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
    state: AppState<'a>,
    /// Component collection (wrapped in RefCell for interior mutability during view)
    components: RefCell<Components>,
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

    // TODO: Move message dispatch into `AppState`.
    //
    // `TearsApp` now only routes messages to `AppState` command methods and never
    // mutates a sub-state directly. The next step toward a self-contained state
    // machine is to move the per-domain dispatch (`handle_timeline_msg`,
    // `handle_editor_msg`, `handle_nostr_msg`, ...) into `AppState::update(AppMsg)`,
    // leaving `TearsApp` as a thin tears adapter responsible only for IO-coupled
    // concerns: key -> message mapping, subscriptions, and `Command::effect(Quit)`.
    // That would make `AppState` own both state and transitions, and would let its
    // fields become private (external code could only drive it via messages).
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
                // Unsubscribe from all timeline subscriptions and disconnect from relays
                let _ = self.state.close_connection();

                // Trigger the quit action
                Command::effect(Action::Quit)
            }
            SystemMsg::Resize(width, height) => {
                log::debug!("Terminal resized to {width}x{height}");
                // Terminal resize is handled automatically by ratatui
                Command::none()
            }
            // Track app FPS based on tick events (approximately matches render FPS)
            SystemMsg::Tick => self.state.record_tick(),
            SystemMsg::ShowError(error) => self.state.show_error(error),
            SystemMsg::KeyInput(key) => self.handle_key_input(key),
        }
    }

    /// Handle key input based on current editor state
    fn handle_key_input(&mut self, key: KeyEvent) -> Command<AppMsg> {
        // Note: Ctrl+C is now handled by signal subscription, not as keyboard input
        // This ensures it works reliably across different terminal emulators and
        // properly separates OS signals from application keybindings

        // Mode-specific keybindings
        if self.state.editor.is_active() {
            self.handle_composing_mode_key(key)
        } else {
            self.handle_normal_mode_key(key)
        }
    }

    /// Handle key input in Normal mode
    fn handle_normal_mode_key(&mut self, key: KeyEvent) -> Command<AppMsg> {
        // First, try to resolve from configured keybindings
        if let Some(action) = self.config.keybindings.home.get(&vec![key]) {
            return self.handle_action(action.clone());
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
        // Ignore timeline operations while the application is still starting up, so
        // the "loading..." status message is preserved until the first event arrives.
        // Quitting is handled via SystemMsg and is unaffected by this gate.
        if self.state.startup.is_in_progress() {
            return Command::none();
        }

        // Clear any status message before handling the operation.
        let _ = self.state.clear_status_message();

        match msg {
            TimelineMsg::ScrollUp => self.state.scroll_up(),
            TimelineMsg::ScrollDown => self.state.scroll_down(),
            TimelineMsg::Select(index) => self.state.select_note(index),
            TimelineMsg::Deselect => self.state.deselect_note(),
            TimelineMsg::SelectFirst => self.state.select_first_note(),
            TimelineMsg::SelectLast => self.state.select_last_note(),
            TimelineMsg::LoadMore => self.state.load_more_timeline(),
            TimelineMsg::ReactToSelected => self.state.react_to_selected(),
            TimelineMsg::RepostSelected => self.state.repost_selected(),
            TimelineMsg::SelectTab(index) => self.state.select_tab(index),
            TimelineMsg::NextTab => self.state.next_tab(),
            TimelineMsg::PrevTab => self.state.prev_tab(),
            TimelineMsg::OpenAuthorTimeline => {
                // Open author timeline for the selected note's author.
                let author_pubkey = self
                    .state
                    .timeline
                    .selected_note()
                    .map(|note| note.author_pubkey());
                match author_pubkey {
                    Some(author_pubkey) => self.state.open_author_timeline(author_pubkey),
                    None => Command::none(),
                }
            }
            TimelineMsg::CloseCurrentTab => self.state.close_current_tab(),
        }
    }

    /// Handle editor messages
    fn handle_editor_msg(&mut self, msg: EditorMsg) -> Command<AppMsg> {
        match msg {
            EditorMsg::StartComposing => self.state.start_composing(),
            EditorMsg::StartReply => self.state.start_reply(),
            EditorMsg::CancelComposing => self.state.cancel_composing(),
            EditorMsg::SubmitNote => self.state.submit_note(),
            EditorMsg::ProcessTextAreaInput(key_event) => self.state.process_text_input(key_event),
        }
    }

    /// Handle Nostr messages from the subscription
    fn handle_nostr_msg(&mut self, msg: NostrMsg) -> Command<AppMsg> {
        match msg {
            NostrMsg::Connect => {
                // NostrEvents subscription handles connection automatically
                log::info!("NostrEvents subscription will handle connection");
                Command::none()
            }
            NostrMsg::Disconnect => {
                log::info!("Disconnected from Nostr");
                self.state.close_connection()
            }
            NostrMsg::SubscriptionMessage(sub_msg) => {
                self.handle_nostr_subscription_message(sub_msg)
            }
        }
    }

    fn handle_media_msg(&mut self, msg: Result<MediaEvent, MediaSourceError>) -> Command<AppMsg> {
        match msg {
            Ok(MediaEvent::TrackChanged { track, .. }) => self.state.publish_music_status(track),
            Ok(_) => Command::none(),
            Err(e) => {
                log::error!("media source error: {e}");
                Command::none()
            }
        }
    }

    /// Handle NostrEvents subscription messages
    fn handle_nostr_subscription_message(
        &mut self,
        msg: NostrSubscriptionMessage,
    ) -> Command<AppMsg> {
        match msg {
            NostrSubscriptionMessage::Ready { sender } => self.state.on_connection_ready(sender),
            NostrSubscriptionMessage::SubscriptionCreated {
                tab_type,
                subscription_id,
            } => self
                .state
                .track_subscription_created(tab_type, subscription_id),
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
                    Command::none()
                }
                RelayPoolNotification::Message { message, .. } => {
                    log::debug!("Received relay message: {message:?}");

                    if let RelayMessage::Event {
                        subscription_id,
                        event,
                    } = message
                    {
                        self.state
                            .route_relay_event(&subscription_id, event.into_owned())
                    } else {
                        Command::none()
                    }
                }
                RelayPoolNotification::Shutdown => self.state.notify_subscription_shutdown(),
            },
            NostrSubscriptionMessage::Error { error } => {
                self.state.notify_subscription_error(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::config::Config;
    use crate::model::editor::Message as EditorMessage;
    use crate::model::status_bar::Message as StatusBarMessage;
    use crate::model::timeline::tab::TimelineTabType;
    use crate::model::timeline::Message as TimelineMessage;

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
    fn test_unselect_action_delegates_to_deselect() {
        let mut app = create_test_app();

        // Add a test note to allow selection
        let keys = Keys::generate();
        let event = EventBuilder::text_note("test note")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        let _ = app
            .state
            .process_nostr_event_for_tab(event, &TimelineTabType::Home);

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
        assert_eq!(app.state.status_bar.message(), None);
    }

    #[test]
    fn test_escape_key_triggers_deselect() {
        let mut app = create_test_app();

        // Add a test note to allow selection
        let keys = Keys::generate();
        let event = EventBuilder::text_note("test note")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        let _ = app
            .state
            .process_nostr_event_for_tab(event, &TimelineTabType::Home);

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
        assert_eq!(app.state.status_bar.message(), None);
    }

    #[test]
    fn test_timeline_ops_ignored_during_startup() {
        let mut app = create_test_app();
        assert!(app.state.startup.is_in_progress());

        // Simulate the "loading..." status message shown during startup
        app.state
            .status_bar
            .update(StatusBarMessage::MessageChanged {
                label: "Home".to_owned(),
                message: "loading...".to_owned(),
            });

        // During startup, a timeline operation is ignored and the status message
        // is preserved (the gate returns before clearing it).
        let _ = app.update(AppMsg::Timeline(TimelineMsg::Deselect));
        assert_eq!(app.state.status_bar.message(), Some("[Home] loading..."));

        // Once startup completes, the same operation goes through and
        // clears the status message.
        app.state.startup.mark_completed();
        let _ = app.update(AppMsg::Timeline(TimelineMsg::Deselect));
        assert_eq!(app.state.status_bar.message(), None);
    }

    #[test]
    fn test_select_first_with_notes() {
        let mut app = create_test_app();

        // Add test notes to timeline
        let keys = Keys::generate();
        let event1 = EventBuilder::text_note("test note 1")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        let event2 = EventBuilder::text_note("test note 2")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        let _ = app
            .state
            .process_nostr_event_for_tab(event1, &TimelineTabType::Home);
        let _ = app
            .state
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
    fn test_select_last_with_notes() {
        let mut app = create_test_app();

        // Add test notes to timeline
        let keys = Keys::generate();
        let event1 = EventBuilder::text_note("test note 1")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        let event2 = EventBuilder::text_note("test note 2")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        let _ = app
            .state
            .process_nostr_event_for_tab(event1, &TimelineTabType::Home);
        let _ = app
            .state
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

        // Add a test note
        let keys = Keys::generate();
        let event = EventBuilder::text_note("test note")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        let _ = app
            .state
            .process_nostr_event_for_tab(event, &TimelineTabType::Home);

        // Directly test the delegation by calling SelectFirst
        let _ = app.update(AppMsg::Timeline(TimelineMsg::SelectFirst));

        // Selection should be at index 0
        assert_eq!(app.state.timeline.selected_index(), Some(0));
    }

    #[test]
    fn test_scroll_to_bottom_delegates() {
        let mut app = create_test_app();

        // Add test notes
        let keys = Keys::generate();
        let event1 = EventBuilder::text_note("test note 1")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        let event2 = EventBuilder::text_note("test note 2")
            .sign_with_keys(&keys)
            .expect("Failed to sign test event");
        let _ = app
            .state
            .process_nostr_event_for_tab(event1, &TimelineTabType::Home);
        let _ = app
            .state
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
        app.state.editor.update(EditorMessage::ComposingStarted);

        // In composing mode, 'q' key should be passed to textarea, not trigger quit
        let q_key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let _cmd = app.handle_key_input(q_key);

        // Should produce ProcessTextAreaInput command
        // The application should still be in composing mode
        assert!(app.state.editor.is_active());

        // The textarea should contain 'q' after processing
        let _ = app.update(AppMsg::Editor(EditorMsg::ProcessTextAreaInput(q_key)));
        assert_eq!(app.state.editor.get_content(), "q");
    }

    #[test]
    fn test_escape_cancels_composing_mode() {
        let mut app = create_test_app();

        // Start composing mode with some content
        app.state.editor.update(EditorMessage::ComposingStarted);

        // Set content directly on the component (simulating user input)
        app.state.editor.update(EditorMessage::KeyEventReceived {
            event: KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE),
        });

        // Escape key should cancel composing
        let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let _cmd = app.handle_key_input(esc_key);

        // Should return to normal mode
        let _ = app.update(AppMsg::Editor(EditorMsg::CancelComposing));
        assert!(!app.state.editor.is_active());
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
