use crossterm::event::KeyEvent;
use nostr_sdk::prelude::*;
use nowhear::Track;
use tears::prelude::*;
use tokio::sync::mpsc;

use crate::{
    application::config::Config,
    application::message::AppMsg,
    domain::nostr::{nip10::ReplyTagsBuilder, nip38::MusicStatus, FeedKind, Profile},
    model::{
        editor::{Editor, Message as EditorMessage},
        fps::{Fps, Message as FpsMessage},
        nostr::{Message as NostrMessage, Nostr, NostrOutcome},
        nostr_gateway::{CommandError, NostrCommand},
        status_bar::{Message, StatusBar},
        timeline::{
            tab::TimelineOutcome, text_note::TextNote, Message as TimelineMessage, Timeline,
        },
    },
};

pub mod user;

pub use user::UserState;

/// Tracks whether the application is still starting up.
///
/// Startup lasts from launch until the first event arrives. While starting up,
/// the application ignores timeline operations so the "loading..." status
/// message is preserved. This is a startup indicator, not a data-completeness
/// signal (it does not wait for EOSE). `Default` starts in the startup state.
#[derive(Debug, Clone, Default)]
pub struct Startup {
    completed: bool,
}

impl Startup {
    /// Whether the application is still starting up
    pub fn is_in_progress(&self) -> bool {
        !self.completed
    }

    /// Mark startup as completed (idempotent)
    pub fn mark_completed(&mut self) {
        self.completed = true;
    }
}

/// Unified application state
#[derive(Debug, Default)]
pub struct AppState<'a> {
    pub timeline: Timeline,
    pub editor: Editor<'a>,
    pub user: UserState,
    pub nostr: Nostr,
    pub config: ConfigState,
    pub fps: Fps,
    pub status_bar: StatusBar,
    pub startup: Startup,
    /// Sender for dispatching commands to the Nostr subscription worker.
    /// Owned here (not in `model::nostr`) so the application layer performs the
    /// I/O while `model` stays side-effect free; set once the worker is ready.
    command_sender: Option<mpsc::UnboundedSender<NostrCommand>>,
}

/// Configuration state - holds all user-configurable settings
#[derive(Debug, Clone, Default)]
pub struct ConfigState {
    /// Current configuration loaded from file
    pub config: Config,
}

impl<'a> AppState<'a> {
    /// Initialize AppState with the specified public key
    pub fn new(current_user_pubkey: PublicKey) -> Self {
        Self {
            user: UserState::new_with_pubkey(current_user_pubkey),
            ..Default::default()
        }
    }

    /// Initialize AppState with the specified public key and config
    pub fn new_with_config(current_user_pubkey: PublicKey, config: Config) -> Self {
        Self {
            user: UserState::new_with_pubkey(current_user_pubkey),
            config: ConfigState { config },
            ..Default::default()
        }
    }

    /// Process a received Nostr event for a specific tab
    pub fn process_nostr_event_for_tab(
        &mut self,
        event: Event,
        feed: &FeedKind,
    ) -> Command<AppMsg> {
        // Receiving any event means startup has produced its first results.
        self.startup.mark_completed();

        match event.kind {
            Kind::TextNote => {
                let current_loading_more_state = self.timeline.is_loading_more_for_feed(feed);

                let _ = self.timeline.update(TimelineMessage::NoteAddedToTab {
                    event,
                    feed: feed.clone(),
                });

                let new_loading_more_state = self.timeline.is_loading_more_for_feed(feed);

                if current_loading_more_state == Some(true) && new_loading_more_state == Some(false)
                {
                    let tab_title = self.active_tab_title();
                    self.set_status(tab_title, "loaded more");
                }

                Command::none()
            }
            Kind::Metadata => {
                // Metadata is shared across all tabs
                if let Ok(metadata) = Metadata::from_json(event.content.clone()) {
                    let profile = Profile::new(event.pubkey, event.created_at, metadata);
                    self.user.insert_newer_profile(profile);
                }
                Command::none()
            }
            Kind::Repost => {
                let _ = self.timeline.update(TimelineMessage::RepostAdded { event });
                Command::none()
            }
            Kind::Reaction => {
                let _ = self
                    .timeline
                    .update(TimelineMessage::ReactionAdded { event });
                Command::none()
            }
            Kind::ZapReceipt => {
                let _ = self
                    .timeline
                    .update(TimelineMessage::ZapReceiptAdded { event });
                Command::none()
            }
            _ => Command::none(),
        }
    }

    /// Open (or switch to) the mention timeline tab.
    ///
    /// If the Mention tab already exists, it is selected. Otherwise a new tab is
    /// created, a subscription is requested, and a "loading" status message is shown.
    pub fn open_mention_tab(&mut self) -> Command<AppMsg> {
        let feed = FeedKind::Mention;

        // Tab already open: just switch to it.
        if let Some(index) = self.timeline.find_tab_by_feed(&feed) {
            let _ = self.timeline.update(TimelineMessage::TabSelected { index });
            return Command::none();
        }

        let _ = self
            .timeline
            .update(TimelineMessage::TabAdded { feed: feed.clone() });

        if self.timeline.find_tab_by_feed(&feed).is_some() {
            log::info!("Created new mention timeline");

            let outcome = self
                .nostr
                .update(NostrMessage::SubscriptionRequested { feed });
            self.dispatch_nostr(outcome);

            self.set_status("Mention", "loading...");
        } else {
            log::error!("Failed to create mention timeline");

            self.set_status_error("Mention", "failed to open tab");
        }

        Command::none()
    }

    /// Open (or switch to) the author timeline tab for the given pubkey.
    ///
    /// If a tab for this author already exists, it is selected. Otherwise a new
    /// tab is created, a subscription is requested, and a "loading" status
    /// message is shown (or an error message if the tab could not be created).
    pub fn open_author_timeline(&mut self, author_pubkey: PublicKey) -> Command<AppMsg> {
        let Ok(author_npub) = author_pubkey.to_bech32();
        let feed = FeedKind::Author(author_pubkey);

        // Tab already open: just switch to it.
        if let Some(index) = self.timeline.find_tab_by_feed(&feed) {
            let _ = self.timeline.update(TimelineMessage::TabSelected { index });
            return Command::none();
        }

        // Otherwise create it, then subscribe and show the loading status.
        let _ = self
            .timeline
            .update(TimelineMessage::TabAdded { feed: feed.clone() });

        if self.timeline.find_tab_by_feed(&feed).is_some() {
            log::info!("Created new author timeline for {author_npub}");

            let outcome = self
                .nostr
                .update(NostrMessage::SubscriptionRequested { feed });
            self.dispatch_nostr(outcome);

            self.set_status(author_npub, "loading...");
        } else {
            log::error!("Failed to create author timeline");

            self.set_status_error(author_npub, "failed to open tab");
        }

        Command::none()
    }

    /// Close the currently active tab and unsubscribe from its subscriptions.
    ///
    /// The Home tab cannot be closed; the [`Timeline`] enforces this, so calling
    /// this while Home is active is a no-op apart from the (no-op) unsubscribe.
    pub fn close_current_tab(&mut self) -> Command<AppMsg> {
        let current_index = self.timeline.active_tab_index();

        // Capture the feed before removing the tab.
        let feed = self.timeline.active_tab().feed().clone();

        let _ = self.timeline.update(TimelineMessage::TabRemoved {
            index: current_index,
        });

        // Unsubscribe the subscriptions associated with the closed tab.
        let outcome = self.nostr.update(NostrMessage::SubscriptionClosed { feed });
        self.dispatch_nostr(outcome);

        Command::none()
    }

    /// Submit a NIP-25 reaction for the currently selected note.
    pub fn react_to_selected(&mut self) -> Command<AppMsg> {
        self.submit_engagement_for_selected("Reacted", TextNote::reaction_builder)
    }

    /// Submit a NIP-18 repost for the currently selected note.
    pub fn repost_selected(&mut self) -> Command<AppMsg> {
        self.submit_engagement_for_selected("Reposted", TextNote::repost_builder)
    }

    /// Build an engagement event for the selected note with `build`, submit it,
    /// and show `label` in the status bar. No-op when nothing is selected.
    fn submit_engagement_for_selected(
        &mut self,
        label: &str,
        build: fn(&TextNote) -> EventBuilder,
    ) -> Command<AppMsg> {
        let Some(note) = self.timeline.selected_note() else {
            return Command::none();
        };

        let note_id = note.bech32_id();
        let event_builder = build(note);
        log::info!("{label} event: {note_id}");

        let outcome = self
            .nostr
            .update(NostrMessage::EventSubmitted { event_builder });
        self.dispatch_nostr(outcome);

        self.set_status(label, note_id);

        Command::none()
    }

    /// Start composing a reply to the currently selected note.
    ///
    /// Sets the reply context (target event and author profile) on the editor.
    /// No-op when nothing is selected.
    pub fn start_reply(&mut self) -> Command<AppMsg> {
        let Some(note) = self.timeline.selected_note() else {
            return Command::none();
        };

        let note_id = note.bech32_id();
        let event = note.as_event().clone();
        let author_pubkey = note.author_pubkey();
        log::info!("Starting reply to event: {note_id}");

        let profile = self.user.get_profile(&author_pubkey).cloned();

        self.editor.update(EditorMessage::ReplyStarted {
            to: Box::new(event),
            profile: Box::new(profile),
        });

        Command::none()
    }

    /// Publish the editor's current content as a text note, or as a NIP-10 reply
    /// when a reply target is set, then reset the editor.
    pub fn submit_note(&mut self) -> Command<AppMsg> {
        let content = self.editor.get_content();

        let event_builder = if let Some(reply_to_event) = self.editor.reply_target() {
            log::info!("Publishing reply: {content}");
            // Build NIP-10 reply tags (root/reply markers, deduped p-tag).
            EventBuilder::text_note(&content).tags(ReplyTagsBuilder::build(reply_to_event.clone()))
        } else {
            log::info!("Publishing note: {content}");
            EventBuilder::text_note(&content)
        };

        let outcome = self
            .nostr
            .update(NostrMessage::EventSubmitted { event_builder });
        self.dispatch_nostr(outcome);

        self.set_status("Posted", content);

        // Clear UI state.
        self.editor.update(EditorMessage::ComposingCanceled);

        Command::none()
    }

    /// Publish a NIP-38 live status event for the currently playing track and
    /// show it in the status bar. No-op when the track is missing the fields
    /// required to build a status.
    pub fn publish_music_status(&mut self, track: Track) -> Command<AppMsg> {
        let Some(status) = MusicStatus::new(track) else {
            return Command::none();
        };

        let content = status.content();
        let event_builder = status.live_status_builder();

        let outcome = self
            .nostr
            .update(NostrMessage::EventSubmitted { event_builder });
        self.dispatch_nostr(outcome);

        self.set_status("Music", content);

        Command::none()
    }

    /// Title of the currently active tab, resolved against known profiles.
    fn active_tab_title(&self) -> String {
        self.timeline.active_tab().tab_title(self.user.profiles())
    }

    /// Show a status message in the status bar.
    fn set_status(&mut self, label: impl Into<String>, message: impl Into<String>) {
        self.status_bar.update(Message::MessageChanged {
            label: label.into(),
            message: message.into(),
        });
    }

    /// Show an error message in the status bar.
    fn set_status_error(&mut self, label: impl Into<String>, message: impl Into<String>) {
        self.status_bar.update(Message::ErrorMessageChanged {
            label: label.into(),
            message: message.into(),
        });
    }

    /// Dispatch a [`NostrOutcome`] produced by `model::nostr` to the worker.
    ///
    /// `model::nostr::update` is side-effect free and only reports the command
    /// to send; the application owns the sender and performs the actual I/O.
    fn dispatch_nostr(&self, outcome: Option<NostrOutcome>) {
        let Some(NostrOutcome::Send(command)) = outcome else {
            return;
        };

        let Some(sender) = self.command_sender.as_ref() else {
            log::warn!("Dropping Nostr command, worker not ready: {command:?}");
            return;
        };

        if sender.send(command).is_err() {
            log::error!("Failed to send Nostr command: subscription worker is gone");
        }
    }

    /// Record that the Nostr subscription is ready and store its command sender,
    /// then show the initial "loading" status for the active tab.
    pub fn on_connection_ready(
        &mut self,
        command_sender: mpsc::UnboundedSender<NostrCommand>,
    ) -> Command<AppMsg> {
        log::info!("NostrEvents subscription ready");

        let tab_title = self.active_tab_title();
        self.command_sender = Some(command_sender);
        let outcome = self.nostr.update(NostrMessage::ConnectionReady);
        self.dispatch_nostr(outcome);
        self.set_status(tab_title, "loading...");

        Command::none()
    }

    /// Route an incoming relay event to the tab that owns its subscription.
    ///
    /// While still starting up, the first event flips the status to "loaded".
    /// Events whose subscription is not tracked by any tab are ignored.
    pub fn route_relay_event(
        &mut self,
        subscription_id: &SubscriptionId,
        event: Event,
    ) -> Command<AppMsg> {
        if self.startup.is_in_progress() {
            let tab_title = self.active_tab_title();
            self.set_status(tab_title, "loaded");
        }

        let Some(feed) = self
            .nostr
            .find_tab_by_subscription(subscription_id)
            .cloned()
        else {
            return Command::none();
        };

        log::debug!(
            "Routing event {} (kind: {:?}) to tab {feed:?}",
            event.id,
            event.kind
        );

        self.process_nostr_event_for_tab(event, &feed)
    }

    /// Request older events for the active tab, paginating before its oldest
    /// known timestamp. No-op when the active timeline has no events yet.
    pub fn load_more_timeline(&mut self) -> Command<AppMsg> {
        log::info!("Loading more timeline events");

        let Some(since) = self.timeline.oldest_timestamp() else {
            log::warn!("No oldest timestamp available, cannot load more");
            return Command::none();
        };

        let feed = self.timeline.active_tab().feed().clone();
        let tab_title = self.active_tab_title();

        let outcome = self
            .nostr
            .update(NostrMessage::HistoryRequested { feed, since });
        self.dispatch_nostr(outcome);

        self.set_status(tab_title, "loading more...");

        Command::none()
    }

    // --- Thin command methods ---
    //
    // These wrap a single sub-state transition so that `TearsApp` never mutates
    // a sub-state directly; all state changes flow through `AppState`. Read-only
    // access (used by the view) is intentionally left to the public fields.

    /// Clear the current status message.
    pub fn clear_status_message(&mut self) -> Command<AppMsg> {
        self.status_bar.update(Message::MessageCleared);
        Command::none()
    }

    /// Show a system-level error in the status bar.
    pub fn show_error(&mut self, error: String) -> Command<AppMsg> {
        log::error!("{error}");
        self.set_status_error("System", error);
        Command::none()
    }

    /// Record a frame tick for FPS tracking.
    pub fn record_tick(&mut self) -> Command<AppMsg> {
        self.fps.update(FpsMessage::FrameRecorded { now: None });
        Command::none()
    }

    /// Move the selection to the previous timeline item.
    pub fn scroll_up(&mut self) -> Command<AppMsg> {
        let _ = self.timeline.update(TimelineMessage::PreviousItemSelected);
        Command::none()
    }

    /// Move the selection to the next timeline item.
    ///
    /// When the selection is already at the bottom, the timeline reports
    /// `LoadMoreRequested` and the application loads older events.
    pub fn scroll_down(&mut self) -> Command<AppMsg> {
        match self.timeline.update(TimelineMessage::NextItemSelected) {
            Some(TimelineOutcome::LoadMoreRequested) => self.load_more_timeline(),
            None => Command::none(),
        }
    }

    /// Select the timeline item at `index`.
    pub fn select_note(&mut self, index: usize) -> Command<AppMsg> {
        let _ = self
            .timeline
            .update(TimelineMessage::ItemSelected { index });
        Command::none()
    }

    /// Clear the current timeline selection.
    pub fn deselect_note(&mut self) -> Command<AppMsg> {
        let _ = self.timeline.update(TimelineMessage::ItemSelectionCleared);
        Command::none()
    }

    /// Select the first timeline item.
    pub fn select_first_note(&mut self) -> Command<AppMsg> {
        let _ = self.timeline.update(TimelineMessage::FirstItemSelected);
        Command::none()
    }

    /// Select the last timeline item.
    pub fn select_last_note(&mut self) -> Command<AppMsg> {
        let _ = self.timeline.update(TimelineMessage::LastItemSelected);
        Command::none()
    }

    /// Switch to the tab at `index`.
    pub fn select_tab(&mut self, index: usize) -> Command<AppMsg> {
        let _ = self.timeline.update(TimelineMessage::TabSelected { index });
        log::debug!("Selected tab index: {}", self.timeline.active_tab_index());
        Command::none()
    }

    /// Switch to the next tab (wraps around).
    pub fn next_tab(&mut self) -> Command<AppMsg> {
        let _ = self.timeline.update(TimelineMessage::NextTabSelected);
        log::debug!("Switched to next tab: {}", self.timeline.active_tab_index());
        Command::none()
    }

    /// Switch to the previous tab (wraps around).
    pub fn prev_tab(&mut self) -> Command<AppMsg> {
        let _ = self.timeline.update(TimelineMessage::PreviousTabSelected);
        log::debug!(
            "Switched to previous tab: {}",
            self.timeline.active_tab_index()
        );
        Command::none()
    }

    /// Start composing a new note.
    pub fn start_composing(&mut self) -> Command<AppMsg> {
        self.editor.update(EditorMessage::ComposingStarted);
        Command::none()
    }

    /// Cancel the current composing/reply session.
    pub fn cancel_composing(&mut self) -> Command<AppMsg> {
        self.editor.update(EditorMessage::ComposingCanceled);
        Command::none()
    }

    /// Forward a key event to the editor's text area.
    pub fn process_text_input(&mut self, event: KeyEvent) -> Command<AppMsg> {
        self.editor
            .update(EditorMessage::KeyEventReceived { event });
        Command::none()
    }

    /// Close the Nostr connection: unsubscribe and disconnect from relays.
    pub fn close_connection(&mut self) -> Command<AppMsg> {
        let outcome = self.nostr.update(NostrMessage::ConnectionClosed);
        self.dispatch_nostr(outcome);
        self.command_sender = None;
        Command::none()
    }

    /// Track a subscription that the relay layer created for a tab.
    pub fn track_subscription_created(
        &mut self,
        feed: FeedKind,
        subscription_id: SubscriptionId,
    ) -> Command<AppMsg> {
        log::info!("Subscription created for {feed:?}: {subscription_id:?}");
        let outcome = self.nostr.update(NostrMessage::SubscriptionCreated {
            feed,
            sub_id: subscription_id,
        });
        self.dispatch_nostr(outcome);
        Command::none()
    }

    /// Show that the Nostr subscription was shut down.
    pub fn notify_subscription_shutdown(&mut self) -> Command<AppMsg> {
        log::info!("Nostr subscription shut down");
        self.set_status("Nostr", "disconntected");
        Command::none()
    }

    /// Show a Nostr subscription error in the status bar.
    pub fn notify_subscription_error(&mut self, error: CommandError) -> Command<AppMsg> {
        self.set_status_error("Nostr", format!("{error:?}"));
        Command::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{nostr::Profile, text::shorten_npub};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::time::Duration;

    fn create_track(title: &str) -> Track {
        Track {
            title: title.to_owned(),
            artist: vec!["Artist".to_owned()],
            duration: Some(Duration::from_secs(180)),
            album: None,
            album_artist: vec![],
            track_number: None,
            art_url: None,
        }
    }

    fn create_text_note(keys: &Keys, content: &str, created_at: Timestamp) -> Result<Event> {
        Ok(EventBuilder::text_note(content)
            .custom_created_at(created_at)
            .sign_with_keys(keys)?)
    }

    #[test]
    fn test_app_state_default() {
        let state = AppState::default();

        assert_eq!(state.timeline.len(), 0);
        assert!(!state.editor.is_active());
        assert!(state.startup.is_in_progress());
    }

    #[test]
    fn test_startup_mark_completed() {
        let mut startup = Startup::default();
        startup.mark_completed();
        assert!(!startup.is_in_progress());

        // mark_completed is idempotent
        startup.mark_completed();
        assert!(!startup.is_in_progress());
    }

    #[test]
    fn test_app_state_new_with_pubkey() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let state = AppState::new(pubkey);

        assert_eq!(state.user.current_user_pubkey(), pubkey);
        assert_eq!(state.timeline.len(), 0);
    }

    #[test]
    fn test_process_nostr_event_for_tab_text_note_routes_to_specified_tab() -> Result<()> {
        let current_user_pubkey = Keys::generate().public_key();
        let mut state = AppState::new(current_user_pubkey);

        // Add a user timeline tab first (before adding any events)
        let author_keys = Keys::generate();
        let author_pubkey = author_keys.public_key();
        let user_tab = FeedKind::Author(author_pubkey);
        let _ = state.timeline.update(TimelineMessage::TabAdded {
            feed: user_tab.clone(),
        });

        // Add event only to user timeline tab (this also stops loading)
        let event = create_text_note(&author_keys, "hello", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event, &user_tab);

        // Verify it was inserted only into the user timeline.
        let _ = state
            .timeline
            .update(TimelineMessage::TabSelected { index: 0 });
        assert_eq!(state.timeline.len(), 0);

        let _ = state
            .timeline
            .update(TimelineMessage::TabSelected { index: 1 });
        assert_eq!(state.timeline.len(), 1);

        // No loading_more => no status message update.
        assert_eq!(state.status_bar.message(), None);

        Ok(())
    }

    #[test]
    fn test_process_nostr_event_for_tab_propagates_timeline_command() -> Result<()> {
        let mut state = AppState::new(Keys::generate().public_key());

        let event = create_text_note(&Keys::generate(), "hello", Timestamp::from(1000))?;
        let command = state.process_nostr_event_for_tab(event, &FeedKind::Home);

        // The command returned by the timeline update is propagated to the caller
        // rather than discarded. Adding a note currently issues no follow-up command.
        assert!(command.is_none());

        Ok(())
    }

    #[test]
    fn test_process_nostr_event_for_tab_text_note_sets_status_when_load_more_completed_home(
    ) -> Result<()> {
        let current_user_pubkey = Keys::generate().public_key();
        let mut state = AppState::new(current_user_pubkey);

        // Use pre-loaded timeline for testing
        state.timeline = Timeline::default();

        // Ensure Home tab is active.
        let _ = state
            .timeline
            .update(TimelineMessage::TabSelected { index: 0 });

        // Insert an initial note so oldest_timestamp exists.
        let keys = Keys::generate();
        let event1 = create_text_note(&keys, "newer", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event1, &FeedKind::Home);

        // Start loading more. (loading_more_since = oldest_timestamp = 1000)
        let _ = state.timeline.update(TimelineMessage::LastItemSelected);
        let _ = state.timeline.update(TimelineMessage::NextItemSelected);
        assert_eq!(
            state.timeline.is_loading_more_for_feed(&FeedKind::Home),
            Some(true)
        );

        // An older event completes the LoadMore operation.
        let event2 = create_text_note(&keys, "older", Timestamp::from(500))?;
        let _ = state.process_nostr_event_for_tab(event2, &FeedKind::Home);

        assert_eq!(state.status_bar.message(), Some("[Home] loaded more"));
        assert_eq!(
            state.timeline.is_loading_more_for_feed(&FeedKind::Home),
            Some(false)
        );

        Ok(())
    }

    #[test]
    fn test_process_nostr_event_for_tab_text_note_sets_status_when_load_more_completed_user_timeline(
    ) -> Result<()> {
        let current_user_pubkey = Keys::generate().public_key();
        let mut state = AppState::new(current_user_pubkey);

        // Use pre-loaded timeline for testing
        state.timeline = Timeline::default();

        let author_keys = Keys::generate();
        let author_pubkey = author_keys.public_key();
        let Ok(author_npub) = author_pubkey.to_bech32();
        let user_tab = FeedKind::Author(author_pubkey);
        let _ = state.timeline.update(TimelineMessage::TabAdded {
            feed: user_tab.clone(),
        });

        // Insert an initial note so oldest_timestamp exists.
        let event1 = create_text_note(&author_keys, "newer", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event1, &user_tab);

        let _ = state.timeline.update(TimelineMessage::LastItemSelected);
        let _ = state.timeline.update(TimelineMessage::NextItemSelected);
        assert_eq!(
            state.timeline.is_loading_more_for_feed(&user_tab),
            Some(true)
        );

        // An older event completes the LoadMore operation.
        let event2 = create_text_note(&author_keys, "older", Timestamp::from(500))?;
        let _ = state.process_nostr_event_for_tab(event2, &user_tab);

        assert_eq!(
            state.status_bar.message(),
            Some(format!("[{}] loaded more", shorten_npub(author_npub)).as_ref())
        );
        assert_eq!(
            state.timeline.is_loading_more_for_feed(&user_tab),
            Some(false)
        );

        Ok(())
    }

    #[test]
    fn test_process_nostr_event_for_tab_metadata_inserts_profile_when_valid_json() -> Result<()> {
        let current_user_pubkey = Keys::generate().public_key();
        let mut state = AppState::new(current_user_pubkey);

        let author_keys = Keys::generate();
        let author_pubkey = author_keys.public_key();

        let metadata = Metadata::new().name("alice").display_name("Alice");
        let metadata_event = EventBuilder::new(Kind::Metadata, metadata.as_json())
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&author_keys)?;

        let _ = state.process_nostr_event_for_tab(metadata_event, &FeedKind::Home);

        let stored = state
            .user
            .get_profile(&author_pubkey)
            .expect("profile should be inserted");

        assert_eq!(
            stored,
            &Profile::new(author_pubkey, Timestamp::from(1000), metadata)
        );

        Ok(())
    }

    #[test]
    fn test_process_nostr_event_for_tab_metadata_ignores_invalid_json() -> Result<()> {
        let current_user_pubkey = Keys::generate().public_key();
        let mut state = AppState::new(current_user_pubkey);

        let author_keys = Keys::generate();
        let author_pubkey = author_keys.public_key();

        let invalid_metadata_event = EventBuilder::new(Kind::Metadata, "not json")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&author_keys)?;

        let _ = state.process_nostr_event_for_tab(invalid_metadata_event, &FeedKind::Home);

        assert_eq!(state.user.get_profile(&author_pubkey), None);
        assert_eq!(state.user.profile_count(), 0);

        Ok(())
    }

    #[test]
    fn test_open_author_timeline_switches_to_existing_tab() {
        let mut state = AppState::new(Keys::generate().public_key());
        let author_pubkey = Keys::generate().public_key();
        let feed = FeedKind::Author(author_pubkey);

        // Pre-create the author tab, then move focus back to Home.
        let _ = state
            .timeline
            .update(TimelineMessage::TabAdded { feed: feed.clone() });
        let _ = state
            .timeline
            .update(TimelineMessage::TabSelected { index: 0 });
        assert_eq!(state.timeline.active_tab_index(), 0);

        let _ = state.open_author_timeline(author_pubkey);

        // Switches to the existing tab instead of creating a duplicate.
        assert_eq!(state.timeline.tabs().len(), 2);
        assert_eq!(state.timeline.active_tab().feed(), &feed);
    }

    #[test]
    fn test_open_author_timeline_creates_new_tab_and_shows_loading() {
        let mut state = AppState::new(Keys::generate().public_key());
        let author_pubkey = Keys::generate().public_key();
        let Ok(author_npub) = author_pubkey.to_bech32();
        let feed = FeedKind::Author(author_pubkey);

        let _ = state.open_author_timeline(author_pubkey);

        // A new author tab is created, focused, and a loading status is shown.
        assert_eq!(state.timeline.tabs().len(), 2);
        assert_eq!(state.timeline.active_tab().feed(), &feed);
        assert_eq!(
            state.status_bar.message(),
            Some(format!("[{author_npub}] loading...").as_str())
        );
    }

    #[test]
    fn test_open_mention_tab_switches_to_existing_tab() {
        let mut state = AppState::new(Keys::generate().public_key());
        let feed = FeedKind::Mention;

        // Pre-create the mention tab, then move focus back to Home.
        let _ = state
            .timeline
            .update(TimelineMessage::TabAdded { feed: feed.clone() });
        let _ = state
            .timeline
            .update(TimelineMessage::TabSelected { index: 0 });
        assert_eq!(state.timeline.active_tab_index(), 0);

        let _ = state.open_mention_tab();

        // Switches to the existing tab instead of creating a duplicate.
        assert_eq!(state.timeline.tabs().len(), 2);
        assert_eq!(state.timeline.active_tab().feed(), &feed);
    }

    #[test]
    fn test_open_mention_tab_creates_new_tab_and_shows_loading() {
        let mut state = AppState::new(Keys::generate().public_key());
        let feed = FeedKind::Mention;

        let _ = state.open_mention_tab();

        // A new mention tab is created, focused, and a loading status is shown.
        assert_eq!(state.timeline.tabs().len(), 2);
        assert_eq!(state.timeline.active_tab().feed(), &feed);
        assert_eq!(state.status_bar.message(), Some("[Mention] loading..."));
    }

    #[test]
    fn test_close_current_tab_removes_active_tab() {
        let mut state = AppState::new(Keys::generate().public_key());
        let author_pubkey = Keys::generate().public_key();
        let feed = FeedKind::Author(author_pubkey);
        let _ = state.timeline.update(TimelineMessage::TabAdded { feed });
        assert_eq!(state.timeline.active_tab_index(), 1);

        let _ = state.close_current_tab();

        // The active author tab is removed and focus falls back to Home.
        assert_eq!(state.timeline.tabs().len(), 1);
        assert_eq!(state.timeline.active_tab().feed(), &FeedKind::Home);
    }

    #[test]
    fn test_close_current_tab_keeps_home_tab() {
        let mut state = AppState::new(Keys::generate().public_key());
        assert_eq!(state.timeline.active_tab().feed(), &FeedKind::Home);

        let _ = state.close_current_tab();

        // Home cannot be closed, so the timeline is unchanged.
        assert_eq!(state.timeline.tabs().len(), 1);
        assert_eq!(state.timeline.active_tab().feed(), &FeedKind::Home);
    }

    #[test]
    fn test_react_to_selected_without_selection_is_noop() {
        let mut state = AppState::new(Keys::generate().public_key());

        let command = state.react_to_selected();

        assert!(command.is_none());
        assert_eq!(state.status_bar.message(), None);
    }

    #[test]
    fn test_react_to_selected_sets_status() -> Result<()> {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let Ok(note1) = event.id.to_bech32();
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);
        let _ = state.timeline.update(TimelineMessage::FirstItemSelected);

        let _ = state.react_to_selected();

        assert_eq!(
            state.status_bar.message(),
            Some(format!("[Reacted] {note1}").as_str())
        );

        Ok(())
    }

    #[test]
    fn test_repost_selected_sets_status() -> Result<()> {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let Ok(note1) = event.id.to_bech32();
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);
        let _ = state.timeline.update(TimelineMessage::FirstItemSelected);

        let _ = state.repost_selected();

        assert_eq!(
            state.status_bar.message(),
            Some(format!("[Reposted] {note1}").as_str())
        );

        Ok(())
    }

    #[test]
    fn test_start_reply_without_selection_is_noop() {
        let mut state = AppState::new(Keys::generate().public_key());

        let _ = state.start_reply();

        assert!(!state.editor.is_active());
        assert_eq!(state.editor.reply_target(), None);
    }

    #[test]
    fn test_start_reply_sets_reply_context() -> Result<()> {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event.clone(), &FeedKind::Home);
        let _ = state.timeline.update(TimelineMessage::FirstItemSelected);

        let _ = state.start_reply();

        assert!(state.editor.is_active());
        assert_eq!(state.editor.reply_target(), Some(&event));

        Ok(())
    }

    #[test]
    fn test_submit_note_posts_content_and_resets_editor() {
        let mut state = AppState::new(Keys::generate().public_key());

        // Compose "hi" in the editor.
        state.editor.update(EditorMessage::ComposingStarted);
        for code in ["h", "i"] {
            state.editor.update(EditorMessage::KeyEventReceived {
                event: KeyEvent::new(
                    KeyCode::Char(code.chars().next().expect("single char")),
                    KeyModifiers::NONE,
                ),
            });
        }

        let _ = state.submit_note();

        assert_eq!(state.status_bar.message(), Some("[Posted] hi"));
        assert!(!state.editor.is_active());
    }

    #[test]
    fn test_submit_note_as_reply_posts_and_resets_editor() -> Result<()> {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        // Select a note and start replying to it.
        let event = create_text_note(&keys, "original", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);
        let _ = state.timeline.update(TimelineMessage::FirstItemSelected);
        let _ = state.start_reply();

        // Type a reply and submit it (exercises the NIP-10 reply branch).
        state.editor.update(EditorMessage::KeyEventReceived {
            event: KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
        });

        let _ = state.submit_note();

        assert_eq!(state.status_bar.message(), Some("[Posted] y"));
        assert!(!state.editor.is_active());

        Ok(())
    }

    #[test]
    fn test_publish_music_status_sets_status() {
        let mut state = AppState::new(Keys::generate().public_key());

        let _ = state.publish_music_status(create_track("Song"));

        assert_eq!(state.status_bar.message(), Some("[Music] Song - Artist"));
    }

    #[test]
    fn test_publish_music_status_ignores_invalid_track() {
        let mut state = AppState::new(Keys::generate().public_key());

        // A track with an empty title cannot form a status, so nothing happens.
        let _ = state.publish_music_status(create_track(""));

        assert_eq!(state.status_bar.message(), None);
    }

    #[test]
    fn test_on_connection_ready_marks_ready_and_shows_loading() {
        let mut state = AppState::new(Keys::generate().public_key());
        let (tx, _rx) = mpsc::unbounded_channel();

        let _ = state.on_connection_ready(tx);

        assert!(state.nostr.is_ready());
        assert_eq!(state.status_bar.message(), Some("[Home] loading..."));
    }

    #[test]
    fn test_route_relay_event_routes_to_owning_tab() -> Result<()> {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        // Associate a subscription with the Home tab.
        let sub_id = SubscriptionId::new("home_sub");
        let _ = state.nostr.update(NostrMessage::SubscriptionCreated {
            feed: FeedKind::Home,
            sub_id: sub_id.clone(),
        });

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let _ = state.route_relay_event(&sub_id, event);

        // The event is routed to the Home tab, and the first event ends startup.
        assert_eq!(state.timeline.len(), 1);
        assert!(!state.startup.is_in_progress());
        assert_eq!(state.status_bar.message(), Some("[Home] loaded"));

        Ok(())
    }

    #[test]
    fn test_load_more_timeline_without_events_is_noop() {
        let mut state = AppState::new(Keys::generate().public_key());

        // No events => no oldest timestamp => nothing to paginate.
        let _ = state.load_more_timeline();

        assert_eq!(state.status_bar.message(), None);
    }

    #[test]
    fn test_load_more_timeline_sets_loading_status() -> Result<()> {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);

        let _ = state.load_more_timeline();

        assert_eq!(state.status_bar.message(), Some("[Home] loading more..."));

        Ok(())
    }

    #[test]
    fn test_route_relay_event_ignores_untracked_subscription() -> Result<()> {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let _ = state.route_relay_event(&SubscriptionId::new("unknown"), event);

        // No tab owns the subscription, so the event is dropped, but the
        // "loaded" status is still shown while starting up.
        assert_eq!(state.timeline.len(), 0);
        assert_eq!(state.status_bar.message(), Some("[Home] loaded"));

        Ok(())
    }

    // --- Dispatch seam: each use case sends the expected NostrCommand ---
    //
    // The model only emits commands once connected, and the application owns the
    // sender, so these tests connect via `on_connection_ready` (which injects the
    // sender and marks the worker ready) and then drain the receiver. They guard
    // the application <-> worker wiring that visible-side-effect tests miss — e.g.
    // the #458 regression where `load_more_timeline` set the status but never
    // dispatched `LoadMore`.

    fn connected_state() -> (AppState<'static>, mpsc::UnboundedReceiver<NostrCommand>) {
        let mut state = AppState::new(Keys::generate().public_key());
        let (tx, rx) = mpsc::unbounded_channel();
        let _ = state.on_connection_ready(tx);
        (state, rx)
    }

    #[test]
    fn test_on_connection_ready_dispatches_nothing() {
        let (_state, mut rx) = connected_state();

        // Becoming ready must not, by itself, send any command.
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_open_author_timeline_dispatches_subscribe() {
        let (mut state, mut rx) = connected_state();
        let author_pubkey = Keys::generate().public_key();

        let _ = state.open_author_timeline(author_pubkey);

        assert_eq!(
            rx.try_recv(),
            Ok(NostrCommand::Subscribe {
                feed: FeedKind::Author(author_pubkey),
            })
        );
    }

    #[test]
    fn test_close_current_tab_dispatches_unsubscribe() {
        let (mut state, mut rx) = connected_state();
        let feed = FeedKind::Author(Keys::generate().public_key());
        let sub_id = SubscriptionId::new("author_sub");

        // Open an author tab and register a subscription for it without going
        // through `open_author_timeline` (which would also emit `Subscribe`).
        let _ = state
            .timeline
            .update(TimelineMessage::TabAdded { feed: feed.clone() });
        let _ = state.track_subscription_created(feed, sub_id.clone());

        let _ = state.close_current_tab();

        assert_eq!(
            rx.try_recv(),
            Ok(NostrCommand::Unsubscribe {
                subscription_ids: vec![sub_id],
            })
        );
    }

    #[test]
    fn test_react_to_selected_dispatches_send_event() -> Result<()> {
        let (mut state, mut rx) = connected_state();
        let keys = Keys::generate();

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);
        let _ = state.timeline.update(TimelineMessage::FirstItemSelected);

        let _ = state.react_to_selected();

        assert!(matches!(
            rx.try_recv(),
            Ok(NostrCommand::SendEventBuilder { .. })
        ));

        Ok(())
    }

    #[test]
    fn test_repost_selected_dispatches_send_event() -> Result<()> {
        let (mut state, mut rx) = connected_state();
        let keys = Keys::generate();

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);
        let _ = state.timeline.update(TimelineMessage::FirstItemSelected);

        let _ = state.repost_selected();

        assert!(matches!(
            rx.try_recv(),
            Ok(NostrCommand::SendEventBuilder { .. })
        ));

        Ok(())
    }

    #[test]
    fn test_submit_note_dispatches_send_event() {
        let (mut state, mut rx) = connected_state();

        state.editor.update(EditorMessage::ComposingStarted);
        state.editor.update(EditorMessage::KeyEventReceived {
            event: KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
        });

        let _ = state.submit_note();

        assert!(matches!(
            rx.try_recv(),
            Ok(NostrCommand::SendEventBuilder { .. })
        ));
    }

    #[test]
    fn test_publish_music_status_dispatches_send_event() {
        let (mut state, mut rx) = connected_state();

        let _ = state.publish_music_status(create_track("Song"));

        assert!(matches!(
            rx.try_recv(),
            Ok(NostrCommand::SendEventBuilder { .. })
        ));
    }

    #[test]
    fn test_load_more_timeline_dispatches_load_more() -> Result<()> {
        // Regression guard for #458: the status-only test passed while the
        // `LoadMore` dispatch was missing.
        let (mut state, mut rx) = connected_state();
        let keys = Keys::generate();

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);

        let _ = state.load_more_timeline();

        assert_eq!(
            rx.try_recv(),
            Ok(NostrCommand::LoadMore {
                feed: FeedKind::Home,
                since: Timestamp::from(1000),
            })
        );

        Ok(())
    }

    #[test]
    fn test_close_connection_dispatches_shutdown() {
        let (mut state, mut rx) = connected_state();

        let _ = state.close_connection();

        assert_eq!(rx.try_recv(), Ok(NostrCommand::Shutdown));
    }

    #[test]
    fn test_show_error_sets_error_status() {
        let mut state = AppState::new(Keys::generate().public_key());

        let _ = state.show_error("boom".to_owned());

        assert_eq!(state.status_bar.message(), Some("[ERR: System] boom"));
    }

    #[test]
    fn test_notify_subscription_error_sets_error_status() {
        let mut state = AppState::new(Keys::generate().public_key());

        let _ = state.notify_subscription_error(CommandError::SendEventFailed {
            error: "x".to_owned(),
        });

        let message = state.status_bar.message().expect("status set");
        assert!(message.starts_with("[ERR: Nostr]"));
    }
}
