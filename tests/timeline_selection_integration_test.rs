// Integration test for timeline selection functionality
// Tests the interaction between timeline state management and UI selection

use nostr_sdk::prelude::*;
use nostui::{
    core::{msg::Msg, state::AppState, update::update},
    domain::nostr::SortableEvent,
};
use std::cmp::Reverse;

fn create_state_with_hex_usernames(count: usize) -> AppState {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    for i in 0..count {
        let author_keys = Keys::generate();
        let event = EventBuilder::text_note(format!("Note {} with hex username", i))
            .sign_with_keys(&author_keys)
            .unwrap();

        let sortable = SortableEvent::new(event);
        state.timeline.notes.find_or_insert(Reverse(sortable));
    }

    // Explicitly don't add any profiles - all users will show as hex
    state
}

#[test]
fn test_timeline_selection_state_management_with_hex_users() {
    // Test that timeline selection works correctly for hex usernames
    let state = create_state_with_hex_usernames(3);

    // No initial selection
    assert_eq!(state.timeline.selected_index, None);

    // Select first note
    let (state1, _) = update(Msg::ScrollDown, state);
    assert_eq!(state1.timeline.selected_index, Some(0));

    // Select second note
    let (state2, _) = update(Msg::ScrollDown, state1);
    assert_eq!(state2.timeline.selected_index, Some(1));

    // Move back up
    let (state3, _) = update(Msg::ScrollUp, state2);
    assert_eq!(state3.timeline.selected_index, Some(0));

    // Go to top
    let (state4, _) = update(Msg::ScrollToTop, state3);
    assert_eq!(state4.timeline.selected_index, Some(0));

    // Go to bottom
    let (state5, _) = update(Msg::ScrollToBottom, state4);
    assert_eq!(state5.timeline.selected_index, Some(2)); // 3 notes, last index is 2
}
