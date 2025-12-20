use std::time::Duration;

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
    Cmd, VERSION,
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
fn test_complex_workflow() {
    let keys = Keys::generate();
    let initial_state = AppState::new(keys.public_key());
    let mut runtime = Runtime::new(initial_state);

    // 1. Add event to timeline
    let event = EventBuilder::text_note("Test post")
        .sign_with_keys(&keys)
        .unwrap();
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
}

/// Error handling integration test
#[test]
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
async fn test_async_message_handling() {
    let keys = Keys::generate();
    let initial_state = AppState::new(keys.public_key());
    let mut runtime = Runtime::new(initial_state);
    let sender = runtime.get_sender().unwrap();

    // Send messages asynchronously
    let handle = tokio::spawn(async move {
        sleep(Duration::from_millis(10)).await;
        sender.send(Msg::Ui(UiMsg::ShowNewNote)).unwrap();
        sender
            .send(Msg::Ui(UiMsg::UpdateInputContent(
                "Async message".to_string(),
            )))
            .unwrap();
        sender.send(Msg::Ui(UiMsg::SubmitNote)).unwrap();
    });

    // Wait for task completion
    let timeout = timeout(Duration::from_millis(100), handle).await;
    assert!(timeout.is_ok());
    timeout.unwrap().unwrap();

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
}

/// Performance test
#[test]
fn test_performance_many_events() {
    let keys = Keys::generate();
    let initial_state = AppState::new(keys.public_key());
    let mut runtime = Runtime::new(initial_state);

    let start = Instant::now();

    // Process 1000 events
    for i in 0..1000 {
        let event = EventBuilder::text_note(format!("Event #{i}"))
            .sign_with_keys(&keys)
            .unwrap();
        runtime.send_msg(Msg::Timeline(TimelineMsg::AddNote(event)));
    }

    runtime.process_all_messages();
    let elapsed = start.elapsed();

    println!("Processed 1000 events in {elapsed:?}");

    assert_eq!(runtime.state().timeline.len(), 1000);
    assert!(elapsed < Duration::from_millis(500)); // Should complete within 500ms
}
