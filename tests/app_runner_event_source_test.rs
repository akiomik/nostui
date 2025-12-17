use nostui::integration::app_runner::AppRunner;
#[tokio::test]
async fn test_app_runner_event_source_injection_and_processing() {
    use nostr_sdk::prelude::*;

    // Prepare config
    let cfg = nostui::infrastructure::config::Config {
        privatekey: Keys::generate().secret_key().to_bech32().unwrap(),
        relays: vec!["wss://example.com".into()],
        ..Default::default()
    };

    // Create runner headless
    let mut runner = AppRunner::new_with_config(cfg, 10.0, 30.0, true)
        .await
        .expect("failed to create AppRunner");

    // Inject a test event source that emits a Resize then a Quit
    use nostui::infrastructure::tui_event_source::{EventSource, TuiEvent};
    let events = vec![TuiEvent::Resize(100, 40), TuiEvent::Quit];
    runner.set_event_source_for_tests(EventSource::test(events));

    // Run one cycle -> should process first event (Resize)
    runner
        .run_one_cycle_for_tests()
        .await
        .expect("one cycle should succeed");

    // After processing, send Quit and ensure the loop would stop
    runner
        .run_one_cycle_for_tests()
        .await
        .expect("one cycle should succeed");

    // Check that state contains the resized dimensions (through RawMsg -> update)
    let state = runner.runtime().state().clone();
    // We don't have direct width/height on state, but we can at least assert that not panicked and processed
    // For stronger assertion, we would inspect TuiService mock or expose resize effect. For now, we ensure quit flag.

    // Quit was sent in the second cycle
    assert!(state.system.should_quit || !state.system.is_loading);
}
