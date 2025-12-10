use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;
use nostui::{
    core::cmd::Cmd, core::cmd_executor::CmdExecutor, core::msg::Msg, core::raw_msg::RawMsg,
    core::state::AppState, core::translator::translate_raw_to_domain,
    integration::elm_integration::ElmRuntime, integration::legacy::action::Action,
};
use tokio::sync::mpsc;

/// Integration tests for command execution system
fn create_test_state() -> AppState {
    AppState::new(Keys::generate().public_key())
}

/// Create test state with proper config for keybindings tests
fn create_test_state_with_config() -> AppState {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use nostui::infrastructure::config::Config;
    use nostui::integration::legacy::{action::Action, mode::Mode};
    use nostui::presentation::config::keybindings::KeyBindings;
    use std::collections::HashMap;

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

    let mut keybindings_map = HashMap::new();
    keybindings_map.insert(Mode::Home, home_bindings);
    config.keybindings = KeyBindings(keybindings_map);

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
    // Set up the pipeline: ElmRuntime -> CmdExecutor -> Actions
    let state = create_test_state_with_config();
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();
    let mut runtime = ElmRuntime::new_with_executor(state, action_tx);

    // Simulate user input: like a post
    let target_event = create_test_event();
    let key_event = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('l'),
        crossterm::event::KeyModifiers::NONE,
    );

    // Add event to timeline first so the key can work
    runtime.send_msg(Msg::AddNote(target_event.clone()));
    runtime.send_msg(Msg::ScrollDown); // Select the note

    // Process messages to update state before testing translator
    let _initial_commands = runtime.process_all_messages();

    // Raw input -> Domain messages
    let raw_msg = RawMsg::Key(key_event);

    // Debug: Test translator directly with updated state
    let translated_msgs =
        nostui::core::translator::translate_raw_to_domain(raw_msg.clone(), runtime.state());
    println!("Translated messages from 'l' key: {:?}", translated_msgs);

    runtime.send_raw_msg(raw_msg);

    // 2. Process all messages and execute commands
    let execution_log = runtime
        .run_update_cycle()
        .expect("Command execution should succeed");

    // Debug: Print execution log
    println!("Execution log: {:?}", execution_log);
    println!(
        "Runtime state: show_input={}, selected_index={:?}",
        runtime.state().ui.show_input,
        runtime.state().timeline.selected_index
    );
    println!("Timeline length: {}", runtime.state().timeline_len());

    // Should have executed the SendReaction command
    assert!(!execution_log.is_empty());
    assert!(execution_log.iter().any(|log| log.contains("SendReaction")));

    // Verify Action was sent to legacy system
    let received_action = action_rx.try_recv()?;
    match received_action {
        Action::SendReaction(received_event) => {
            assert_eq!(received_event.id, target_event.id);
        }
        _ => panic!("Expected SendReaction action, got {:?}", received_action),
    }

    Ok(())
}

#[test]
fn test_text_note_submission_workflow() -> Result<()> {
    let state = create_test_state();
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();
    let mut runtime = ElmRuntime::new_with_executor(state, action_tx);

    // Simulate text note submission workflow
    // Start new note
    runtime.send_msg(Msg::ShowNewNote);

    // Add content
    runtime.send_msg(Msg::UpdateInputContent(
        "Hello, Nostr from Elm!".to_string(),
    ));

    // Submit note
    runtime.send_msg(Msg::SubmitNote);

    // Process and execute commands
    let execution_log = runtime
        .run_update_cycle()
        .expect("Command execution should succeed");

    // Should have executed SendTextNote command
    assert!(execution_log.iter().any(|log| log.contains("SendTextNote")));

    // Verify the action was sent
    let received_action = action_rx.try_recv()?;
    match received_action {
        Action::SendTextNote(content, tags) => {
            assert_eq!(content, "Hello, Nostr from Elm!");
            assert!(tags.is_empty());
        }
        _ => panic!("Expected SendTextNote action, got {:?}", received_action),
    }

    Ok(())
}

#[test]
fn test_reply_workflow_with_tags() -> Result<()> {
    let state = create_test_state();
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();
    let mut runtime = ElmRuntime::new_with_executor(state, action_tx);

    let target_event = create_test_event();

    // Start reply
    runtime.send_msg(Msg::ShowReply(target_event.clone()));
    runtime.send_msg(Msg::UpdateInputContent("Great point!".to_string()));
    runtime.send_msg(Msg::SubmitNote);

    let execution_log = runtime
        .run_update_cycle()
        .expect("Command execution should succeed");
    assert!(execution_log.iter().any(|log| log.contains("SendTextNote")));

    // Verify reply was sent with proper tags
    let received_action = action_rx.try_recv()?;
    match received_action {
        Action::SendTextNote(content, tags) => {
            assert_eq!(content, "Great point!");
            // Should have reply tags
            assert!(!tags.is_empty());
        }
        _ => panic!("Expected SendTextNote action for reply"),
    }

    Ok(())
}

#[test]
fn test_multiple_commands_in_sequence() -> Result<()> {
    let state = create_test_state();
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();
    let mut runtime = ElmRuntime::new_with_executor(state, action_tx);

    let event1 = create_test_event();
    let event2 = create_test_event();

    // Send multiple commands
    runtime.send_msg(Msg::SendReaction(event1.clone()));
    runtime.send_msg(Msg::SendRepost(event2.clone()));
    runtime.send_msg(Msg::System(nostui::core::msg::system::SystemMsg::Resize(
        100, 50,
    )));

    let execution_log = runtime
        .run_update_cycle()
        .expect("Command execution should succeed");
    assert_eq!(execution_log.len(), 3);

    // Verify all actions were sent
    let action1 = action_rx.try_recv()?;
    let action2 = action_rx.try_recv()?;
    let action3 = action_rx.try_recv()?;

    assert!(matches!(action1, Action::SendReaction(_)));
    assert!(matches!(action2, Action::SendRepost(_)));
    assert!(matches!(action3, Action::Resize(100, 50)));

    Ok(())
}

#[test]
fn test_batch_command_execution() -> Result<()> {
    let _state = create_test_state();
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();
    let executor = CmdExecutor::new(action_tx);

    // Create a batch command
    let batch_cmd = Cmd::batch(vec![
        Cmd::Render,
        Cmd::Resize {
            width: 80,
            height: 24,
        },
        Cmd::LogInfo {
            message: "Batch execution test".to_string(),
        },
    ]);

    executor.execute_command(&batch_cmd)?;

    // Should receive Render and Resize actions (LogInfo doesn't generate actions)
    let action1 = action_rx.try_recv()?;
    let action2 = action_rx.try_recv()?;

    assert!(matches!(action1, Action::Render));
    assert!(matches!(action2, Action::Resize(80, 24)));

    // No more actions should be available
    assert!(action_rx.try_recv().is_err());

    Ok(())
}

#[test]
fn test_error_handling_in_execution() -> Result<()> {
    let state = create_test_state();
    let (action_tx, action_rx) = mpsc::unbounded_channel::<Action>();
    let mut runtime = ElmRuntime::new_with_executor(state, action_tx);

    // Drop the action receiver to simulate a closed channel
    drop(action_rx);

    // Try to send a command - should handle the error gracefully
    runtime.send_msg(Msg::SendReaction(create_test_event()));
    let result = runtime.run_update_cycle();

    // The execution should succeed but log the error
    match result {
        Ok(log) => {
            // Should contain error log about failed execution
            assert!(!log.is_empty());
            assert!(log
                .iter()
                .any(|entry| entry.contains("✗ Failed to execute")));
            println!("Error correctly logged: {:?}", log);
        }
        Err(_) => {
            // This shouldn't happen as errors are logged, not returned
            panic!("Expected successful execution with error logging, not failure");
        }
    }

    Ok(())
}

#[test]
fn test_translator_integration_with_executor() -> Result<()> {
    let mut state = create_test_state_with_config();
    let (action_tx, mut _action_rx) = mpsc::unbounded_channel::<Action>();
    let mut runtime = ElmRuntime::new_with_executor(state.clone(), action_tx);

    // Add an event and select it
    let event = create_test_event();
    state.timeline.notes.find_or_insert(std::cmp::Reverse(
        nostui::domain::nostr::SortableEvent::new(event.clone()),
    ));
    state.timeline.selected_index = Some(0);

    // Simulate key presses through translator
    let key_r = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('r'),
        crossterm::event::KeyModifiers::NONE,
    );

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
    assert!(runtime.state().ui.show_input);
    assert!(runtime.state().ui.reply_to.is_some());

    Ok(())
}

#[test]
fn test_performance_with_many_commands() -> Result<()> {
    let state = create_test_state();
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();
    let mut runtime = ElmRuntime::new_with_executor(state, action_tx);

    let start = std::time::Instant::now();

    // Send many commands
    for i in 0..100 {
        runtime.send_msg(Msg::System(nostui::core::msg::system::SystemMsg::Resize(
            100 + i,
            50 + i,
        )));
    }

    let execution_log = runtime
        .run_update_cycle()
        .expect("Command execution should succeed");
    let duration = start.elapsed();

    assert_eq!(execution_log.len(), 100);

    // Should complete in reasonable time (less than 100ms)
    assert!(duration.as_millis() < 100);

    // Verify all actions were sent
    for i in 0..100 {
        let action = action_rx.try_recv()?;
        match action {
            Action::Resize(width, height) => {
                assert_eq!(width, 100 + (i as u16));
                assert_eq!(height, 50 + (i as u16));
            }
            _ => panic!("Expected Resize action"),
        }
    }

    Ok(())
}

#[test]
fn test_runtime_stats_with_executor() -> Result<()> {
    let state = create_test_state();
    let (action_tx, _action_rx) = mpsc::unbounded_channel::<Action>();
    let runtime = ElmRuntime::new_with_executor(state, action_tx);

    let stats = runtime.get_stats();
    assert!(stats.has_executor);
    assert_eq!(stats.queued_messages, 0);
    assert_eq!(stats.queued_commands, 0);
    assert_eq!(stats.timeline_notes_count, 0);

    Ok(())
}
