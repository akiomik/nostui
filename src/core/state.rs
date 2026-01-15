use nostr_sdk::prelude::*;
use timeline::TimelineTabType;

use crate::{domain::nostr::Profile, infrastructure::config::Config};

pub mod editor;
pub mod fps;
pub mod nostr;
pub mod system;
pub mod timeline;
pub mod user;

pub use editor::EditorState;
pub use fps::FpsState;
pub use nostr::NostrState;
pub use system::SystemState;
pub use timeline::TimelineState;
pub use user::UserState;

/// Unified application state
#[derive(Debug, Default)]
pub struct AppState {
    pub timeline: TimelineState,
    pub editor: EditorState,
    pub user: UserState,
    pub system: SystemState,
    pub nostr: NostrState,
    pub config: ConfigState,
    pub fps: FpsState,
}

/// Configuration state - holds all user-configurable settings
#[derive(Debug, Clone, Default)]
pub struct ConfigState {
    /// Current configuration loaded from file
    pub config: Config,
}

impl AppState {
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
    pub fn process_nostr_event_for_tab(&mut self, event: Event, tab_type: &TimelineTabType) {
        match event.kind {
            Kind::TextNote => {
                let (_was_inserted, loading_completed) =
                    self.timeline.add_note_to_tab(event, tab_type);

                if loading_completed {
                    match tab_type {
                        TimelineTabType::Home => {
                            self.system.set_status_message("[Home] Loaded more");
                        }
                        TimelineTabType::UserTimeline { .. } => {
                            self.system.set_status_message("[User] Loaded more");
                        }
                    }
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
                self.timeline.add_repost(event);
            }
            Kind::Reaction => {
                self.timeline.add_reaction(event);
            }
            Kind::ZapReceipt => {
                self.timeline.add_zap_receipt(event);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_text_note(keys: &Keys, content: &str, created_at: Timestamp) -> Result<Event> {
        Ok(EventBuilder::text_note(content)
            .custom_created_at(created_at)
            .sign_with_keys(keys)?)
    }

    #[test]
    fn test_app_state_default() {
        let state = AppState::default();

        assert_eq!(state.timeline.len(), 0);
        assert!(!state.editor.is_composing());
        assert!(state.system.is_loading());
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
        state.system.stop_loading();

        // Add a user timeline tab.
        let author_keys = Keys::generate();
        let author_pubkey = author_keys.public_key();
        let user_tab = TimelineTabType::UserTimeline {
            pubkey: author_pubkey,
        };
        state.timeline.add_tab(user_tab.clone())?;

        let event = create_text_note(&author_keys, "hello", Timestamp::from(1000))?;
        state.process_nostr_event_for_tab(event, &user_tab);

        // Verify it was inserted only into the user timeline.
        state.timeline.select_tab(0);
        assert_eq!(state.timeline.len(), 0);

        state.timeline.select_tab(1);
        assert_eq!(state.timeline.len(), 1);

        // No loading_more => no status message update.
        assert_eq!(state.system.status_message(), None);

        Ok(())
    }

    #[test]
    fn test_process_nostr_event_for_tab_text_note_sets_status_when_load_more_completed_home(
    ) -> Result<()> {
        let current_user_pubkey = Keys::generate().public_key();
        let mut state = AppState::new(current_user_pubkey);
        state.system.stop_loading();

        // Ensure Home tab is active.
        state.timeline.select_tab(0);

        // Insert an initial note so oldest_timestamp exists.
        let keys = Keys::generate();
        let event1 = create_text_note(&keys, "newer", Timestamp::from(1000))?;
        state.process_nostr_event_for_tab(event1, &TimelineTabType::Home);

        // Start loading more. (loading_more_since = oldest_timestamp = 1000)
        state.timeline.start_loading_more();
        assert!(state.timeline.is_loading_more());

        // An older event completes the LoadMore operation.
        let event2 = create_text_note(&keys, "older", Timestamp::from(500))?;
        state.process_nostr_event_for_tab(event2, &TimelineTabType::Home);

        assert_eq!(
            state.system.status_message(),
            Some(&"[Home] Loaded more".to_owned())
        );
        assert!(!state.timeline.is_loading_more());

        Ok(())
    }

    #[test]
    fn test_process_nostr_event_for_tab_text_note_sets_status_when_load_more_completed_user_timeline(
    ) -> Result<()> {
        let current_user_pubkey = Keys::generate().public_key();
        let mut state = AppState::new(current_user_pubkey);
        state.system.stop_loading();

        let author_keys = Keys::generate();
        let author_pubkey = author_keys.public_key();
        let user_tab = TimelineTabType::UserTimeline {
            pubkey: author_pubkey,
        };
        state.timeline.add_tab(user_tab.clone())?;

        // Make the user tab active because loading state is tracked per active tab.
        state.timeline.select_tab(1);

        // Insert an initial note so oldest_timestamp exists.
        let event1 = create_text_note(&author_keys, "newer", Timestamp::from(1000))?;
        state.process_nostr_event_for_tab(event1, &user_tab);

        state.timeline.start_loading_more();
        assert!(state.timeline.is_loading_more());

        // An older event completes the LoadMore operation.
        let event2 = create_text_note(&author_keys, "older", Timestamp::from(500))?;
        state.process_nostr_event_for_tab(event2, &user_tab);

        assert_eq!(
            state.system.status_message(),
            Some(&"[User] Loaded more".to_owned())
        );
        assert!(!state.timeline.is_loading_more());

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

    #[test]
    fn test_process_nostr_event_for_tab_reaction_repost_zap_are_added_globally() -> Result<()> {
        let current_user_pubkey = Keys::generate().public_key();
        let mut state = AppState::new(current_user_pubkey);

        let author_keys = Keys::generate();

        let target = create_text_note(&author_keys, "target", Timestamp::from(1000))?;
        let target_id = target.id;
        state.process_nostr_event_for_tab(target.clone(), &TimelineTabType::Home);

        // Reaction
        let reaction = EventBuilder::reaction(&target, "+").sign_with_keys(&author_keys)?;
        let reaction_id = reaction.id;
        state.process_nostr_event_for_tab(reaction, &TimelineTabType::Home);
        assert_eq!(state.timeline.reactions_for(&target_id).len(), 1);
        assert!(state
            .timeline
            .reactions_for(&target_id)
            .contains(&reaction_id));

        // Repost
        let repost = EventBuilder::repost(&target, None).sign_with_keys(&author_keys)?;
        let repost_id = repost.id;
        state.process_nostr_event_for_tab(repost, &TimelineTabType::Home);
        assert_eq!(state.timeline.reposts_for(&target_id).len(), 1);
        assert!(state.timeline.reposts_for(&target_id).contains(&repost_id));

        // ZapReceipt (Kind 9735)
        let zap = EventBuilder::new(Kind::ZapReceipt, "zap")
            .tag(Tag::event(target_id))
            .sign_with_keys(&author_keys)?;
        let zap_id = zap.id;
        state.process_nostr_event_for_tab(zap, &TimelineTabType::Home);
        assert_eq!(state.timeline.zap_receipts_for(&target_id).len(), 1);
        assert!(state
            .timeline
            .zap_receipts_for(&target_id)
            .contains(&zap_id));

        Ok(())
    }
}
