use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;
use nostui::infrastructure::config::Config;
use nostui::infrastructure::test_tui::TestTui;
use nostui::integration::app_runner::AppRunner;

#[tokio::test]
async fn test_render_coalesce_with_test_terminal() -> Result<()> {
    // Prepare config
    let cfg = Config {
        privatekey: Keys::generate().secret_key().to_bech32()?,
        relays: vec!["wss://example.com".into()],
        ..Default::default()
    };

    // Create runner (headless)
    let mut runner = AppRunner::new_with_config(cfg, 10.0, 30.0, true).await?;

    // Inject TestTui
    use std::sync::Arc;
    use tokio::sync::Mutex;
    let test_tui = Arc::new(Mutex::new(TestTui::new(80, 24)?));
    runner.set_tui_for_tests(test_tui.clone());

    // Emulate render request channel: send multiple requests in one logical loop
    // Using run_one_cycle_for_tests doesn't drive coalesce in main loop, so directly call render
    // to simulate the coalesced execution.
    // In a full integration, we would pump the render_req channel; here we assert draw count.
    // Call render twice to emulate two loops with coalesce
    runner.render_for_tests().await?;
    runner.render_for_tests().await?;

    // With TestTui, draw_count reflects the number of frames drawn.
    // We expect exactly 2 frames (once per "loop").
    let count = test_tui.lock().await.draw_count();
    assert_eq!(count, 2);

    Ok(())
}
