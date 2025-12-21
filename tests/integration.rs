use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nostr_sdk::prelude::*;
use tokio::time::{sleep, timeout};

use nostui::{
    core::{
        cmd::NostrCmd,
        msg::{nostr::NostrMsg, system::SystemMsg, timeline::TimelineMsg, ui::UiMsg, Msg},
        state::AppState,
        update::update,
    },
    integration::runtime::Runtime,
    Cmd, RawMsg, VERSION,
};

/// Basic library flow test
#[test]
fn test_library_basic_flow() {
    let keys = Keys::generate();
    let initial_state = AppState::new(keys.public_key());

    // Test basic message processing
    let (state, cmds) = update(Msg::Ui(UiMsg::ShowNewNote), initial_state);
    assert!(state.ui.is_composing());
    assert!(cmds.is_empty());

    // Test input handling
    let (state, cmds) = update(
        Msg::Ui(UiMsg::UpdateInputContent("Hello".to_string())),
        state,
    );
    assert_eq!(state.ui.textarea.content, "Hello");
    assert!(cmds.is_empty());

    // Test submission
    let (state, cmds) = update(Msg::Ui(UiMsg::SubmitNote), state);
    assert!(state.ui.is_normal());
    assert_eq!(cmds.len(), 1);

    match &cmds[0] {
        Cmd::Nostr(NostrCmd::SendTextNote { content, .. }) => {
            assert_eq!(content, "Hello");
        }
        _ => panic!("Expected SendTextNote command"),
    }
}

/// Runtime integration test
#[test]
fn test_elm_runtime_integration() {
    let keys = Keys::generate();
    let initial_state = AppState::new(keys.public_key());
    let mut runtime = Runtime::new(initial_state);

    // Test runtime message processing
    runtime.send_msg(Msg::Ui(UiMsg::ShowNewNote));
    let commands = runtime.process_all_messages();

    assert!(runtime.state().ui.is_composing());
    assert!(commands.is_empty());

    // Test statistics
    let stats = runtime.get_stats();
    assert_eq!(stats.queued_messages, 0);
    assert!(stats.is_input_shown);
}

/// Version information test
#[test]
fn test_version_info() {
    assert!(!VERSION.is_empty());
    println!("Nostui version: {VERSION}");
}

/// Complex workflow integration test
#[test]
fn test_complex_workflow() -> Result<()> {
    let keys = Keys::generate();
    let initial_state = AppState::new(keys.public_key());
    let mut runtime = Runtime::new(initial_state);

    // 1. Add event to timeline
    let event = EventBuilder::text_note("Test post").sign_with_keys(&keys)?;
    runtime.send_msg(Msg::Timeline(TimelineMsg::AddNote(event.clone())));

    // 2. Send reaction
    runtime.send_msg(Msg::Nostr(NostrMsg::SendReaction(event.clone())));

    // 3. Start reply
    runtime.send_msg(Msg::Ui(UiMsg::ShowReply(event)));
    runtime.send_msg(Msg::Ui(UiMsg::UpdateInputContent("Nice post!".to_string())));
    runtime.send_msg(Msg::Ui(UiMsg::SubmitNote));

    // Process all messages
    let commands = runtime.process_all_messages();

    // Verification
    assert_eq!(runtime.state().timeline.len(), 1);
    assert!(runtime.state().ui.is_normal());
    assert!(runtime.state().ui.textarea.content.is_empty());

    // Two commands should be generated (reaction + reply)
    assert_eq!(commands.len(), 2);

    let mut has_reaction = false;
    let mut has_reply = false;

    for cmd in &commands {
        match cmd {
            Cmd::Nostr(NostrCmd::SendReaction { .. }) => has_reaction = true,
            Cmd::Nostr(NostrCmd::SendTextNote { content, .. }) => {
                has_reply = true;
                assert_eq!(content, "Nice post!");
            }
            _ => {}
        }
    }

    assert!(has_reaction);
    assert!(has_reply);

    Ok(())
}

/// Error handling integration test
#[test]
#[allow(clippy::unwrap_used)]
fn test_error_handling_integration() {
    let keys = Keys::generate();
    let initial_state = AppState::new(keys.public_key());
    let mut runtime = Runtime::new(initial_state);

    // Send error message
    runtime.send_msg(Msg::System(SystemMsg::ShowError("Test error".to_string())));
    runtime.process_all_messages();

    // Check if error is displayed in status message
    assert!(runtime.state().system.status_message.is_some());
    assert!(runtime
        .state()
        .system
        .status_message
        .as_ref()
        .unwrap()
        .contains("Error: Test error"));
}

/// Asynchronous message processing integration test
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn test_async_message_handling() -> Result<()> {
    let keys = Keys::generate();
    let initial_state = AppState::new(keys.public_key());
    let mut runtime = Runtime::new(initial_state);
    // Ensure input mode is active without relying on keybinding config
    runtime.send_msg(Msg::Ui(UiMsg::ShowNewNote));
    runtime.process_all_messages();
    assert!(runtime.state().ui.is_composing());

    // Send messages asynchronously using RawMsg sequence via translator (typing + submit)
    let raw_sender = runtime.get_raw_sender();
    let handle = tokio::spawn(async move {
        sleep(Duration::from_millis(10)).await;
        // Type content "Async message"
        for ch in "Async message".chars() {
            raw_sender
                .send(RawMsg::Key(KeyEvent::new(
                    KeyCode::Char(ch),
                    KeyModifiers::NONE,
                )))
                .unwrap();
        }
        // Submit with Ctrl+P
        raw_sender
            .send(RawMsg::Key(KeyEvent::new(
                KeyCode::Char('p'),
                KeyModifiers::CONTROL,
            )))
            .unwrap();
    });

    // Wait for task completion
    let timeout = timeout(Duration::from_millis(100), handle).await?;
    timeout?;

    // Wait a bit then process messages
    sleep(Duration::from_millis(20)).await;

    let commands = runtime.process_all_messages();

    // SendTextNote command should be generated
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        Cmd::Nostr(NostrCmd::SendTextNote { content, .. }) => {
            assert_eq!(content, "Async message");
        }
        _ => panic!("Expected SendTextNote command"),
    }

    // Check if UI is reset
    assert!(runtime.state().ui.is_normal());

    Ok(())
}

/// Performance test
#[test]
fn test_performance_many_events() -> Result<()> {
    let keys = Keys::generate();
    let initial_state = AppState::new(keys.public_key());
    let mut runtime = Runtime::new(initial_state);

    let start = Instant::now();

    // Process 1000 events
    for i in 0..1000 {
        let event = EventBuilder::text_note(format!("Event #{i}")).sign_with_keys(&keys)?;
        runtime.send_msg(Msg::Timeline(TimelineMsg::AddNote(event)));
    }

    runtime.process_all_messages();
    let elapsed = start.elapsed();

    println!("Processed 1000 events in {elapsed:?}");

    assert_eq!(runtime.state().timeline.len(), 1000);
    assert!(elapsed < Duration::from_millis(500)); // Should complete within 500ms

    Ok(())
}
