use nostr_sdk::prelude::*;
use tears::prelude::*;

use crate::{
    core::message::AppMsg,
    domain::nostr::{nip10::ReplyTagsBuilder, Profile},
    infrastructure::config::Config,
    model::{
        editor::{Editor, Message as EditorMessage},
        fps::Fps,
        nostr::{Message as NostrMessage, Nostr},
        status_bar::{Message, StatusBar},
        timeline::{
            tab::TimelineTabType, text_note::TextNote, Message as TimelineMessage, Timeline,
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
        tab_type: &TimelineTabType,
    ) -> Command<AppMsg> {
        // Receiving any event means startup has produced its first results.
        self.startup.mark_completed();

        match event.kind {
            Kind::TextNote => {
                let current_loading_more_state = self.timeline.is_loading_more_for_tab(tab_type);

                let command = self.timeline.update(TimelineMessage::NoteAddedToTab {
                    event,
                    tab_type: tab_type.clone(),
                });

                let new_loading_more_state = self.timeline.is_loading_more_for_tab(tab_type);

                if current_loading_more_state == Some(true) && new_loading_more_state == Some(false)
                {
                    let tab_title = self.timeline.active_tab().tab_title(self.user.profiles());
                    self.status_bar.update(Message::MessageChanged {
                        label: tab_title,
                        message: "loaded more".to_owned(),
                    });
                }

                command
            }
            Kind::Metadata => {
                // Metadata is shared across all tabs
                if let Ok(metadata) = Metadata::from_json(event.content.clone()) {
                    let profile = Profile::new(event.pubkey, event.created_at, metadata);
                    self.user.insert_newer_profile(profile);
                }
                Command::none()
            }
            Kind::Repost => self.timeline.update(TimelineMessage::RepostAdded { event }),
            Kind::Reaction => self
                .timeline
                .update(TimelineMessage::ReactionAdded { event }),
            Kind::ZapReceipt => self
                .timeline
                .update(TimelineMessage::ZapReceiptAdded { event }),
            _ => Command::none(),
        }
    }

    /// Open (or switch to) the author timeline tab for the given pubkey.
    ///
    /// If a tab for this author already exists, it is selected. Otherwise a new
    /// tab is created, a subscription is requested, and a "loading" status
    /// message is shown (or an error message if the tab could not be created).
    pub fn open_author_timeline(&mut self, author_pubkey: PublicKey) -> Command<AppMsg> {
        let Ok(author_npub) = author_pubkey.to_bech32();
        let tab_type = TimelineTabType::UserTimeline {
            pubkey: author_pubkey,
        };

        // Tab already open: just switch to it.
        if let Some(index) = self.timeline.find_tab_by_type(&tab_type) {
            let _ = self.timeline.update(TimelineMessage::TabSelected { index });
            return Command::none();
        }

        // Otherwise create it, then subscribe and show the loading status.
        let _ = self.timeline.update(TimelineMessage::TabAdded {
            tab_type: tab_type.clone(),
        });

        if self.timeline.find_tab_by_type(&tab_type).is_some() {
            log::info!("Created new author timeline for {author_npub}");

            self.nostr
                .update(NostrMessage::SubscriptionRequested { tab_type });

            self.status_bar.update(Message::MessageChanged {
                label: author_npub,
                message: "loading...".to_owned(),
            });
        } else {
            log::error!("Failed to create author timeline");

            self.status_bar.update(Message::ErrorMessageChanged {
                label: author_npub,
                message: "failed to open tab".to_owned(),
            });
        }

        Command::none()
    }

    /// Close the currently active tab and unsubscribe from its subscriptions.
    ///
    /// The Home tab cannot be closed; the [`Timeline`] enforces this, so calling
    /// this while Home is active is a no-op apart from the (no-op) unsubscribe.
    pub fn close_current_tab(&mut self) -> Command<AppMsg> {
        let current_index = self.timeline.active_tab_index();

        // Capture the tab type before removing the tab.
        let tab_type = self.timeline.active_tab().tab_type().clone();

        let _ = self.timeline.update(TimelineMessage::TabRemoved {
            index: current_index,
        });

        // Unsubscribe the subscriptions associated with the closed tab.
        self.nostr
            .update(NostrMessage::SubscriptionClosed { tab_type });

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

        self.nostr
            .update(NostrMessage::EventSubmitted { event_builder });

        self.status_bar.update(Message::MessageChanged {
            label: label.to_owned(),
            message: note_id,
        });

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

        self.nostr
            .update(NostrMessage::EventSubmitted { event_builder });

        self.status_bar.update(Message::MessageChanged {
            label: "Posted".to_owned(),
            message: content,
        });

        // Clear UI state.
        self.editor.update(EditorMessage::ComposingCanceled);

        Command::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{nostr::Profile, text::shorten_npub};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
        let user_tab = TimelineTabType::UserTimeline {
            pubkey: author_pubkey,
        };
        let _ = state.timeline.update(TimelineMessage::TabAdded {
            tab_type: user_tab.clone(),
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
        let command = state.process_nostr_event_for_tab(event, &TimelineTabType::Home);

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
        let _ = state.process_nostr_event_for_tab(event1, &TimelineTabType::Home);

        // Start loading more. (loading_more_since = oldest_timestamp = 1000)
        let _ = state.timeline.update(TimelineMessage::LastItemSelected);
        let _ = state.timeline.update(TimelineMessage::NextItemSelected);
        assert_eq!(
            state
                .timeline
                .is_loading_more_for_tab(&TimelineTabType::Home),
            Some(true)
        );

        // An older event completes the LoadMore operation.
        let event2 = create_text_note(&keys, "older", Timestamp::from(500))?;
        let _ = state.process_nostr_event_for_tab(event2, &TimelineTabType::Home);

        assert_eq!(state.status_bar.message(), Some("[Home] loaded more"));
        assert_eq!(
            state
                .timeline
                .is_loading_more_for_tab(&TimelineTabType::Home),
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
        let user_tab = TimelineTabType::UserTimeline {
            pubkey: author_pubkey,
        };
        let _ = state.timeline.update(TimelineMessage::TabAdded {
            tab_type: user_tab.clone(),
        });

        // Insert an initial note so oldest_timestamp exists.
        let event1 = create_text_note(&author_keys, "newer", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event1, &user_tab);

        let _ = state.timeline.update(TimelineMessage::LastItemSelected);
        let _ = state.timeline.update(TimelineMessage::NextItemSelected);
        assert_eq!(
            state.timeline.is_loading_more_for_tab(&user_tab),
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
            state.timeline.is_loading_more_for_tab(&user_tab),
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

        let _ = state.process_nostr_event_for_tab(metadata_event, &TimelineTabType::Home);

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

        let _ = state.process_nostr_event_for_tab(invalid_metadata_event, &TimelineTabType::Home);

        assert_eq!(state.user.get_profile(&author_pubkey), None);
        assert_eq!(state.user.profile_count(), 0);

        Ok(())
    }

    #[test]
    fn test_open_author_timeline_switches_to_existing_tab() {
        let mut state = AppState::new(Keys::generate().public_key());
        let author_pubkey = Keys::generate().public_key();
        let tab_type = TimelineTabType::UserTimeline {
            pubkey: author_pubkey,
        };

        // Pre-create the author tab, then move focus back to Home.
        let _ = state.timeline.update(TimelineMessage::TabAdded {
            tab_type: tab_type.clone(),
        });
        let _ = state
            .timeline
            .update(TimelineMessage::TabSelected { index: 0 });
        assert_eq!(state.timeline.active_tab_index(), 0);

        let _ = state.open_author_timeline(author_pubkey);

        // Switches to the existing tab instead of creating a duplicate.
        assert_eq!(state.timeline.tabs().len(), 2);
        assert_eq!(state.timeline.active_tab().tab_type(), &tab_type);
    }

    #[test]
    fn test_open_author_timeline_creates_new_tab_and_shows_loading() {
        let mut state = AppState::new(Keys::generate().public_key());
        let author_pubkey = Keys::generate().public_key();
        let Ok(author_npub) = author_pubkey.to_bech32();
        let tab_type = TimelineTabType::UserTimeline {
            pubkey: author_pubkey,
        };

        let _ = state.open_author_timeline(author_pubkey);

        // A new author tab is created, focused, and a loading status is shown.
        assert_eq!(state.timeline.tabs().len(), 2);
        assert_eq!(state.timeline.active_tab().tab_type(), &tab_type);
        assert_eq!(
            state.status_bar.message(),
            Some(format!("[{author_npub}] loading...").as_str())
        );
    }

    #[test]
    fn test_close_current_tab_removes_active_tab() {
        let mut state = AppState::new(Keys::generate().public_key());
        let author_pubkey = Keys::generate().public_key();
        let tab_type = TimelineTabType::UserTimeline {
            pubkey: author_pubkey,
        };
        let _ = state
            .timeline
            .update(TimelineMessage::TabAdded { tab_type });
        assert_eq!(state.timeline.active_tab_index(), 1);

        let _ = state.close_current_tab();

        // The active author tab is removed and focus falls back to Home.
        assert_eq!(state.timeline.tabs().len(), 1);
        assert_eq!(
            state.timeline.active_tab().tab_type(),
            &TimelineTabType::Home
        );
    }

    #[test]
    fn test_close_current_tab_keeps_home_tab() {
        let mut state = AppState::new(Keys::generate().public_key());
        assert_eq!(
            state.timeline.active_tab().tab_type(),
            &TimelineTabType::Home
        );

        let _ = state.close_current_tab();

        // Home cannot be closed, so the timeline is unchanged.
        assert_eq!(state.timeline.tabs().len(), 1);
        assert_eq!(
            state.timeline.active_tab().tab_type(),
            &TimelineTabType::Home
        );
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
        let _ = state.process_nostr_event_for_tab(event, &TimelineTabType::Home);
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
        let _ = state.process_nostr_event_for_tab(event, &TimelineTabType::Home);
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
        let _ = state.process_nostr_event_for_tab(event.clone(), &TimelineTabType::Home);
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
        let _ = state.process_nostr_event_for_tab(event, &TimelineTabType::Home);
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
}
