use nostr_sdk::ToBech32;
use nostui::core::raw_msg::RawMsg;
use nostui::integration::app_runner::AppRunner;

// Note: This test avoids opening a real interactive TUI by injecting a TestTui when needed.
// Network side effects are contained within NostrService which we won't drive far.

#[tokio::test]
async fn test_app_runner_headless_initialization() {
    use nostr_sdk::prelude::*;

    // Create runner in headless mode so it won't enter crossterm raw mode
    let cfg = nostui::infrastructure::config::Config {
        privatekey: Keys::generate().secret_key().to_bech32().unwrap(),
        relays: vec!["wss://example.com".into()],
        ..Default::default()
    };

    use std::sync::Arc;
    use tokio::sync::Mutex;
    let tui = Arc::new(Mutex::new(
        nostui::infrastructure::tui::test::TestTui::new(80, 24).expect("failed to create TestTui"),
    ));
    let runner = AppRunner::new_with_config(
        cfg,
        tui.clone(),
        nostui::infrastructure::tui::event_source::EventSource::real(tui),
    )
    .await
    .expect("failed to create AppRunner");

    // Basic sanity checks on internal runtime state
    let state = runner.runtime().state().clone();
    assert!(state.system.is_loading); // initial state starts loading
}

#[tokio::test]
async fn test_app_runner_headless_one_loop_quit() {
    // Headless runner
    let cfg = nostui::infrastructure::config::Config {
        privatekey: nostr_sdk::prelude::Keys::generate()
            .secret_key()
            .to_bech32()
            .unwrap(),
        relays: vec!["wss://example.com".into()],
        ..Default::default()
    };

    use std::sync::Arc;
    use tokio::sync::Mutex;
    let tui = Arc::new(Mutex::new(
        nostui::infrastructure::tui::test::TestTui::new(80, 24).expect("failed to create TestTui"),
    ));
    let mut runner = AppRunner::new_with_config(
        cfg,
        tui.clone(),
        nostui::infrastructure::tui::event_source::EventSource::real(tui),
    )
    .await
    .expect("failed to create AppRunner");

    // Send a Quit to runtime before running, so the loop exits immediately
    runner.runtime_mut().send_raw_msg(RawMsg::Quit);

    // Run should exit quickly
    let res = tokio::time::timeout(std::time::Duration::from_millis(50), runner.run()).await;
    assert!(
        res.is_ok(),
        "runner.run() should complete promptly in headless quit scenario"
    );
}
