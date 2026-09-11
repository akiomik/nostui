use nostr_sdk::prelude::*;
use std::collections::HashSet;
use std::fmt;
use std::ops::{Deref, Index};
use std::slice::Iter;
use std::vec::IntoIter;

/// A set of events with automatic deduplication
/// Provides O(1) duplicate checking based on EventId while preserving insertion order
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventSet {
    events: Vec<Event>,
    event_ids: HashSet<EventId>,
}

impl EventSet {
    /// Creates a new empty set
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            event_ids: HashSet::new(),
        }
    }

    /// Creates a new set with the specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            events: Vec::with_capacity(capacity),
            event_ids: HashSet::with_capacity(capacity),
        }
    }

    /// Inserts an event into the set (ignores duplicates)
    /// Returns: true if the event was actually inserted, false if it was a duplicate
    pub fn insert(&mut self, event: Event) -> bool {
        if self.event_ids.insert(event.id) {
            self.events.push(event);
            true
        } else {
            false
        }
    }

    /// Alias for insert() providing Vec-like API
    pub fn push(&mut self, event: Event) -> bool {
        self.insert(event)
    }

    /// Checks if an EventId is contained in the set
    pub fn contains(&self, event_id: &EventId) -> bool {
        self.event_ids.contains(event_id)
    }

    /// Gets an event by index
    pub fn get(&self, index: usize) -> Option<&Event> {
        self.events.get(index)
    }

    /// Gets the first event
    pub fn first(&self) -> Option<&Event> {
        self.events.first()
    }

    /// Gets the last event
    pub fn last(&self) -> Option<&Event> {
        self.events.last()
    }

    /// Returns a reference to the internal Vec (read-only)
    pub fn as_slice(&self) -> &[Event] {
        &self.events
    }

    /// Gets the capacity
    pub fn capacity(&self) -> usize {
        self.events.capacity()
    }

    /// Reserves capacity
    pub fn reserve(&mut self, additional: usize) {
        self.events.reserve(additional);
        self.event_ids.reserve(additional);
    }

    /// Shrinks the capacity to fit
    pub fn shrink_to_fit(&mut self) {
        self.events.shrink_to_fit();
        self.event_ids.shrink_to_fit();
    }

    /// Retains events matching a predicate
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&Event) -> bool,
    {
        let mut i = 0;
        while i < self.events.len() {
            if f(&self.events[i]) {
                i += 1;
            } else {
                let removed = self.events.remove(i);
                self.event_ids.remove(&removed.id);
            }
        }
        debug_assert_eq!(self.events.len(), self.event_ids.len());
    }

    /// Clears all events
    pub fn clear(&mut self) {
        self.events.clear();
        self.event_ids.clear();
    }
}

// === Standard library trait implementations ===

impl Default for EventSet {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for EventSet {
    type Target = [Event];

    fn deref(&self) -> &Self::Target {
        &self.events
    }
}

impl Index<usize> for EventSet {
    type Output = Event;

    fn index(&self, index: usize) -> &Self::Output {
        &self.events[index]
    }
}

impl AsRef<[Event]> for EventSet {
    fn as_ref(&self) -> &[Event] {
        &self.events
    }
}

impl IntoIterator for EventSet {
    type Item = Event;
    type IntoIter = IntoIter<Event>;

    fn into_iter(self) -> Self::IntoIter {
        self.events.into_iter()
    }
}

impl<'a> IntoIterator for &'a EventSet {
    type Item = &'a Event;
    type IntoIter = Iter<'a, Event>;

    fn into_iter(self) -> Self::IntoIter {
        self.events.iter()
    }
}

impl FromIterator<Event> for EventSet {
    fn from_iter<T: IntoIterator<Item = Event>>(iter: T) -> Self {
        let mut events = Self::new();
        for event in iter {
            events.insert(event);
        }
        events
    }
}

impl Extend<Event> for EventSet {
    fn extend<T: IntoIterator<Item = Event>>(&mut self, iter: T) {
        for event in iter {
            self.insert(event);
        }
    }
}

impl fmt::Display for EventSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EventSet[{} events]", self.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use color_eyre::eyre::Result;
    use nostr_sdk::prelude::Signature;
    use nostr_sdk::prelude::{Kind, Timestamp};

    /// Only the last byte varies, so every suffix gives a distinct id.
    fn id_of(id_suffix: u8) -> EventId {
        let mut id_bytes = [0u8; 32];
        id_bytes[31] = id_suffix;
        EventId::from_byte_array(id_bytes)
    }

    fn create_test_event(id_suffix: u8, content: &str) -> Result<Event> {
        let keys = Keys::generate();
        Ok(Event::new(
            id_of(id_suffix),
            keys.public_key(),
            Timestamp::now(),
            Kind::TextNote,
            vec![],
            content.to_string(),
            Signature::from_slice(&[0u8; 64])?,
        ))
    }

    #[test]
    fn new_collection_is_empty() {
        let events = EventSet::new();
        assert!(events.is_empty());
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn insert_new_event_returns_true() -> Result<()> {
        let mut events = EventSet::new();
        let event = create_test_event(1, "test content")?;

        let was_added = events.insert(event.clone());

        assert!(was_added);
        assert_eq!(events.len(), 1);
        assert!(events.contains(&event.id));

        Ok(())
    }

    #[test]
    fn insert_duplicate_event_returns_false() -> Result<()> {
        let mut events = EventSet::new();
        let event = create_test_event(1, "test content")?;

        let first_add = events.insert(event.clone());
        assert!(first_add);
        assert_eq!(events.len(), 1);

        let second_add = events.insert(event);
        assert!(!second_add);
        assert_eq!(events.len(), 1);

        Ok(())
    }

    #[test]
    fn insert_different_events_both_added() -> Result<()> {
        let mut events = EventSet::new();
        let event1 = create_test_event(1, "first event")?;
        let event2 = create_test_event(2, "second event")?;

        assert!(events.insert(event1.clone()));
        assert!(events.insert(event2.clone()));

        assert_eq!(events.len(), 2);
        assert!(events.contains(&event1.id));
        assert!(events.contains(&event2.id));

        Ok(())
    }

    #[test]
    fn push_is_alias_for_insert() -> Result<()> {
        let mut events = EventSet::new();
        let event = create_test_event(1, "test content")?;

        assert!(events.push(event.clone()));
        assert!(!events.push(event));
        assert_eq!(events.len(), 1);

        Ok(())
    }

    #[test]
    fn duplicate_event_with_different_content() -> Result<()> {
        let mut events = EventSet::new();

        let id = EventId::from_byte_array([1u8; 32]);
        let keys = Keys::generate();

        let event1 = Event::new(
            id,
            keys.public_key(),
            Timestamp::now(),
            Kind::TextNote,
            vec![],
            "first content".to_string(),
            Signature::from_slice(&[0u8; 64])?,
        );

        let event2 = Event::new(
            id,
            keys.public_key(),
            Timestamp::now(),
            Kind::TextNote,
            vec![],
            "second content".to_string(),
            Signature::from_slice(&[0u8; 64])?,
        );

        assert!(events.insert(event1));
        // Rejected on the id alone; the differing content does not make it a new event.
        assert!(!events.insert(event2));
        assert_eq!(events.len(), 1);

        Ok(())
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn iteration_yields_every_inserted_event_in_insertion_order() -> Result<()> {
        let mut events_set = EventSet::new();
        // Suffixes out of order on purpose: with ascending ids, insertion order and id
        // order coincide, so the assertions below could not tell the two apart and a
        // reimplementation over any id-ordered container would pass.
        let test_events = [
            create_test_event(3, "first")?,
            create_test_event(1, "second")?,
            create_test_event(2, "third")?,
        ];

        for event in test_events.iter() {
            events_set.insert(event.clone());
        }

        // Unsorted, because the type's doc promises insertion order and nothing else in
        // the crate pins it. No caller observes the order today — `EventSet` holds a
        // note's reactions, reposts and zap receipts, which are read by two `len()`s and
        // an order-independent fold — so this guards the documented contract rather than
        // any behaviour a user could see. Sorting both sides would guard neither.
        let expected: Vec<_> = test_events.iter().map(|e| e.id).collect();

        // len() reaches the slice through Deref, EventSet defining none of its own;
        // first() on the line below is EventSet's own method, not the slice's.
        assert_eq!(events_set.len(), 3);
        assert_eq!(events_set.first().unwrap().content, "first");

        // iter() the same way, rather than the IntoIterator impl on &EventSet.
        let collected: Vec<_> = events_set.iter().map(|e| e.id).collect();
        assert_eq!(collected, expected);

        let ids: Vec<_> = events_set.into_iter().map(|e| e.id).collect();
        assert_eq!(ids, expected);

        Ok(())
    }

    #[test]
    fn clear_empties_the_collection() -> Result<()> {
        let mut events = EventSet::new();
        let event = create_test_event(1, "test")?;

        events.insert(event.clone());
        assert_eq!(events.len(), 1);

        events.clear();
        assert_eq!(events.len(), 0);
        assert!(events.is_empty());
        assert!(!events.contains(&event.id));

        Ok(())
    }

    #[test]
    fn trait_impls_reflect_the_set_contents() -> Result<()> {
        let mut events = EventSet::new();
        let event1 = create_test_event(1, "first")?;
        let event2 = create_test_event(2, "second")?;

        // FromIterator
        let events_from_iter: EventSet = vec![event1.clone(), event2.clone()].into_iter().collect();
        assert_eq!(events_from_iter.len(), 2);

        // Extend
        events.extend(vec![event1.clone(), event2]);
        assert_eq!(events.len(), 2);

        // Index
        assert_eq!(events[0].id, event1.id);

        // AsRef<[Event]>
        let slice: &[Event] = events.as_ref();
        assert_eq!(slice.len(), 2);

        // Display
        let display = format!("{events}");
        assert!(display.contains("2 events"));

        Ok(())
    }

    /// Ten ids, then eleven inserts of which six repeat, so the set has seen both a
    /// duplicate and a new id by the time it is read.
    fn overlapping_inserts() -> Result<EventSet> {
        let mut events = EventSet::new();

        for i in 1..=10 {
            events.insert(create_test_event(i, &format!("event {i}"))?);
        }

        // 5-15, so 5-10 repeat what is already there and 11-15 are new.
        for i in 5..=15 {
            events.insert(create_test_event(i, &format!("duplicate attempt {i}"))?);
        }

        Ok(events)
    }

    #[test]
    fn duplicate_inserts_leave_one_event_per_id() -> Result<()> {
        let events = overlapping_inserts()?;

        // The ids themselves rather than a count: 1-10 from the first loop and 11-15
        // from the second, each exactly once. A count alone would also hold for a set
        // that stored two events under one id and dropped another id entirely.
        let ids: Vec<EventId> = events.iter().map(|event| event.id).collect();
        let expected: Vec<EventId> = (1..=15).map(id_of).collect();
        assert_eq!(ids, expected);

        Ok(())
    }

    #[test]
    fn a_duplicate_insert_leaves_the_event_already_stored() -> Result<()> {
        let events = overlapping_inserts()?;

        // Only the ids offered twice carry this claim, so only those are read. `insert`
        // ignores the second offer, so the contents are the first loop's; an `insert`
        // that replaced on a duplicate id would leave "duplicate attempt 5" here and
        // still hold fifteen events under the same fifteen ids.
        let contents: Vec<&str> = (5..=10)
            .map(|i| {
                let id = id_of(i);
                events
                    .iter()
                    .find(|event| event.id == id)
                    .map(|event| event.content.as_str())
                    .expect("offered in the first loop")
            })
            .collect();
        assert_eq!(
            contents,
            vec!["event 5", "event 6", "event 7", "event 8", "event 9", "event 10"]
        );

        Ok(())
    }

    #[test]
    fn the_id_index_and_the_events_stay_in_step() -> Result<()> {
        let events = overlapping_inserts()?;

        // Fifteen written out rather than read back off `events`: a length taken from
        // the same Vec being iterated compares `[]` against `[]` on a set that stored
        // nothing, and an `insert` that stored nothing would pass.
        let indexed: Vec<bool> = events
            .iter()
            .map(|event| events.event_ids.contains(&event.id))
            .collect();
        assert_eq!(indexed, vec![true; 15]);

        // And no id in the index without an event of its own.
        assert_eq!(events.event_ids.len(), 15);

        Ok(())
    }

    #[test]
    fn with_capacity_reserves_the_capacity_asked_for() {
        let events = EventSet::with_capacity(256);

        // Both halves: `capacity()` reads the events, and an `event_ids` left
        // unreserved would make the set grow its index on the first inserts anyway.
        // `at least`, because that is all `Vec` and `HashSet` promise.
        assert!(events.capacity() >= 256);
        assert!(events.event_ids.capacity() >= 256);
    }

    #[test]
    fn a_thousand_inserts_drawn_from_256_ids_leave_256_events() -> Result<()> {
        let mut events = EventSet::new();

        for i in 0..1000 {
            events.insert(create_test_event((i % 256) as u8, &format!("event {i}"))?);
        }

        assert_eq!(events.len(), 256);

        Ok(())
    }

    #[test]
    fn contains_finds_an_id_even_on_an_event_it_never_saw() -> Result<()> {
        let mut events = EventSet::new();
        events.insert(create_test_event(1, "the content that was inserted")?);

        // A freshly built event carrying the same id: different content, different
        // author, never offered to the set. The id is what is looked up.
        let same_id = create_test_event(1, "nothing like it")?;
        assert!(events.contains(&same_id.id));

        Ok(())
    }

    #[test]
    fn contains_is_false_for_an_id_never_inserted() -> Result<()> {
        let mut events = EventSet::new();
        events.insert(create_test_event(1, "the content that was inserted")?);

        let never_inserted = create_test_event(2, "the content that was inserted")?;
        assert!(!events.contains(&never_inserted.id));

        Ok(())
    }

    #[test]
    fn retain_keeps_only_the_events_its_predicate_accepts() -> Result<()> {
        let mut events = EventSet::new();
        for i in 1..=6 {
            let label = if i % 2 == 0 { "drop" } else { "keep" };
            events.insert(create_test_event(i, &format!("{label} {i}"))?);
        }

        events.retain(|event| event.content.starts_with("keep"));

        // The exact survivors, not merely fewer than before: a `retain` that dropped
        // everything, or kept the wrong half, would pass a count-only assertion.
        let contents: Vec<&str> = events.iter().map(|event| event.content.as_str()).collect();
        assert_eq!(contents, vec!["keep 1", "keep 3", "keep 5"]);

        Ok(())
    }
}
