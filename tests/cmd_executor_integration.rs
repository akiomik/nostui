use std::cmp::Reverse;
use std::collections::HashMap;

use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nostr_sdk::prelude::*;
use tokio::sync::mpsc;

use nostui::core::cmd::TuiCmd;
use nostui::core::msg::nostr::NostrMsg;
use nostui::core::msg::system::SystemMsg;
use nostui::core::msg::ui::UiMsg;
use nostui::domain::nostr::SortableEvent;
use nostui::infrastructure::config::Config;
use nostui::presentation::config::keybindings::{Action, KeyBindings};
use nostui::{
    core::{
        cmd::Cmd,
        cmd_executor::CmdExecutor,
        msg::{timeline::TimelineMsg, Msg},
        raw_msg::RawMsg,
        state::AppState,
        translator::translate_raw_to_domain,
    },
    integration::runtime::Runtime,
};

/// Integration tests for command execution system
fn create_test_state() -> AppState {
    AppState::new(Keys::generate().public_key())
}

/// Create test state with proper config for keybindings tests
fn create_test_state_with_config() -> AppState {
    // Create config with test keybindings
    let mut config = Config::default();

    // Create test keybindings that match expected behavior
    let mut home_bindings = HashMap::new();
    home_bindings.insert(
        vec![KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)],
        Action::ScrollDown,
    );
    home_bindings.insert(
        vec![KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE)],
        Action::ScrollUp,
    );
    home_bindings.insert(
        vec![KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE)],
        Action::React,
    );
    home_bindings.insert(
        vec![KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)],
        Action::ReplyTextNote,
    );
    home_bindings.insert(
        vec![KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE)],
        Action::Repost,
    );
    home_bindings.insert(
        vec![KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)],
        Action::NewTextNote,
    );
    home_bindings.insert(
        vec![KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)],
        Action::Unselect,
    );

    config.keybindings = KeyBindings(home_bindings);

    AppState::new_with_config(Keys::generate().public_key(), config)
}

fn create_test_event() -> Event {
    let keys = Keys::generate();
    EventBuilder::text_note("test content")
        .sign_with_keys(&keys)
        .unwrap()
}

#[test]
fn test_complete_elm_to_action_workflow() -> Result<()> {
    // Set up the pipeline: Runtime -> CmdExecutor -> Actions
    let state = create_test_state_with_config();
    let (_action_tx, mut action_rx) = mpsc::unbounded_channel::<()>();
    let mut runtime = Runtime::new_with_executor(state);

    // Simulate user input: like a post
    let target_event = create_test_event();
    let key_event = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE);

    // Add event to timeline first so the key can work
    runtime.send_msg(Msg::Timeline(TimelineMsg::AddNote(target_event)));
    runtime.send_msg(Msg::Timeline(TimelineMsg::ScrollDown)); // Select the note

    // Process messages to update state before testing translator
    let _initial_commands = runtime.process_all_messages();

    // Raw input -> Domain messages
    let raw_msg = RawMsg::Key(key_event);

    // Debug: Test translator directly with updated state
    let translated_msgs = translate_raw_to_domain(raw_msg.clone(), runtime.state());
    println!("Translated messages from 'l' key: {translated_msgs:?}");

    runtime.send_raw_msg(raw_msg);

    // 2. Process all messages and execute commands
    let execution_log = runtime
        .run_update_cycle()
        .expect("Command execution should succeed");

    // Debug: Print execution log
    println!("Execution log: {execution_log:?}");
    println!(
        "Runtime state: input_mode={}, selected_index={:?}",
        runtime.state().ui.is_composing(),
        runtime.state().timeline.selected_index
    );
    println!("Timeline length: {}", runtime.state().timeline.len());

    // Should have executed the SendReaction command
    assert!(!execution_log.is_empty());
    assert!(execution_log.iter().any(|log| log.contains("SendReaction")));

    // Without Nostr executor, no Action should be sent (command is dropped with warning)
    assert!(action_rx.try_recv().is_err());

    Ok(())
}

#[test]
fn test_text_note_submission_workflow() -> Result<()> {
    let state = create_test_state();
    let (_action_tx, mut action_rx) = mpsc::unbounded_channel::<()>();
    let mut runtime = Runtime::new_with_executor(state);

    // Simulate text note submission workflow
    // Start new note
    runtime.send_msg(Msg::Ui(UiMsg::ShowNewNote));

    // Add content
    runtime.send_msg(Msg::Ui(UiMsg::UpdateInputContent(
        "Hello, Nostr from Elm!".to_string(),
    )));

    // Submit note
    runtime.send_msg(Msg::Ui(UiMsg::SubmitNote));

    // Process and execute commands
    let execution_log = runtime
        .run_update_cycle()
        .expect("Command execution should succeed");

    // Should have executed SendTextNote command
    assert!(execution_log.iter().any(|log| log.contains("SendTextNote")));

    // Without Nostr executor, no Action should be sent (command is dropped with warning)
    assert!(action_rx.try_recv().is_err());

    Ok(())
}

#[test]
fn test_reply_workflow_with_tags() -> Result<()> {
    let state = create_test_state();
    let (_action_tx, mut action_rx) = mpsc::unbounded_channel::<()>();
    let mut runtime = Runtime::new_with_executor(state);

    let target_event = create_test_event();

    // Start reply
    runtime.send_msg(Msg::Ui(UiMsg::ShowReply(target_event)));
    runtime.send_msg(Msg::Ui(UiMsg::UpdateInputContent(
        "Great point!".to_string(),
    )));
    runtime.send_msg(Msg::Ui(UiMsg::SubmitNote));

    let execution_log = runtime
        .run_update_cycle()
        .expect("Command execution should succeed");
    assert!(execution_log.iter().any(|log| log.contains("SendTextNote")));

    // Without Nostr executor, no Action should be sent (command is dropped with warning)
    assert!(action_rx.try_recv().is_err());

    Ok(())
}

#[test]
fn test_multiple_commands_in_sequence() -> Result<()> {
    let state = create_test_state();
    let (_action_tx, mut action_rx) = mpsc::unbounded_channel::<()>();
    let mut runtime = Runtime::new_with_executor(state);

    let event1 = create_test_event();
    let event2 = create_test_event();

    // Send multiple commands
    runtime.send_msg(Msg::Nostr(NostrMsg::SendReaction(event1)));
    runtime.send_msg(Msg::Nostr(NostrMsg::SendRepost(event2)));
    runtime.send_msg(Msg::System(SystemMsg::Resize(100, 50)));

    // Provide TUI sender BEFORE executing to capture resize command
    let (tui_tx, mut tui_rx) = mpsc::unbounded_channel::<TuiCmd>();
    runtime.add_tui_sender(tui_tx).unwrap();

    let execution_log = runtime
        .run_update_cycle()
        .expect("Command execution should succeed");
    assert_eq!(execution_log.len(), 3);

    // Without Nostr executor, no Nostr actions should be sent (commands are dropped with warning)
    assert!(action_rx.try_recv().is_err());

    // Verify resize TUI command
    let tui_cmd = tui_rx.try_recv()?;
    assert!(matches!(
        tui_cmd,
        TuiCmd::Resize {
            width: 100,
            height: 50
        }
    ));

    Ok(())
}

#[test]
fn test_batch_command_execution() -> Result<()> {
    let _state = create_test_state();
    let mut executor = CmdExecutor::new();

    // Create a batch command
    let batch_cmd = Cmd::batch(vec![
        Cmd::RequestRender,
        Cmd::Tui(TuiCmd::Resize {
            width: 80,
            height: 24,
        }),
        Cmd::LogInfo {
            message: "Batch execution test".to_string(),
        },
    ]);

    // Route TUI and Render requests through dedicated channels BEFORE execution
    let (tui_tx, mut tui_rx) = mpsc::unbounded_channel::<TuiCmd>();
    executor.set_tui_sender(tui_tx);
    let (render_tx, mut render_rx) = mpsc::channel::<()>(1);
    executor.set_render_request_sender(render_tx);

    executor.execute_command(&batch_cmd)?;

    // Should receive RequestRender signal and Resize TUI command (LogInfo doesn't generate actions)
    render_rx.try_recv()?;
    let tui_cmd = tui_rx.try_recv()?;
    assert!(matches!(
        tui_cmd,
        TuiCmd::Resize {
            width: 80,
            height: 24
        }
    ));

    // No more TUI commands or render signals should be available (coalesced channel)
    assert!(tui_rx.try_recv().is_err());
    assert!(render_rx.try_recv().is_err());

    Ok(())
}

#[test]
fn test_error_handling_in_execution() -> Result<()> {
    let state = create_test_state();
    let (_action_tx, action_rx) = mpsc::unbounded_channel::<()>();
    let mut runtime = Runtime::new_with_executor(state);

    // Drop the action receiver to simulate a closed channel
    drop(action_rx);

    // Try to send a command - should handle the error gracefully
    runtime.send_msg(Msg::Nostr(NostrMsg::SendReaction(create_test_event())));
    let result = runtime.run_update_cycle();

    // The execution should succeed and simply ignore the command when Nostr is unavailable
    match result {
        Ok(log) => {
            // Should be empty or contain only non-error entries
            assert!(log
                .iter()
                .all(|entry| entry.contains("✓ Executed") || entry.is_empty()));
        }
        Err(_) => {
            panic!("Expected successful execution without errors");
        }
    }

    Ok(())
}

#[test]
fn test_translator_integration_with_executor() -> Result<()> {
    let mut state = create_test_state_with_config();
    let (_action_tx, mut _action_rx) = mpsc::unbounded_channel::<()>();
    let mut runtime = Runtime::new_with_executor(state.clone());

    // Add an event and select it
    let event = create_test_event();
    state
        .timeline
        .notes
        .find_or_insert(Reverse(SortableEvent::new(event)));
    state.timeline.selected_index = Some(0);

    // Simulate key presses through translator
    let key_r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);

    // Translate key to domain messages
    let messages = translate_raw_to_domain(RawMsg::Key(key_r), &state);
    assert!(!messages.is_empty());

    // Send messages through runtime
    for msg in messages {
        runtime.send_msg(msg);
    }

    let _execution_log = runtime
        .run_update_cycle()
        .expect("Command execution should succeed");

    // Should have triggered ShowReply message (no command)
    // The actual reply submission would happen when user presses Enter
    assert!(runtime.state().ui.is_composing());
    assert!(runtime.state().ui.reply_to.is_some());

    Ok(())
}

#[test]
fn test_performance_with_many_commands() -> Result<()> {
    let state = create_test_state();
    let mut runtime = Runtime::new_with_executor(state);

    // Provide TUI sender BEFORE executing to capture resize commands
    let (tui_tx, mut tui_rx) = mpsc::unbounded_channel::<TuiCmd>();
    runtime.add_tui_sender(tui_tx).unwrap();

    let start = Instant::now();

    // Send many commands
    for i in 0..100 {
        runtime.send_msg(Msg::System(SystemMsg::Resize(100 + i, 50 + i)));
    }

    let execution_log = runtime
        .run_update_cycle()
        .expect("Command execution should succeed");
    let duration = start.elapsed();

    assert_eq!(execution_log.len(), 100);

    // Should complete in reasonable time (less than 100ms)
    assert!(duration.as_millis() < 100);

    // Verify all resize commands were sent via TUI channel
    for i in 0..100 {
        let tui_cmd = tui_rx.try_recv()?;
        match tui_cmd {
            TuiCmd::Resize { width, height } => {
                assert_eq!(width, 100 + (i as u16));
                assert_eq!(height, 50 + (i as u16));
            }
        }
    }

    Ok(())
}

#[test]
fn test_runtime_stats_with_executor() -> Result<()> {
    let state = create_test_state();
    let runtime = Runtime::new_with_executor(state);

    let stats = runtime.get_stats();
    assert!(stats.has_executor);
    assert_eq!(stats.queued_messages, 0);
    assert_eq!(stats.queued_commands, 0);
    assert_eq!(stats.timeline_notes_count, 0);

    Ok(())
}
