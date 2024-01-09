use nostr_sdk::prelude::*;

#[derive(PartialEq, Eq)]
pub struct SortableEvent {
    pub event: Event,
}

impl SortableEvent {
    pub fn new(event: Event) -> Self {
        Self { event }
    }
}

impl PartialOrd for SortableEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SortableEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.event.created_at == other.event.created_at {
            self.event.id.cmp(&other.event.id)
        } else {
            self.event.created_at.cmp(&other.event.created_at)
        }
    }
}
