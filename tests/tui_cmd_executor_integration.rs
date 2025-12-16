use nostui::core::cmd::{Cmd, TuiCommand};
use nostui::core::cmd_executor::CmdExecutor;
use tokio::sync::mpsc;

#[tokio::test]
async fn cmd_executor_sends_tui_command_when_sender_is_present() {
    // Arrange
    let (_action_tx, mut action_rx) = mpsc::unbounded_channel::<()>();
    let mut exec = CmdExecutor::new();

    let (tui_tx, mut tui_rx) = mpsc::unbounded_channel::<TuiCommand>();
    exec.set_tui_sender(tui_tx);

    // Act: execute a TUI resize command
    exec.execute_command(&Cmd::Tui(TuiCommand::Resize {
        width: 80,
        height: 24,
    }))
    .expect("execute_command should succeed");

    // Assert: no Action was emitted (handled by TUI sender path)
    assert!(
        action_rx.try_recv().is_err(),
        "no Action should be emitted when TUI sender is present"
    );

    // Assert: TuiCommand was sent to the TUI channel
    match tui_rx.try_recv() {
        Ok(TuiCommand::Resize { width, height }) => {
            assert_eq!(width, 80);
            assert_eq!(height, 24);
        }
        other => panic!("expected a Resize command on tui_rx, got: {:?}", other),
    }
}
