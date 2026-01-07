use std::cmp::Ordering;

use nostr_sdk::prelude::*;

/// A wrapper around nostr_sdk::Event that provides additional functionality
/// such as sorting and domain-specific operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventWrapper {
    pub event: Event,
}

impl EventWrapper {
    pub fn new(event: Event) -> Self {
        Self { event }
    }

    /// Get the last event ID from e tags (for replies/reactions/reposts)
    ///
    /// This method extracts the event ID from the last 'e' tag in the event's tags.
    /// According to NIP-10, the last 'e' tag typically references the event being replied to,
    /// reacted to, or reposted.
    ///
    /// # Returns
    /// - `Some(EventId)` if at least one 'e' tag is found
    /// - `None` if no 'e' tags are present
    pub fn last_event_id_from_tags(&self) -> Option<EventId> {
        use nostr_sdk::nostr::{Alphabet, SingleLetterTag, TagKind, TagStandard};

        self.event
            .tags
            .filter_standardized(TagKind::SingleLetter(SingleLetterTag::lowercase(
                Alphabet::E,
            )))
            .last()
            .and_then(|tag| match tag {
                TagStandard::Event { event_id, .. } => Some(*event_id),
                _ => None,
            })
    }
}

impl PartialOrd for EventWrapper {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EventWrapper {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.event.created_at == other.event.created_at {
            self.event.id.cmp(&other.event.id)
        } else {
            self.event.created_at.cmp(&other.event.created_at)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_last_event_id_from_tags_with_single_e_tag() {
        // Create an event with a single 'e' tag
        let keys = Keys::generate();
        let target_event_id = EventId::all_zeros(); // Use a known event ID

        let event = EventBuilder::text_note("test reply")
            .tag(Tag::event(target_event_id))
            .sign_with_keys(&keys)
            .expect("Failed to sign event");

        let wrapper = EventWrapper::new(event);

        // Should return the event ID from the 'e' tag
        assert_eq!(wrapper.last_event_id_from_tags(), Some(target_event_id));
    }

    #[test]
    fn test_last_event_id_from_tags_with_multiple_e_tags() {
        // Create an event with multiple 'e' tags (NIP-10 style reply)
        let keys = Keys::generate();
        let root_event_id = EventId::all_zeros();
        let reply_event_id = EventId::from_slice(&[1u8; 32]).expect("Valid event ID");

        let event = EventBuilder::text_note("test reply")
            .tag(Tag::event(root_event_id)) // First 'e' tag (root)
            .tag(Tag::event(reply_event_id)) // Last 'e' tag (reply target)
            .sign_with_keys(&keys)
            .expect("Failed to sign event");

        let wrapper = EventWrapper::new(event);

        // Should return the LAST 'e' tag's event ID
        assert_eq!(wrapper.last_event_id_from_tags(), Some(reply_event_id));
    }

    #[test]
    fn test_last_event_id_from_tags_with_no_e_tags() {
        // Create an event without any 'e' tags
        let keys = Keys::generate();

        let event = EventBuilder::text_note("test note without references")
            .sign_with_keys(&keys)
            .expect("Failed to sign event");

        let wrapper = EventWrapper::new(event);

        // Should return None when there are no 'e' tags
        assert_eq!(wrapper.last_event_id_from_tags(), None);
    }

    #[test]
    fn test_last_event_id_from_tags_with_mixed_tags() {
        // Create an event with various tag types including 'e' tags
        let keys = Keys::generate();
        let pubkey = PublicKey::from_slice(&[2u8; 32]).expect("Valid public key");
        let target_event_id = EventId::from_slice(&[3u8; 32]).expect("Valid event ID");

        let event = EventBuilder::text_note("test with mixed tags")
            .tag(Tag::public_key(pubkey)) // 'p' tag
            .tag(Tag::event(target_event_id)) // 'e' tag
            .tag(Tag::hashtag("nostr")) // 't' tag
            .sign_with_keys(&keys)
            .expect("Failed to sign event");

        let wrapper = EventWrapper::new(event);

        // Should correctly find the 'e' tag among other tag types
        assert_eq!(wrapper.last_event_id_from_tags(), Some(target_event_id));
    }

    #[test]
    fn test_last_event_id_from_tags_ignores_non_e_tags() {
        // Create an event with only non-'e' tags
        let keys = Keys::generate();
        let pubkey = PublicKey::from_slice(&[2u8; 32]).expect("Valid public key");

        let event = EventBuilder::text_note("test with only p tags")
            .tag(Tag::public_key(pubkey))
            .tag(Tag::hashtag("test"))
            .sign_with_keys(&keys)
            .expect("Failed to sign event");

        let wrapper = EventWrapper::new(event);

        // Should return None when there are no 'e' tags
        assert_eq!(wrapper.last_event_id_from_tags(), None);
    }

    #[test]
    fn test_event_wrapper_ordering() {
        // Test that EventWrapper correctly orders by timestamp
        let keys = Keys::generate();
        let now = Timestamp::now();

        let event1 = EventBuilder::text_note("older")
            .custom_created_at(now - 10)
            .sign_with_keys(&keys)
            .expect("Failed to sign event");

        let event2 = EventBuilder::text_note("newer")
            .custom_created_at(now)
            .sign_with_keys(&keys)
            .expect("Failed to sign event");

        let wrapper1 = EventWrapper::new(event1);
        let wrapper2 = EventWrapper::new(event2);

        // wrapper1 (older) should be less than wrapper2 (newer)
        assert!(wrapper1 < wrapper2);
    }
}
