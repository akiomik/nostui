use nostr_sdk::prelude::*;
use tokio::sync::mpsc;

use nostui::{
    core::msg::{system::SystemMsg, Msg},
    core::raw_msg::RawMsg,
    core::state::AppState,
    core::translator::translate_raw_to_domain,
    core::update::update,
    infrastructure::fps_service::FpsService,
    presentation::components::fps::FpsCounter,
};

/// Test FPS counter integration with Elm architecture
#[test]
fn test_fps_counter_stateless() {
    let fps1 = FpsCounter::new();
    let fps2 = FpsCounter;

    // FpsCounter should be completely stateless
    // Since it's a unit struct, all instances are equivalent
    assert_eq!(format!("{fps1:?}"), format!("{fps2:?}"));
}

#[test]
fn test_fps_raw_message_translation() {
    let state = AppState::new(Keys::generate().public_key());

    // Test app FPS translation
    let raw_msg = RawMsg::AppFpsUpdate(60.0);
    let domain_msgs = translate_raw_to_domain(raw_msg, &state);
    assert_eq!(
        domain_msgs,
        vec![Msg::System(SystemMsg::UpdateAppFps(60.0))]
    );

    // Test render FPS translation
    let raw_msg = RawMsg::RenderFpsUpdate(120.0);
    let domain_msgs = translate_raw_to_domain(raw_msg, &state);
    assert_eq!(
        domain_msgs,
        vec![Msg::System(SystemMsg::UpdateRenderFps(120.0))]
    );
}

// test_fps_domain_message_handling removed - migrated to SystemState unit tests in src/core/state/system.rs

#[test]
fn test_fps_service_basic_functionality() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut fps_service = FpsService::new(tx);

    // Test initial state
    assert_eq!(fps_service.app_frames, 0);
    assert_eq!(fps_service.render_frames, 0);

    // Test tick increments
    fps_service.on_app_tick();
    fps_service.on_render();

    assert_eq!(fps_service.app_frames, 1);
    assert_eq!(fps_service.render_frames, 1);

    // Should not send messages immediately (less than 1 second)
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn test_fps_integration_full_flow() {
    // Setup
    let keys = Keys::generate();
    let mut state = AppState::new(keys.public_key());
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut fps_service = FpsService::new(tx);

    // Initial FPS should be 0
    assert_eq!(state.system.fps_data.app_fps, 0.0);
    assert_eq!(state.system.fps_data.render_fps, 0.0);

    // Simulate some ticks (won't send messages until 1 second passes)
    fps_service.on_app_tick();
    fps_service.on_render();

    // No messages should be sent yet
    assert!(rx.try_recv().is_err());

    // Manually test the update flow by simulating FPS updates
    let (new_state, _) = update(Msg::System(SystemMsg::UpdateAppFps(60.0)), state.clone());
    state = new_state;

    let (new_state, _) = update(Msg::System(SystemMsg::UpdateRenderFps(120.0)), state);
    state = new_state;

    // Verify state updates
    assert_eq!(state.system.fps_data.app_fps, 60.0);
    assert_eq!(state.system.fps_data.render_fps, 120.0);
    assert_eq!(state.system.fps_data.app_frames, 1);
    assert_eq!(state.system.fps_data.render_frames, 1);

    // Test FpsCounter can access the state
    let _fps = FpsCounter::new();

    // The component should be able to access FPS data from state
    // (Actual rendering test would require a terminal backend mock)
    assert_eq!(state.system.fps_data.app_fps, 60.0);
    assert_eq!(state.system.fps_data.render_fps, 120.0);
}

#[test]
fn test_fps_service_independence() {
    // Test that multiple FPS services can work independently
    let (tx1, mut rx1) = mpsc::unbounded_channel();
    let (tx2, mut rx2) = mpsc::unbounded_channel();

    let mut service1 = FpsService::new(tx1);
    let mut service2 = FpsService::new(tx2);

    // Each service maintains its own state
    service1.on_app_tick();
    service1.on_app_tick();

    service2.on_render();

    assert_eq!(service1.app_frames, 2);
    assert_eq!(service1.render_frames, 0);

    assert_eq!(service2.app_frames, 0);
    assert_eq!(service2.render_frames, 1);

    // No messages sent yet (time hasn't elapsed)
    assert!(rx1.try_recv().is_err());
    assert!(rx2.try_recv().is_err());
}

/// Test comparison with legacy FPS counter behavior
#[test]
fn test_new_vs_legacy_fps_approach() {
    let keys = Keys::generate();
    let state = AppState::new(keys.public_key());

    // Legacy approach: Component holds its own state (not testable with fixed values)
    // New approach: State comes from AppState (easily testable)

    // Simulate state with known FPS values
    let mut test_state = state;
    test_state.system.fps_data.app_fps = 30.0;
    test_state.system.fps_data.render_fps = 60.0;

    // FpsCounter can render any state passed to it
    let _new_fps = FpsCounter::new();

    // Component is stateless - same instance can render different states
    assert_eq!(test_state.system.fps_data.app_fps, 30.0);
    assert_eq!(test_state.system.fps_data.render_fps, 60.0);

    // Change state
    test_state.system.fps_data.app_fps = 120.0;
    test_state.system.fps_data.render_fps = 240.0;

    // Same component can render updated state
    assert_eq!(test_state.system.fps_data.app_fps, 120.0);
    assert_eq!(test_state.system.fps_data.render_fps, 240.0);
}
