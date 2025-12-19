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
    use nostr_sdk::nostr::{Kind, Timestamp};
    use nostr_sdk::prelude::Signature;

    fn create_test_event(id_suffix: u8, content: &str) -> Event {
        let mut id_bytes = [0u8; 32];
        id_bytes[31] = id_suffix; // 最後のバイトを変えて異なるIDを作成

        let keys = Keys::generate();
        Event::new(
            EventId::from_byte_array(id_bytes),
            keys.public_key(),
            Timestamp::now(),
            Kind::TextNote,
            vec![],
            content.to_string(),
            Signature::from_slice(&[0u8; 64]).unwrap(),
        )
    }

    #[test]
    fn test_new_collection_is_empty() {
        let events = EventSet::new();
        assert!(events.is_empty());
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_insert_new_event_returns_true() {
        let mut events = EventSet::new();
        let event = create_test_event(1, "test content");

        let was_added = events.insert(event.clone());

        assert!(was_added);
        assert_eq!(events.len(), 1);
        assert!(events.contains(&event.id));
    }

    #[test]
    fn test_insert_duplicate_event_returns_false() {
        let mut events = EventSet::new();
        let event = create_test_event(1, "test content");

        // 最初の挿入
        let first_add = events.insert(event.clone());
        assert!(first_add);
        assert_eq!(events.len(), 1);

        // 重複挿入
        let second_add = events.insert(event);
        assert!(!second_add);
        assert_eq!(events.len(), 1); // サイズは変わらない
    }

    #[test]
    fn test_insert_different_events_both_added() {
        let mut events = EventSet::new();
        let event1 = create_test_event(1, "first event");
        let event2 = create_test_event(2, "second event");

        assert!(events.insert(event1.clone()));
        assert!(events.insert(event2.clone()));

        assert_eq!(events.len(), 2);
        assert!(events.contains(&event1.id));
        assert!(events.contains(&event2.id));
    }

    #[test]
    fn test_push_is_alias_for_insert() {
        let mut events = EventSet::new();
        let event = create_test_event(1, "test content");

        assert!(events.push(event.clone()));
        assert!(!events.push(event)); // 重複
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_duplicate_event_with_different_content() {
        let mut events = EventSet::new();

        // 同じEventIdで異なるコンテンツのイベントを作成
        let id = EventId::from_byte_array([1u8; 32]);
        let keys = Keys::generate();

        let event1 = Event::new(
            id,
            keys.public_key(),
            Timestamp::now(),
            Kind::TextNote,
            vec![],
            "first content".to_string(),
            Signature::from_slice(&[0u8; 64]).unwrap(),
        );

        let event2 = Event::new(
            id, // 同じID
            keys.public_key(),
            Timestamp::now(),
            Kind::TextNote,
            vec![],
            "second content".to_string(), // 異なるコンテンツ
            Signature::from_slice(&[0u8; 64]).unwrap(),
        );

        assert!(events.insert(event1));
        assert!(!events.insert(event2)); // IDが同じなので拒否される
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_iteration() {
        let mut events_set = EventSet::new();
        let test_events = [
            create_test_event(1, "first"),
            create_test_event(2, "second"),
            create_test_event(3, "third"),
        ];

        for event in test_events.iter() {
            events_set.insert(event.clone());
        }

        // Deref経由でスライスメソッドを使用
        assert_eq!(events_set.len(), 3);
        assert_eq!(events_set.first().unwrap().content, "first");

        // iter()でのイテレーション（Deref経由）
        let collected: Vec<_> = events_set.iter().collect();
        assert_eq!(collected.len(), 3);

        // into_iter()でのイテレーション
        let ids: Vec<_> = events_set.into_iter().map(|e| e.id).collect();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn test_clear() {
        let mut events = EventSet::new();
        let event = create_test_event(1, "test");

        events.insert(event.clone());
        assert_eq!(events.len(), 1);

        events.clear();
        assert_eq!(events.len(), 0);
        assert!(events.is_empty());
        assert!(!events.contains(&event.id));
    }

    #[test]
    fn test_standard_traits() {
        let mut events = EventSet::new();
        let event1 = create_test_event(1, "first");
        let event2 = create_test_event(2, "second");

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
    }

    #[test]
    fn test_internal_consistency() {
        let mut events = EventSet::new();

        // 複数のイベントを追加 (1-10)
        for i in 1..=10 {
            events.insert(create_test_event(i, &format!("event {i}")));
        }

        // いくつか重複を試行 (5-15で5-10は重複)
        for i in 5..=15 {
            events.insert(create_test_event(i, &format!("duplicate attempt {i}")));
        }

        // 内部の一貫性をチェック
        assert_eq!(events.events.len(), events.event_ids.len());
        // 1-10の最初の追加 + 11-15の新規追加 = 15個のユニークなイベント
        assert_eq!(events.len(), 15);

        // 全てのイベントIDがHashSetに存在することを確認
        for event in events.iter() {
            assert!(events.event_ids.contains(&event.id));
        }
    }

    #[test]
    fn test_performance_and_capacity() {
        let mut events = EventSet::with_capacity(256);
        assert_eq!(events.capacity(), 256);

        // 大量のイベントを追加（パフォーマンステスト）
        for i in 0..1000 {
            let event = create_test_event((i % 256) as u8, &format!("event {i}"));
            events.insert(event);
        }

        // 256個のユニークなイベントのみが保存されるはず
        assert_eq!(events.len(), 256);

        // contains()の動作確認
        let test_event = create_test_event(100, "test");
        assert!(events.contains(&test_event.id));

        // retain機能のテスト
        events.retain(|e| e.content.starts_with("event 1"));
        assert!(events.len() < 256); // いくつかのイベントが削除される
    }
}
