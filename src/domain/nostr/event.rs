use std::cmp::Ordering;

use nostr_sdk::prelude::*;

/// A lightweight wrapper around EventId that includes timestamp for sorting
///
/// This type is designed for use in collections like `ReverseSortedSet` where
/// events need to be ordered by timestamp while maintaining constant-time
/// deduplication based on EventId.
///
/// The actual event data is stored separately in a centralized HashMap,
/// making this approach memory-efficient when the same event appears in multiple tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SortableEventId {
    pub id: EventId,
    pub created_at: Timestamp,
}

impl SortableEventId {
    /// Create a new SortableEventId from an EventId and Timestamp
    pub fn new(id: EventId, created_at: Timestamp) -> Self {
        Self { id, created_at }
    }

    /// Create a SortableEventId from an Event
    pub fn from_event(event: &Event) -> Self {
        Self {
            id: event.id,
            created_at: event.created_at,
        }
    }
}

impl PartialOrd for SortableEventId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SortableEventId {
    fn cmp(&self, other: &Self) -> Ordering {
        // Sort by timestamp first (primary key), then by event ID (secondary key)
        match self.created_at.cmp(&other.created_at) {
            Ordering::Equal => self.id.cmp(&other.id),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sortable_event_id_creation() {
        let event_id = EventId::all_zeros();
        let timestamp = Timestamp::from(1000);

        let sortable = SortableEventId::new(event_id, timestamp);

        assert_eq!(sortable.id, event_id);
        assert_eq!(sortable.created_at, timestamp);
    }

    #[test]
    fn test_sortable_event_id_from_event() -> Result<()> {
        let keys = Keys::generate();
        let timestamp = Timestamp::from(1234567890);

        let event = EventBuilder::text_note("test")
            .custom_created_at(timestamp)
            .sign_with_keys(&keys)?;

        let sortable = SortableEventId::from_event(&event);

        assert_eq!(sortable.id, event.id);
        assert_eq!(sortable.created_at, timestamp);

        Ok(())
    }

    #[test]
    fn test_sortable_event_id_ordering_by_timestamp() {
        let event_id1 = EventId::all_zeros();
        let event_id2 = EventId::from_slice(&[1u8; 32]).expect("Valid event ID");

        let older = SortableEventId::new(event_id1, Timestamp::from(1000));
        let newer = SortableEventId::new(event_id2, Timestamp::from(2000));

        // Older timestamp should be less than newer timestamp
        assert!(older < newer);
        assert!(newer > older);
    }

    #[test]
    fn test_sortable_event_id_ordering_by_id_when_same_timestamp() {
        let timestamp = Timestamp::from(1000);
        let event_id1 = EventId::from_slice(&[0u8; 32]).expect("Valid event ID");
        let event_id2 = EventId::from_slice(&[1u8; 32]).expect("Valid event ID");

        let sortable1 = SortableEventId::new(event_id1, timestamp);
        let sortable2 = SortableEventId::new(event_id2, timestamp);

        // When timestamps are equal, compare by event ID
        assert!(sortable1 < sortable2);
        assert!(sortable2 > sortable1);
    }

    #[test]
    fn test_sortable_event_id_equality() {
        let event_id = EventId::all_zeros();
        let timestamp = Timestamp::from(1000);

        let sortable1 = SortableEventId::new(event_id, timestamp);
        let sortable2 = SortableEventId::new(event_id, timestamp);

        assert_eq!(sortable1, sortable2);
    }

    #[test]
    fn test_sortable_event_id_in_reverse_sorted_set() {
        use sorted_vec::ReverseSortedSet;
        use std::cmp::Reverse;

        let mut set: ReverseSortedSet<SortableEventId> = ReverseSortedSet::new();

        let id1 = SortableEventId::new(EventId::all_zeros(), Timestamp::from(1000));
        let id2 = SortableEventId::new(
            EventId::from_slice(&[1u8; 32]).expect("Valid"),
            Timestamp::from(2000),
        );
        let id3 = SortableEventId::new(
            EventId::from_slice(&[2u8; 32]).expect("Valid"),
            Timestamp::from(3000),
        );

        // Insert in arbitrary order
        let _ = set.find_or_insert(Reverse(id2));
        let _ = set.find_or_insert(Reverse(id1));
        let _ = set.find_or_insert(Reverse(id3));

        // Should be sorted in reverse order (newest first)
        let sorted: Vec<_> = set.iter().map(|r| r.0).collect();
        assert_eq!(sorted, vec![id3, id2, id1]);
    }
}
