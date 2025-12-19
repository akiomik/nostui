use std::sync::Arc;
use std::time::Duration;

use nostr_sdk::prelude::*;
use tokio::sync::Mutex;
use tokio::time::timeout;

use nostui::core::raw_msg::RawMsg;
use nostui::infrastructure::config::Config;
use nostui::infrastructure::tui::event_source::EventSource;
use nostui::infrastructure::tui::test::TestTui;
use nostui::integration::app_runner::AppRunner;

// Note: This test avoids opening a real interactive TUI by injecting a TestTui when needed.
// Network side effects are contained within NostrService which we won't drive far.

#[tokio::test]
async fn test_app_runner_headless_initialization() {
    // Create runner in headless mode so it won't enter crossterm raw mode
    let cfg = Config {
        privatekey: Keys::generate().secret_key().to_bech32().unwrap(),
        relays: vec!["wss://example.com".into()],
        ..Default::default()
    };

    let tui = Arc::new(Mutex::new(
        TestTui::new(80, 24).expect("failed to create TestTui"),
    ));
    let runner = AppRunner::new_with_config(
        cfg,
        Arc::<Mutex<TestTui>>::clone(&tui),
        EventSource::real(tui),
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
    let cfg = Config {
        privatekey: Keys::generate().secret_key().to_bech32().unwrap(),
        relays: vec!["wss://example.com".into()],
        ..Default::default()
    };

    let tui = Arc::new(Mutex::new(
        TestTui::new(80, 24).expect("failed to create TestTui"),
    ));
    let mut runner = AppRunner::new_with_config(
        cfg,
        Arc::<Mutex<TestTui>>::clone(&tui),
        EventSource::real(tui),
    )
    .await
    .expect("failed to create AppRunner");

    // Send a Quit to runtime before running, so the loop exits immediately
    runner.runtime_mut().send_raw_msg(RawMsg::Quit);

    // Run should exit quickly
    let res = timeout(Duration::from_millis(50), runner.run()).await;
    assert!(
        res.is_ok(),
        "runner.run() should complete promptly in headless quit scenario"
    );
}
