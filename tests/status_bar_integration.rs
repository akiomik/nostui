use nostr_sdk::prelude::*;
use nostui::{
    core::msg::{system::SystemMsg, Msg},
    core::state::AppState,
    core::update::update,
    domain::nostr::Profile,
    presentation::components::status_bar::StatusBar,
};

/// Test StatusBar integration with Elm architecture
#[test]
fn test_status_bar_stateless() {
    let status1 = StatusBar::new();
    let status2 = StatusBar;

    // StatusBar should be completely stateless
    assert_eq!(format!("{:?}", status1), format!("{:?}", status2));
}

#[test]
fn test_status_bar_display_name_flow() {
    let keys = Keys::generate();
    let state = AppState::new(keys.public_key());
    let status_bar = StatusBar::new();

    // Initially should show shortened public key
    let initial_name = status_bar.get_display_name(&state);
    assert!(initial_name.contains(":"));

    // Add profile and update state
    let metadata = Metadata::new().name("Alice").display_name("Alice Smith");
    let profile =
        nostui::domain::nostr::Profile::new(keys.public_key(), Timestamp::now(), metadata);

    let (new_state, cmds) = update(Msg::UpdateProfile(keys.public_key(), profile), state);
    assert!(cmds.is_empty());

    // Now should show profile name
    let updated_name = status_bar.get_display_name(&new_state);
    assert_eq!(updated_name, "Alice Smith");
}

// test_status_message_flow removed - migrated to SystemState unit tests in src/core/state/system.rs

// test_loading_state_flow removed - migrated to SystemState unit tests in src/core/state/system.rs

#[test]
fn test_connection_status_helper() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    // Loading state
    state.system.is_loading = true;
    let status = StatusBar::get_connection_status(&state);
    assert_eq!(status, "Connecting...");

    // Active state with message
    state.system.is_loading = false;
    state.system.status_message = Some("Connected".to_string());
    let status = StatusBar::get_connection_status(&state);
    assert_eq!(status, "Active");

    // Ready state
    state.system.status_message = None;
    let status = StatusBar::get_connection_status(&state);
    assert_eq!(status, "Ready");
}

#[test]
fn test_profile_helpers() {
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());

    // Initially no profile
    assert!(!StatusBar::has_profile_data(&state));
    assert!(StatusBar::get_profile_timestamp(&state).is_none());

    // Add profile
    let metadata = Metadata::new().name("Test User");
    let timestamp = Timestamp::now();
    let profile = Profile::new(keys.public_key(), timestamp, metadata);
    state.user.profiles.insert(keys.public_key(), profile);

    // Now has profile data
    assert!(StatusBar::has_profile_data(&state));
    assert_eq!(StatusBar::get_profile_timestamp(&state), Some(timestamp));
}

#[test]
fn test_multiple_users_scenario() {
    let user1_keys = Keys::generate();
    let user2_keys = Keys::generate();
    let mut state = AppState::new(user1_keys.public_key());

    // Add profiles for both users
    let metadata1 = Metadata::new().name("User One");
    let profile1 = Profile::new(user1_keys.public_key(), Timestamp::now(), metadata1);

    let metadata2 = Metadata::new().name("User Two");
    let profile2 = Profile::new(user2_keys.public_key(), Timestamp::now(), metadata2);

    state
        .user
        .profiles
        .insert(user1_keys.public_key(), profile1);
    state
        .user
        .profiles
        .insert(user2_keys.public_key(), profile2);

    let status_bar = StatusBar::new();

    // Should show current user (User One), not User Two
    let display_name = status_bar.get_display_name(&state);
    assert_eq!(display_name, "@User One");

    // Verify we have multiple profiles
    assert_eq!(state.user.profiles.len(), 2);
}

#[tokio::test]
async fn test_status_bar_integration_full_flow() {
    let keys = Keys::generate();
    let initial_state = AppState::new(keys.public_key());
    let status_bar = StatusBar::new();

    // 1. Initial state - loading, no profile, no message
    assert!(initial_state.system.is_loading);
    assert!(!StatusBar::has_profile_data(&initial_state));
    assert!(initial_state.system.status_message.is_none());

    let initial_name = status_bar.get_display_name(&initial_state);
    assert!(initial_name.contains(":"));

    // 2. Stop loading and set initial message
    let (state, _) = update(Msg::System(SystemMsg::SetLoading(false)), initial_state);
    let (state, _) = update(
        Msg::System(SystemMsg::UpdateStatusMessage(
            "Connecting to relays...".to_string(),
        )),
        state,
    );

    assert!(!state.system.is_loading);
    assert_eq!(StatusBar::get_connection_status(&state), "Active");

    // 3. Receive profile metadata
    let metadata = Metadata::new()
        .name("Integration Test User")
        .display_name("ITest User")
        .about("Testing Elm architecture");
    let profile =
        nostui::domain::nostr::Profile::new(keys.public_key(), Timestamp::now(), metadata);
    let (state, _) = update(Msg::UpdateProfile(keys.public_key(), profile), state);

    assert!(StatusBar::has_profile_data(&state));
    let updated_name = status_bar.get_display_name(&state);
    assert_eq!(updated_name, "ITest User");

    // 4. Update status to connected
    let (state, _) = update(
        Msg::System(SystemMsg::UpdateStatusMessage(
            "Connected to 3 relays".to_string(),
        )),
        state,
    );

    // 5. Clear status message
    let (final_state, _) = update(Msg::System(SystemMsg::ClearStatusMessage), state);

    assert!(final_state.system.status_message.is_none());
    assert_eq!(StatusBar::get_connection_status(&final_state), "Ready");

    // Final verification: all state managed through Elm architecture
    assert!(!final_state.system.is_loading);
    assert!(StatusBar::has_profile_data(&final_state));
    assert!(final_state.system.status_message.is_none());

    let final_name = status_bar.get_display_name(&final_state);
    assert_eq!(final_name, "ITest User");
}

#[test]
fn test_status_bar_vs_legacy_approach() {
    let keys = Keys::generate();
    let state = AppState::new(keys.public_key());

    // Legacy approach: StatusBar manages its own profile, message, loading state
    // New approach: All state comes from AppState

    // StatusBar can render any state configuration
    let status_bar = StatusBar::new();

    // Test different state configurations
    let mut test_state = state;

    // Configuration 1: Loading state
    test_state.system.is_loading = true;
    test_state.system.status_message = None;
    let status = StatusBar::get_connection_status(&test_state);
    assert_eq!(status, "Connecting...");

    // Configuration 2: Active state
    test_state.system.is_loading = false;
    test_state.system.status_message = Some("Active connection".to_string());
    let status = StatusBar::get_connection_status(&test_state);
    assert_eq!(status, "Active");

    // Configuration 3: Ready state
    test_state.system.status_message = None;
    let status = StatusBar::get_connection_status(&test_state);
    assert_eq!(status, "Ready");

    // Same component instance handles all configurations
    let name = status_bar.get_display_name(&test_state);
    assert!(!name.is_empty());
}
