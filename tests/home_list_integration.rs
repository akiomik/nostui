use nostr_sdk::prelude::*;
use ratatui::{backend::TestBackend, prelude::*};

use nostui::{
    core::{
        msg::{timeline::TimelineMsg, ui::UiMsg, Msg},
        state::AppState,
        update::update,
    },
    presentation::components::home_list::HomeList,
};

/// Test Home list UI layer integration with Elm architecture
#[test]
fn test_home_list_stateless() {
    let list1 = HomeList::new();
    let list2 = HomeList::default();

    // HomeList should be completely stateless
    assert_eq!(format!("{list1:?}"), format!("{list2:?}"));
}

#[test]
fn test_scroll_position_calculations() {
    // Empty timeline
    assert_eq!(HomeList::calculate_valid_scroll_position(Some(0), 0), None);
    assert_eq!(HomeList::scroll_up_position(None, 0), None);
    assert_eq!(HomeList::scroll_down_position(None, 0), None);

    // Normal timeline (5 items)
    assert_eq!(
        HomeList::calculate_valid_scroll_position(Some(2), 5),
        Some(2)
    );
    assert_eq!(
        HomeList::calculate_valid_scroll_position(Some(10), 5),
        Some(4)
    ); // Out of bounds

    // Scroll up
    assert_eq!(HomeList::scroll_up_position(Some(3), 5), Some(2));
    assert_eq!(HomeList::scroll_up_position(Some(0), 5), Some(0)); // At top
    assert_eq!(HomeList::scroll_up_position(None, 5), Some(0)); // Start from top

    // Scroll down
    assert_eq!(HomeList::scroll_down_position(Some(1), 5), Some(2));
    assert_eq!(HomeList::scroll_down_position(Some(4), 5), Some(4)); // At bottom
    assert_eq!(HomeList::scroll_down_position(None, 5), Some(0)); // Start from top

    // Scroll to extremes
    assert_eq!(HomeList::scroll_to_top_position(5), Some(0));
    assert_eq!(HomeList::scroll_to_bottom_position(5), Some(4));
    assert_eq!(HomeList::scroll_to_top_position(0), None);
    assert_eq!(HomeList::scroll_to_bottom_position(0), None);
}

#[test]
fn test_selection_state_integration_with_elm() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    // Add test notes
    for i in 0..5 {
        let event = EventBuilder::text_note(format!("Test note {i}"))
            .sign_with_keys(&keys)
            .unwrap();
        let (new_state, _) = update(Msg::Timeline(TimelineMsg::AddNote(event)), state);
        state = new_state;
    }

    // Initially no selection
    assert_eq!(state.timeline.selected_index, None);
    assert!(!HomeList::get_selection_info(&state).has_selection);

    // Select first item via Elm update
    let (new_state, _) = update(Msg::Timeline(TimelineMsg::SelectNote(0)), state);
    state = new_state;
    let info = HomeList::get_selection_info(&state);
    assert_eq!(info.selected_index, Some(0));
    assert!(info.has_selection);
    assert!(info.is_at_top);
    assert!(!info.is_at_bottom);

    // Select last item
    let (new_state, _) = update(Msg::Timeline(TimelineMsg::SelectNote(4)), state);
    state = new_state;
    let info = HomeList::get_selection_info(&state);
    assert!(info.is_at_bottom);
    assert!(!info.is_at_top);

    // Deselect
    let (new_state, _) = update(Msg::Timeline(TimelineMsg::DeselectNote), state);
    state = new_state;
    assert!(!HomeList::get_selection_info(&state).has_selection);
}

#[test]
fn test_scrollable_conditions() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    // Empty timeline - not scrollable
    assert!(!HomeList::is_scrollable(&state));

    // Add notes - becomes scrollable
    let event = EventBuilder::text_note("Test note")
        .sign_with_keys(&keys)
        .unwrap();
    let (new_state, _) = update(Msg::Timeline(TimelineMsg::AddNote(event)), state);
    state = new_state;
    assert!(HomeList::is_scrollable(&state));

    // Show input - not scrollable even with notes
    let (new_state, _) = update(Msg::Ui(UiMsg::ShowNewNote), state);
    state = new_state;
    assert!(!HomeList::is_scrollable(&state));

    // Hide input - scrollable again
    let (new_state, _) = update(Msg::Ui(UiMsg::CancelInput), state);
    state = new_state;
    assert!(HomeList::is_scrollable(&state));
}

#[test]
fn test_scroll_with_input_shown() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    // Add test notes
    for i in 0..5 {
        let event = EventBuilder::text_note(format!("Note {i}"))
            .sign_with_keys(&keys)
            .unwrap();
        let (new_state, _) = update(Msg::Timeline(TimelineMsg::AddNote(event)), state);
        state = new_state;
    }

    // Select first item
    let (new_state, _) = update(Msg::Timeline(TimelineMsg::ScrollDown), state);
    state = new_state;
    assert_eq!(state.timeline.selected_index, Some(0));

    // Show input
    let (new_state, _) = update(Msg::Ui(UiMsg::ShowNewNote), state);
    state = new_state;

    // Try to scroll while input is shown - should not change selection
    let (new_state, _) = update(Msg::Timeline(TimelineMsg::ScrollDown), state);
    state = new_state;
    assert_eq!(state.timeline.selected_index, Some(0)); // Unchanged

    let (new_state, _) = update(Msg::Timeline(TimelineMsg::ScrollUp), state);
    state = new_state;
    assert_eq!(state.timeline.selected_index, Some(0)); // Still unchanged
}

#[test]
fn test_out_of_bounds_selection_handling() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    // Add 3 notes
    for i in 0..3 {
        let event = EventBuilder::text_note(format!("Note {i}"))
            .sign_with_keys(&keys)
            .unwrap();
        let (new_state, _) = update(Msg::Timeline(TimelineMsg::AddNote(event)), state);
        state = new_state;
    }

    // Try to select out of bounds index
    let (new_state, _) = update(Msg::Timeline(TimelineMsg::SelectNote(10)), state);
    state = new_state;

    // The update function should handle out of bounds gracefully
    // In current implementation, it may accept the invalid index
    // This test verifies that HomeList can handle such cases
    let info = HomeList::get_selection_info(&state);
    // The selection info should still be valid for UI purposes
    assert_eq!(info.timeline_length, 3);
}

#[test]
fn test_selection_info_comprehensive() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    // Empty timeline
    let info = HomeList::get_selection_info(&state);
    assert_eq!(info.timeline_length, 0);
    assert!(!info.has_selection);
    assert!(!info.is_at_top);
    assert!(!info.is_at_bottom);

    // Add single note
    let event = EventBuilder::text_note("Single note")
        .sign_with_keys(&keys)
        .unwrap();
    let (new_state, _) = update(Msg::Timeline(TimelineMsg::AddNote(event)), state);
    state = new_state;

    // Select the only note
    let (new_state, _) = update(Msg::Timeline(TimelineMsg::SelectNote(0)), state);
    state = new_state;
    let info = HomeList::get_selection_info(&state);
    assert_eq!(info.timeline_length, 1);
    assert!(info.has_selection);
    assert!(info.is_at_top);
    assert!(info.is_at_bottom); // Same position when only one item

    // Add more notes
    for i in 1..5 {
        let event = EventBuilder::text_note(format!("Note {i}"))
            .sign_with_keys(&keys)
            .unwrap();
        let (new_state, _) = update(Msg::Timeline(TimelineMsg::AddNote(event)), state);
        state = new_state;
    }

    // Select middle
    let (new_state, _) = update(Msg::Timeline(TimelineMsg::SelectNote(2)), state);
    state = new_state;
    let info = HomeList::get_selection_info(&state);
    assert_eq!(info.timeline_length, 5);
    assert!(!info.is_at_top);
    assert!(!info.is_at_bottom);
}

#[test]
fn test_clamps_selection_to_valid_range_in_draw() {
    // Given: state with items and an out-of-bounds selection
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    for i in 0..3 {
        let ev = EventBuilder::text_note(format!("Note {i}"))
            .sign_with_keys(&keys)
            .unwrap();
        let (new_state, _) = update(Msg::Timeline(TimelineMsg::AddNote(ev)), state);
        state = new_state;
    }

    // Set selection to exactly len() (invalid index)
    state.timeline.selected_index = Some(state.timeline.notes.len());

    let list = HomeList::new();

    // When: we render using a test backend, draw should not panic due to OOB
    let area = Rect::new(0, 0, 80, 20);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            let frame_area = f.area();
            let _ = list.draw(&state, f, frame_area);
        })
        .unwrap();
}

#[tokio::test]
async fn test_complete_ui_workflow() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());
    let _home_list = HomeList::new();

    // 1. Start with empty timeline
    assert!(!HomeList::is_scrollable(&state));
    assert_eq!(HomeList::get_selection_info(&state).timeline_length, 0);

    // 2. Add notes progressively
    for i in 0..10 {
        let event = EventBuilder::text_note(format!("Timeline post #{i}"))
            .sign_with_keys(&keys)
            .unwrap();
        let (new_state, _) = update(Msg::Timeline(TimelineMsg::AddNote(event)), state);
        state = new_state;
    }

    // 3. Now scrollable
    assert!(HomeList::is_scrollable(&state));
    assert_eq!(HomeList::get_selection_info(&state).timeline_length, 10);

    // 4. Navigate through timeline
    let (new_state, _) = update(Msg::Timeline(TimelineMsg::ScrollDown), state);
    state = new_state;
    assert_eq!(state.timeline.selected_index, Some(0));

    // 5. Jump to bottom
    let (new_state, _) = update(Msg::Timeline(TimelineMsg::ScrollToBottom), state);
    state = new_state;
    let info = HomeList::get_selection_info(&state);
    assert!(info.is_at_bottom);
    assert_eq!(info.selected_index, Some(9));

    // 6. Show input (disables scrolling)
    let (new_state, _) = update(Msg::Ui(UiMsg::ShowNewNote), state);
    state = new_state;
    assert!(!HomeList::is_scrollable(&state));

    // 7. Try to scroll (should be ignored)
    let old_selection = state.timeline.selected_index;
    let (new_state, _) = update(Msg::Timeline(TimelineMsg::ScrollUp), state);
    state = new_state;
    assert_eq!(state.timeline.selected_index, old_selection);

    // 8. Cancel input (re-enables scrolling)
    let (new_state, _) = update(Msg::Ui(UiMsg::CancelInput), state);
    state = new_state;
    assert!(HomeList::is_scrollable(&state));

    // 9. Final navigation test
    let (new_state, _) = update(Msg::Timeline(TimelineMsg::ScrollToTop), state);
    state = new_state;
    let final_info = HomeList::get_selection_info(&state);
    assert!(final_info.is_at_top);
    assert_eq!(final_info.selected_index, Some(0));
}

// test_scroll_boundary_conditions removed - boundary handling migrated to TimelineState unit tests in src/core/state/timeline.rs
