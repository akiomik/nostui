/// ElmHomeAdapter の描画ロジックをテストするための簡易テスト
#[cfg(test)]
mod elm_home_adapter_tests {
    use nostr_sdk::Keys;
    use nostui::{
        core::state::AppState, integration::elm_home_adapter::ElmHomeAdapter,
        integration::elm_integration::ElmRuntime, integration::legacy::action::Action,
        integration::legacy::Component,
    };
    use tokio::sync::mpsc;

    #[test]
    fn test_elm_home_adapter_with_runtime() {
        // ElmHomeAdapterにElmRuntimeを設定して状態確認
        let mut adapter = ElmHomeAdapter::new();

        // ActionチャンネルとElmRuntime設定
        let (action_tx, _action_rx) = mpsc::unbounded_channel();
        let (nostr_tx, _nostr_rx) = mpsc::unbounded_channel();

        let keys = Keys::generate();
        let state = AppState::new(keys.public_key());
        let runtime = ElmRuntime::new_with_nostr_executor(state, action_tx, nostr_tx);

        // ElmRuntimeを設定
        adapter.set_runtime(runtime);

        // 状態確認
        let current_state = adapter.get_current_state();
        assert!(
            current_state.is_some(),
            "ElmRuntime should be set and state should be available"
        );

        println!("✅ ElmHomeAdapter state test passed!");
        println!(
            "   Timeline length: {}",
            current_state.unwrap().timeline_len()
        );
        println!("   Input shown: {}", current_state.unwrap().ui.show_input);
    }

    #[test]
    fn test_elm_home_adapter_update_action() {
        let mut adapter = ElmHomeAdapter::new();
        let (action_tx, _action_rx) = mpsc::unbounded_channel();
        let (nostr_tx, _nostr_rx) = mpsc::unbounded_channel();

        let keys = Keys::generate();
        let state = AppState::new(keys.public_key());
        let runtime = ElmRuntime::new_with_nostr_executor(state, action_tx, nostr_tx);
        adapter.set_runtime(runtime);

        // Tick actionを送信
        let result = adapter.update(Action::Tick);
        assert!(result.is_ok(), "ElmHomeAdapter should handle Tick action");

        println!("✅ ElmHomeAdapter action handling test passed!");
    }
}
