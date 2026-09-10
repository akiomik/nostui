//! Main Tears Application implementation

use std::cell::RefCell;
use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use nostr_sdk::prelude::*;
use nowhear::{MediaEvent, MediaSourceError};
use ratatui::prelude::*;
use tears::prelude::*;
use tears::subscription::terminal::TerminalEvents;

use crate::application::config::keybindings::Action as KeyAction;
use crate::application::config::Config;
use crate::application::message::{AppMsg, EditorMsg, NostrMsg, SystemMsg, TimelineMsg};
use crate::application::state::AppState;
use crate::infrastructure::subscription::media::MediaEvents;
use crate::infrastructure::subscription::nostr::NostrEvents;
use crate::model::nostr_gateway::Message as NostrSubscriptionMessage;
use crate::presentation::components::Components;

/// Initialization flags for the Tears application
#[derive(Debug)]
pub struct InitFlags {
    pub pubkey: PublicKey,
    pub keys: Option<Keys>,
    pub config: Config,
    pub nostr_client: Client,
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
    /// Current account's public key, used for read-only subscriptions.
    pubkey: PublicKey,
    /// Private keys used to sign outbound events, if the app is not in read-only mode.
    keys: Option<Keys>,
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
        // Without signing keys the application can only read, and it needs to know that
        // before it attempts a publish nothing could ever complete.
        let read_only = flags.keys.is_none();
        let state = AppState::new_with_config(flags.pubkey, flags.config, read_only);

        // Initialize components
        let components = Components::new();

        // Wrap client in Arc for sharing across subscriptions
        // This ensures subscription identity remains constant
        let nostr_client = Arc::new(flags.nostr_client);

        let app = Self {
            state,
            components: RefCell::new(components),
            nostr_client,
            pubkey: flags.pubkey,
            config,
            keys: flags.keys,
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
    // concerns: key -> message mapping, subscriptions, and `Command::quit()`.
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
            Subscription::new(NostrEvents::new(
                Arc::clone(&self.nostr_client),
                self.pubkey,
                self.keys.clone(),
            ))
            .map(|msg| AppMsg::Nostr(NostrMsg::SubscriptionMessage(msg))),
            Subscription::new(TerminalEvents::new()).map(|result| match result {
                Ok(event) => terminal_event_to_msg(event),
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

/// Map one terminal event to the message it should become.
///
/// A key event is input when the key is going down. A Windows console reports releases
/// too — crossterm takes the kind from the record's `key_down` flag — and nostui acted
/// on some of them. Not the configured bindings, whose map is keyed by `KeyEvent` and so
/// compares the kind; the two paths that read the key's code alone. Cancelling a draft
/// with `Esc` closed the composer on the press, and the release then landed in normal
/// mode, whose fallback reads `code` and fires `Deselect` (#531).
///
/// A repeat is the key still being down, so it becomes input too — and becomes a press
/// on the way, because none of the three paths downstream has any use for the
/// difference and each of them handled it differently before (#536). The binding map
/// compares the kind and its entries are all `Press`, so a repeat used to match nothing
/// and holding a key did nothing; the composer and normal mode's fallback read the code
/// alone and so were already treating the two alike. Normalising here is what makes all
/// three agree, rather than three separate opinions about a distinction nobody wants.
///
/// It does mean nothing downstream can tell a held key from a fresh press. That is the
/// point, and it is where to come back if anything ever needs to — refusing to repeat a
/// destructive action, say. Nothing reports a repeat today in any case: only the kitty
/// keyboard protocol does, and nostui never asks for it.
///
/// Not every release is a key coming up. crossterm reports a Windows Alt code as a
/// `Release` carrying the composed character, so the arm below drops it; it never typed
/// anything before either, since `tui-textarea` discarded it further down. Making it
/// work is #537, and it starts here.
///
/// A free function rather than the closure it replaces: the closure lived inside
/// `subscriptions`, which no test drives.
fn terminal_event_to_msg(event: Event) -> AppMsg {
    match event {
        Event::Key(key) => match key.kind {
            KeyEventKind::Press | KeyEventKind::Repeat => {
                AppMsg::System(SystemMsg::KeyInput(KeyEvent {
                    kind: KeyEventKind::Press,
                    ..key
                }))
            }
            KeyEventKind::Release => AppMsg::System(SystemMsg::TerminalEventIgnored),
        },
        Event::Resize(width, height) => AppMsg::System(SystemMsg::Resize(width, height)),
        _ => AppMsg::System(SystemMsg::TerminalEventIgnored),
    }
}

impl<'a> TearsApp<'a> {
    /// Handle system messages
    fn handle_system_msg(&mut self, msg: SystemMsg) -> Command<AppMsg> {
        match msg {
            SystemMsg::Quit => {
                log::info!("Quit requested - initiating graceful shutdown");

                // Queues at most `NostrCommand::Shutdown` — never a per-subscription
                // `CLOSE`, since disconnecting ends them at the relay anyway — and nothing
                // at all if the gateway is already disconnected.
                //
                // This cannot cut a publish short, even though the worker handles
                // `Shutdown` by disconnecting and a disconnect does abort a pending `OK`
                // wait. Every command shares one FIFO channel and the worker awaits each
                // one inline, so while a `SendEventBuilder` is in flight the loop is inside
                // `handle_command` rather than at its `select!`, and a `Shutdown` queued
                // behind it is not dequeued until that send resolves.
                //
                // What the send is not safe from is the exit itself. The quit applies
                // synchronously on tears 0.11, so `run` can return while the worker — a
                // detached task the runtime neither owns nor joins — is still publishing,
                // and the tokio runtime is dropped moments later. The status bar says
                // "Sending" at that point rather than claiming success, so the user is at
                // least not told it worked; #512 covers giving the send a chance to land.
                let _ = self.state.close_connection();

                Command::quit()
            }
            SystemMsg::Resize(width, height) => {
                log::debug!("Terminal resized to {width}x{height}");
                // Terminal resize is handled automatically by ratatui
                Command::none()
            }
            // A terminal event with no handler. Nothing to do and nothing to show, so
            // the pass ends without a render.
            SystemMsg::TerminalEventIgnored => Command::none().without_redraw(),
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
            KeyCode::Esc => Command::message(AppMsg::Timeline(TimelineMsg::Deselect)).into(),
            // A key bound to nothing changes nothing, so it should not cost a render —
            // and with no tick left to repaint anyway, holding one would otherwise be a
            // render per repeat for no reason (#536).
            _ => Command::none().without_redraw(),
        }
    }

    /// Handle key input in Composing mode
    fn handle_composing_mode_key(&mut self, key: KeyEvent) -> Command<AppMsg> {
        // In composing mode, ignore all keybindings to allow normal text input
        // This matches the old architecture behavior where 'q' is just a character,
        // not a quit command. Only hardcoded special keys are processed.
        match (key.code, key.modifiers) {
            // Escape: cancel composing
            (KeyCode::Esc, _) => {
                Command::message(AppMsg::Editor(EditorMsg::CancelComposing)).into()
            }
            // Ctrl+P: submit note (hardcoded for safety)
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                Command::message(AppMsg::Editor(EditorMsg::SubmitNote)).into()
            }
            // All other keys are passed to textarea for input
            _ => Command::message(AppMsg::Editor(EditorMsg::ProcessTextAreaInput(key))).into(),
        }
    }

    /// Handle a KeyAction resolved from keybinding
    fn handle_action(&mut self, action: KeyAction) -> Command<AppMsg> {
        match action {
            // Navigation
            KeyAction::ScrollUp => Command::message(AppMsg::Timeline(TimelineMsg::ScrollUp)).into(),
            KeyAction::ScrollDown => {
                Command::message(AppMsg::Timeline(TimelineMsg::ScrollDown)).into()
            }
            KeyAction::ScrollToTop => {
                // Delegate to TimelineMsg::SelectFirst
                Command::message(AppMsg::Timeline(TimelineMsg::SelectFirst)).into()
            }
            KeyAction::ScrollToBottom => {
                // Delegate to TimelineMsg::SelectLast
                Command::message(AppMsg::Timeline(TimelineMsg::SelectLast)).into()
            }
            KeyAction::Unselect => {
                // Delegate to TimelineMsg::Deselect to keep logic centralized
                Command::message(AppMsg::Timeline(TimelineMsg::Deselect)).into()
            }

            // Compose/interactions
            KeyAction::NewTextNote => {
                Command::message(AppMsg::Editor(EditorMsg::StartComposing)).into()
            }
            KeyAction::ReplyTextNote => {
                Command::message(AppMsg::Editor(EditorMsg::StartReply)).into()
            }
            KeyAction::React => {
                Command::message(AppMsg::Timeline(TimelineMsg::ReactToSelected)).into()
            }
            KeyAction::Repost => {
                Command::message(AppMsg::Timeline(TimelineMsg::RepostSelected)).into()
            }

            // Tab management
            KeyAction::OpenAuthorTimeline => {
                Command::message(AppMsg::Timeline(TimelineMsg::OpenAuthorTimeline)).into()
            }
            KeyAction::OpenMentionTab => {
                Command::message(AppMsg::Timeline(TimelineMsg::OpenMentionTab)).into()
            }
            KeyAction::CloseCurrentTab => {
                Command::message(AppMsg::Timeline(TimelineMsg::CloseCurrentTab)).into()
            }
            KeyAction::PrevTab => Command::message(AppMsg::Timeline(TimelineMsg::PrevTab)).into(),
            KeyAction::NextTab => Command::message(AppMsg::Timeline(TimelineMsg::NextTab)).into(),

            // System
            KeyAction::Quit => Command::message(AppMsg::System(SystemMsg::Quit)).into(),
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
            TimelineMsg::OpenMentionTab => self.state.open_mention_tab(),
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

    /// Handle media messages
    ///
    /// A track change is the only thing here that can reach the screen, via the NIP-38
    /// status line — and only if `MusicStatus` accepts it, which `publish_music_status`
    /// decides. The other events nowhear reports — pausing, seeking, changing the
    /// volume, a player coming or going — are displayed nowhere at all, so they decline
    /// the redraw they would otherwise cost. Without that, nudging the volume of a
    /// player nostui is watching repaints the whole timeline.
    ///
    /// The undisplayed events are listed rather than caught by a wildcard. Declining the
    /// redraw is an assertion that they change nothing visible, and a wildcard would
    /// extend that assertion to whatever nowhear adds next — silently, and with no
    /// periodic repaint left to mask it. `MediaEvent` is not `#[non_exhaustive]`, so
    /// naming them makes the next variant a compile error instead.
    ///
    /// A media source error reaches the screen no more than they do, and on that
    /// reasoning would decline the redraw with them. It keeps one, deliberately,
    /// because it is the one message here that can repeat: a source that cannot be built
    /// reports, and the report is what restarts it, so it fails and reports again with
    /// no backoff (#529). The repaint is the only thing left costing that loop anything
    /// per iteration. Keeping it is a brake nobody designed, held until #529 fits a real
    /// one — a wasted render on the rare genuine error is the cheaper mistake.
    fn handle_media_msg(&mut self, msg: Result<MediaEvent, MediaSourceError>) -> Command<AppMsg> {
        match msg {
            Ok(MediaEvent::TrackChanged { track, .. }) => self.state.publish_music_status(track),
            Ok(
                MediaEvent::StateChanged { .. }
                | MediaEvent::PositionChanged { .. }
                | MediaEvent::VolumeChanged { .. }
                | MediaEvent::PlayerAdded { .. }
                | MediaEvent::PlayerRemoved { .. },
            ) => Command::none().without_redraw(),
            Err(e) => {
                log::error!("media source error: {e}");
                Command::none()
            }
        }
    }

    /// Handle NostrEvents subscription messages
    ///
    /// The two arms that only log decline the redraw, because for them
    /// `Command::without_redraw`'s declaration is true rather than convenient:
    /// `ClientNotification::Event` only logs (nostui routes events from the `Message`
    /// stream instead, see the note below), and a `Message` carrying anything other than
    /// `RelayMessage::Event` also only logs. The pool emits both notifications for each
    /// newly-seen event, so the ignored `Event` used to cost a full render per note for
    /// no visible change.
    ///
    /// Every other arm returns a redrawing command, so on tears 0.11 each inbound relay
    /// notification that lands in its own pass still costs one render. With the frame rate
    /// gone there is no ceiling above that, and on a busy feed redraw frequency tracks
    /// relay throughput where 0.10.x clamped it to `--frame-rate`.
    ///
    /// That remainder is a known, accepted regression, not an oversight. `without_redraw`
    /// is not a general fix: the declaration is false for an arm that appends to the
    /// timeline. Bounding it properly means deciding per message whether the view actually
    /// changed — tracked in #510.
    fn handle_nostr_subscription_message(
        &mut self,
        msg: NostrSubscriptionMessage,
    ) -> Command<AppMsg> {
        match msg {
            NostrSubscriptionMessage::Ready { sender } => self.state.on_connection_ready(sender),
            NostrSubscriptionMessage::SubscriptionCreated {
                feed,
                subscription_id,
            } => self.state.track_subscription_created(feed, subscription_id),
            NostrSubscriptionMessage::EventPublished { id, result } => {
                self.state.resolve_publish(id, result)
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
                ClientNotification::Event {
                    event,
                    subscription_id,
                    ..
                } => {
                    log::debug!(
                        "Received event {} from subscription {subscription_id:?}",
                        event.id
                    );
                    Command::none().without_redraw()
                }
                ClientNotification::Message { message, .. } => {
                    log::debug!("Received relay message: {message:?}");

                    if let RelayMessage::Event {
                        subscription_id,
                        event,
                    } = *message
                    {
                        self.state
                            .route_relay_event(&subscription_id, event.into_owned())
                    } else {
                        Command::none().without_redraw()
                    }
                }
                ClientNotification::Shutdown => self.state.notify_subscription_shutdown(),
            },
            NostrSubscriptionMessage::Error { error } => {
                self.state.notify_subscription_error(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::time::Duration;

    use nostr_sdk::prelude::Event as NostrEvent;
    use nowhear::Track;
    use tears::testing::TestStore;

    use super::*;
    use crate::application::config::Config;
    use crate::domain::nostr::FeedKind;
    use crate::model::editor::Message as EditorMessage;
    use crate::model::status_bar::Message as StatusBarMessage;
    use crate::model::timeline::Message as TimelineMessage;

    /// Create flags for a test app instance
    fn test_flags() -> InitFlags {
        let keys = Keys::generate();

        InitFlags {
            pubkey: keys.public_key(),
            keys: Some(keys),
            config: Config::default(),
            nostr_client: Client::default(),
        }
    }

    /// Create a test app instance
    fn create_test_app() -> TearsApp<'static> {
        let (app, _) = TearsApp::new(test_flags());
        app
    }

    /// Wrap a relay pool notification the way the subscription delivers it.
    fn notification(notif: ClientNotification) -> AppMsg {
        AppMsg::Nostr(NostrMsg::SubscriptionMessage(
            NostrSubscriptionMessage::Notification(Box::new(notif)),
        ))
    }

    fn test_relay_url() -> RelayUrl {
        RelayUrl::parse("wss://relay.example.com").expect("valid relay url")
    }

    fn test_note() -> NostrEvent {
        EventBuilder::new(Kind::TextNote, "test note")
            .finalize(&Keys::generate())
            .expect("Failed to sign test event")
    }

    #[test]
    fn test_unselect_action_delegates_to_deselect() {
        let mut app = create_test_app();

        // Add a test note to allow selection
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, "test note")
            .finalize(&keys)
            .expect("Failed to sign test event");
        let _ = app
            .state
            .process_nostr_event_for_tab(event, &FeedKind::Home);

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
        let event = EventBuilder::new(Kind::TextNote, "test note")
            .finalize(&keys)
            .expect("Failed to sign test event");
        let _ = app
            .state
            .process_nostr_event_for_tab(event, &FeedKind::Home);

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
        let event1 = EventBuilder::new(Kind::TextNote, "test note 1")
            .finalize(&keys)
            .expect("Failed to sign test event");
        let event2 = EventBuilder::new(Kind::TextNote, "test note 2")
            .finalize(&keys)
            .expect("Failed to sign test event");
        let _ = app
            .state
            .process_nostr_event_for_tab(event1, &FeedKind::Home);
        let _ = app
            .state
            .process_nostr_event_for_tab(event2, &FeedKind::Home);

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
        let event1 = EventBuilder::new(Kind::TextNote, "test note 1")
            .finalize(&keys)
            .expect("Failed to sign test event");
        let event2 = EventBuilder::new(Kind::TextNote, "test note 2")
            .finalize(&keys)
            .expect("Failed to sign test event");
        let _ = app
            .state
            .process_nostr_event_for_tab(event1, &FeedKind::Home);
        let _ = app
            .state
            .process_nostr_event_for_tab(event2, &FeedKind::Home);

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
        let event = EventBuilder::new(Kind::TextNote, "test note")
            .finalize(&keys)
            .expect("Failed to sign test event");
        let _ = app
            .state
            .process_nostr_event_for_tab(event, &FeedKind::Home);

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
        let event1 = EventBuilder::new(Kind::TextNote, "test note 1")
            .finalize(&keys)
            .expect("Failed to sign test event");
        let event2 = EventBuilder::new(Kind::TextNote, "test note 2")
            .finalize(&keys)
            .expect("Failed to sign test event");
        let _ = app
            .state
            .process_nostr_event_for_tab(event1, &FeedKind::Home);
        let _ = app
            .state
            .process_nostr_event_for_tab(event2, &FeedKind::Home);

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

        // Command should be a Command::quit() effect (we can't directly test this,
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

    /// The pool reports each newly-seen event twice — once as `Event`, once as a
    /// `Message` — and nostui reads only the `Message`. Redrawing for the arm it
    /// ignores would spend a full render on nothing, so that arm declines it (#510).
    #[test]
    fn test_ignored_event_notification_does_not_redraw() {
        let mut store = TestStore::<TearsApp<'static>>::new(test_flags());

        store.send(notification(ClientNotification::Event {
            relay_url: test_relay_url(),
            subscription_id: SubscriptionId::new("unknown"),
            event: Box::new(test_note()),
        }));

        assert!(!store.redraw_requested());
        store.finish();
    }

    /// A relay message that is not an `EVENT` — an `EOSE`, here — is only logged,
    /// so it changes nothing the view shows and declines the redraw too (#510).
    #[test]
    fn test_non_event_relay_message_does_not_redraw() {
        let mut store = TestStore::<TearsApp<'static>>::new(test_flags());

        store.send(notification(ClientNotification::Message {
            relay_url: test_relay_url(),
            message: Box::new(RelayMessage::EndOfStoredEvents(Cow::Owned(
                SubscriptionId::new("home"),
            ))),
        }));

        assert!(!store.redraw_requested());
        store.finish();
    }

    /// The other half of #510(a): declining the redraw is specific to the two arms
    /// that only log. An `EVENT` routed to a tab appends to the timeline, so it must
    /// still redraw.
    ///
    /// The timeline assertion is what makes this the routed case: an `EVENT` whose
    /// subscription matches no tab is dropped by `route_relay_event` and today redraws
    /// anyway, so asserting the directive alone would hold either way.
    #[test]
    fn test_routed_event_message_still_redraws() {
        let mut store = TestStore::<TearsApp<'static>>::new(test_flags());
        let subscription_id = SubscriptionId::new("home");

        store.send(AppMsg::Nostr(NostrMsg::SubscriptionMessage(
            NostrSubscriptionMessage::SubscriptionCreated {
                feed: FeedKind::Home,
                subscription_id: subscription_id.clone(),
            },
        )));
        store.send(notification(ClientNotification::Message {
            relay_url: test_relay_url(),
            message: Box::new(RelayMessage::Event {
                subscription_id: Cow::Owned(subscription_id),
                event: Cow::Owned(test_note()),
            }),
        }));

        assert_eq!(store.state().state.timeline.len(), 1);
        assert!(store.redraw_requested());
        store.finish();
    }

    /// #531: a Windows console reports a release for every press, and a release is not
    /// someone pressing a key. Configured bindings were safe — the map they live in
    /// compares the kind — but the paths matching on the key's code alone were not:
    /// a release of `Esc` reached normal mode's fallback and deselected the timeline
    /// behind a draft the press had just cancelled.
    #[test]
    fn test_key_release_is_not_input() {
        let release = KeyEvent::new_with_kind(
            KeyCode::Char('j'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        );

        assert!(matches!(
            terminal_event_to_msg(Event::Key(release)),
            AppMsg::System(SystemMsg::TerminalEventIgnored)
        ));
    }

    /// The other two kinds are the key going down or staying down, and both arrive as a
    /// press. Nothing downstream wants the difference, and #536 is what came of three
    /// paths each deciding that for themselves — so this pins that a repeat is
    /// indistinguishable from a press by the time anything acts on it.
    #[test]
    fn test_press_and_repeat_both_arrive_as_a_press() {
        let press =
            KeyEvent::new_with_kind(KeyCode::Char('j'), KeyModifiers::NONE, KeyEventKind::Press);
        let repeat =
            KeyEvent::new_with_kind(KeyCode::Char('j'), KeyModifiers::NONE, KeyEventKind::Repeat);

        for key in [press, repeat] {
            assert!(
                matches!(
                    terminal_event_to_msg(Event::Key(key)),
                    AppMsg::System(SystemMsg::KeyInput(got)) if got == press
                ),
                "{:?} should arrive as the press",
                key.kind
            );
        }
    }

    /// What that buys: holding a key reaches the binding it is configured for. Before
    /// #536 a repeat hashed differently from the `Press` the config was parsed into, so
    /// it matched nothing and holding `j` did not scroll.
    ///
    /// The binding is put there rather than taken from the shipped defaults, so the test
    /// pins the lookup rather than what `.config/config.json5` happens to say.
    #[test]
    fn test_a_held_key_reaches_its_binding() {
        let mut flags = test_flags();
        flags.config.keybindings.home.insert(
            vec![KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)],
            KeyAction::ScrollDown,
        );
        let mut store = TestStore::<TearsApp<'static>>::new(flags);

        store.send(terminal_event_to_msg(Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('j'),
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ))));

        store.receive_matching(|msg| matches!(msg, AppMsg::Timeline(TimelineMsg::ScrollDown)));
        store.finish();
    }

    /// A key bound to nothing changes nothing, so it must not repaint — which matters
    /// now that a held key produces one of these per repeat rather than none (#536).
    #[test]
    fn test_a_key_bound_to_nothing_does_not_redraw() {
        let mut store = TestStore::<TearsApp<'static>>::new(test_flags());

        store.send(AppMsg::System(SystemMsg::KeyInput(KeyEvent::new(
            KeyCode::F(12),
            KeyModifiers::NONE,
        ))));

        assert!(!store.redraw_requested());
        store.finish();
    }

    /// Resizes still route to their own message, and everything else nostui does not
    /// act on to the one that does nothing.
    #[test]
    fn test_other_terminal_events_keep_their_routing() {
        assert!(matches!(
            terminal_event_to_msg(Event::Resize(80, 24)),
            AppMsg::System(SystemMsg::Resize(80, 24))
        ));
        assert!(matches!(
            terminal_event_to_msg(Event::FocusGained),
            AppMsg::System(SystemMsg::TerminalEventIgnored)
        ));
    }

    /// A terminal event nostui does not act on changes nothing, so it must not
    /// render. Before #527 this arm produced a tick, which the FPS display counted
    /// as one and redrew for.
    #[test]
    fn test_ignored_terminal_event_does_not_redraw() {
        let mut store = TestStore::<TearsApp<'static>>::new(test_flags());

        store.send(AppMsg::System(SystemMsg::TerminalEventIgnored));

        assert!(!store.redraw_requested());
        store.finish();
    }

    /// The positive control for the two below it: a track the NIP-38 line *can*
    /// describe is put on the status bar, so that pass has to redraw. Without this,
    /// swapping the arms in `publish_music_status` would leave both of them passing
    /// and the now-playing line invisible on a client with nothing else going on.
    #[test]
    fn test_track_the_status_line_shows_redraws() {
        let mut store = TestStore::<TearsApp<'static>>::new(test_flags());

        store.send(AppMsg::Media(Ok(MediaEvent::TrackChanged {
            // Not "Music": that is `NOW_PLAYING_LABEL`, and the assertion below could
            // not tell the label from an echoed player name.
            player_name: "Spotify".to_owned(),
            track: Track {
                title: "Song".to_owned(),
                artist: vec!["Artist".to_owned()],
                duration: Some(Duration::from_secs(180)),
                album: None,
                album_artist: vec![],
                track_number: None,
                artwork: None,
            },
        })));

        assert!(store.redraw_requested());
        assert_eq!(
            store.state().state.status_bar.message(),
            Some("[Music] Song - Artist")
        );
        store.finish();
    }

    /// The media events nostui does not display — pausing, seeking, volume — must not
    /// repaint either. With the tick gone these are the remaining messages that arrive
    /// without the user touching nostui at all.
    #[test]
    fn test_undisplayed_media_event_does_not_redraw() {
        let mut store = TestStore::<TearsApp<'static>>::new(test_flags());

        store.send(AppMsg::Media(Ok(MediaEvent::VolumeChanged {
            player_name: "Music".to_owned(),
            volume: 0.5,
        })));

        assert!(!store.redraw_requested());
        store.finish();
    }

    /// `handle_timeline_msg` clears the status bar before it dispatches, so a timeline
    /// message that then finds nothing to do has still changed the screen and must
    /// redraw. Declining it leaves the cleared line standing — which the tick used to
    /// hide within a second, and #527 removed the tick.
    #[test]
    fn test_timeline_message_that_does_nothing_still_clears_the_status_bar_on_screen() {
        let mut store = TestStore::<TearsApp<'static>>::new(test_flags());
        let subscription_id = SubscriptionId::new("home");

        // Complete startup: until an event arrives, timeline messages are ignored.
        store.send(AppMsg::Nostr(NostrMsg::SubscriptionMessage(
            NostrSubscriptionMessage::SubscriptionCreated {
                feed: FeedKind::Home,
                subscription_id: subscription_id.clone(),
            },
        )));
        store.send(notification(ClientNotification::Message {
            relay_url: test_relay_url(),
            message: Box::new(RelayMessage::Event {
                subscription_id: Cow::Owned(subscription_id),
                event: Cow::Owned(test_note()),
            }),
        }));
        store.send(AppMsg::System(SystemMsg::ShowError("boom".to_owned())));
        assert!(store.state().state.status_bar.message().is_some());

        // Nothing is selected, so reacting has nothing to submit — but the bar was
        // cleared on the way in.
        store.send(AppMsg::Timeline(TimelineMsg::ReactToSelected));

        assert_eq!(store.state().state.status_bar.message(), None);
        assert!(store.redraw_requested());
        store.finish();
    }

    /// A track the NIP-38 status cannot describe is not shown and not sent, so it must
    /// not repaint either. `MusicStatus::new` rejects a track with no duration, which
    /// radio and live streams report routinely while their metadata keeps changing.
    #[test]
    fn test_track_the_status_line_rejects_does_not_redraw() {
        let mut store = TestStore::<TearsApp<'static>>::new(test_flags());

        store.send(AppMsg::Media(Ok(MediaEvent::TrackChanged {
            player_name: "Radio".to_owned(),
            track: Track {
                title: "Some Stream".to_owned(),
                artist: vec!["Station".to_owned()],
                duration: None,
                album: None,
                album_artist: vec![],
                track_number: None,
                artwork: None,
            },
        })));

        assert!(!store.redraw_requested());
        assert_eq!(store.state().state.status_bar.message(), None);
        store.finish();
    }

    /// A media source error shows nothing, so on its own merits it would decline the
    /// redraw like its neighbours. It keeps one because the error is what restarts the
    /// source that produced it, and the repaint is the only per-iteration cost left in
    /// that loop until #529 bounds it. Pinned so the brake is not removed by tidying.
    #[test]
    fn test_media_source_error_keeps_its_redraw_as_a_brake() {
        let mut store = TestStore::<TearsApp<'static>>::new(test_flags());

        store.send(AppMsg::Media(Err(MediaSourceError::UnsupportedPlatform)));

        assert!(store.redraw_requested());
        store.finish();
    }
}
