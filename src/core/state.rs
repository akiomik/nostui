use nostr_sdk::prelude::*;

use crate::{
    domain::nostr::Profile,
    infrastructure::config::Config,
    model::{
        editor::Editor,
        fps::Fps,
        nostr::Nostr,
        status_bar::{Message, StatusBar},
        timeline::{tab::TimelineTabType, Message as TimelineMessage, Timeline},
    },
};

pub mod user;

pub use user::UserState;

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
    // TODO: Handle commands
    pub fn process_nostr_event_for_tab(&mut self, event: Event, tab_type: &TimelineTabType) {
        match event.kind {
            Kind::TextNote => {
                let current_loading_more_state = self.timeline.is_loading_more_for_tab(tab_type);

                let _ = self.timeline.update(TimelineMessage::NoteAddedToTab {
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
            }
            Kind::Metadata => {
                // Metadata is shared across all tabs
                if let Ok(metadata) = Metadata::from_json(event.content.clone()) {
                    let profile = Profile::new(event.pubkey, event.created_at, metadata);
                    self.user.insert_newer_profile(profile);
                }
            }
            Kind::Repost => {
                let _ = self.timeline.update(TimelineMessage::RepostAdded { event });
            }
            Kind::Reaction => {
                let _ = self
                    .timeline
                    .update(TimelineMessage::ReactionAdded { event });
            }
            Kind::ZapReceipt => {
                let _ = self
                    .timeline
                    .update(TimelineMessage::ZapReceiptAdded { event });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{nostr::Profile, text::shorten_npub};

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
        assert!(state.timeline.is_loading());
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
        state.process_nostr_event_for_tab(event, &user_tab);

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
    fn test_process_nostr_event_for_tab_text_note_sets_status_when_load_more_completed_home(
    ) -> Result<()> {
        let current_user_pubkey = Keys::generate().public_key();
        let mut state = AppState::new(current_user_pubkey);

        // Use pre-loaded timeline for testing
        state.timeline = Timeline::new_loaded();

        // Ensure Home tab is active.
        let _ = state
            .timeline
            .update(TimelineMessage::TabSelected { index: 0 });

        // Insert an initial note so oldest_timestamp exists.
        let keys = Keys::generate();
        let event1 = create_text_note(&keys, "newer", Timestamp::from(1000))?;
        state.process_nostr_event_for_tab(event1, &TimelineTabType::Home);

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
        state.process_nostr_event_for_tab(event2, &TimelineTabType::Home);

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
        state.timeline = Timeline::new_loaded();

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
        state.process_nostr_event_for_tab(event1, &user_tab);

        let _ = state.timeline.update(TimelineMessage::LastItemSelected);
        let _ = state.timeline.update(TimelineMessage::NextItemSelected);
        assert_eq!(
            state.timeline.is_loading_more_for_tab(&user_tab),
            Some(true)
        );

        // An older event completes the LoadMore operation.
        let event2 = create_text_note(&author_keys, "older", Timestamp::from(500))?;
        state.process_nostr_event_for_tab(event2, &user_tab);

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

        state.process_nostr_event_for_tab(metadata_event, &TimelineTabType::Home);

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

        state.process_nostr_event_for_tab(invalid_metadata_event, &TimelineTabType::Home);

        assert_eq!(state.user.get_profile(&author_pubkey), None);
        assert_eq!(state.user.profile_count(), 0);

        Ok(())
    }
}
