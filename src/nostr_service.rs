use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;
use tokio::sync::mpsc;

use crate::{action::Action, nostr::Connection, nostr_command::NostrCommand};

/// NostrService handles all Nostr protocol operations including signing and sending events
/// Evolved from ConnectionProcess with expanded responsibilities:
/// - Event signing with stored keys
/// - WebSocket connection management
/// - Relay management
/// - Timeline subscription
pub struct NostrService {
    conn: Connection,
    keys: Keys,
    // Incoming channels
    cmd_rx: mpsc::UnboundedReceiver<NostrCommand>,
    terminate_rx: mpsc::UnboundedReceiver<()>,
    // Outgoing channels
    event_tx: mpsc::UnboundedSender<Event>, // For received events
    action_tx: mpsc::UnboundedSender<Action>, // For errors and status updates
}

pub type NewNostrService = (
    mpsc::UnboundedReceiver<Event>,      // req_rx - received events
    mpsc::UnboundedSender<NostrCommand>, // cmd_tx - commands to send
    mpsc::UnboundedSender<()>,           // terminate_tx - shutdown signal
    NostrService,
);

impl NostrService {
    /// Create a new NostrService
    pub fn new(
        conn: Connection,
        keys: Keys,
        action_tx: mpsc::UnboundedSender<Action>,
    ) -> Result<NewNostrService> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (terminate_tx, terminate_rx) = mpsc::unbounded_channel();

        Ok((
            event_rx,
            cmd_tx,
            terminate_tx,
            Self {
                conn,
                keys,
                cmd_rx,
                terminate_rx,
                event_tx,
                action_tx,
            },
        ))
    }

    /// Run the NostrService in background task
    pub fn run(mut self) {
        tokio::spawn(async move {
            let result = self.run_service().await;
            if let Err(e) = result {
                log::error!("NostrService error: {}", e);
                let _ = self
                    .action_tx
                    .send(Action::NostrError(format!("NostrService error: {}", e)));
            }
        });
    }

    /// Main service loop
    async fn run_service(&mut self) -> Result<()> {
        let mut timeline = self.conn.subscribe_timeline().await?;

        loop {
            // Handle received events from timeline
            while let Ok(notification) = timeline.try_recv() {
                if let RelayPoolNotification::Event { event, .. } = notification {
                    if let Err(_e) = self.event_tx.send(*event) {
                        // log::error!("Failed to send received event: {}", e);
                    }
                }
            }

            // Handle outgoing commands
            while let Ok(cmd) = self.cmd_rx.try_recv() {
                match self.handle_command(cmd, &mut timeline).await {
                    Ok(should_continue) => {
                        if !should_continue {
                            // Command requested service termination
                            break;
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to handle command: {}", e);
                        let _ = self
                            .action_tx
                            .send(Action::NostrError(format!("Command failed: {}", e)));
                    }
                }
            }

            // Check for termination signal
            if self.terminate_rx.try_recv().is_ok() {
                log::info!("NostrService received termination signal");
                break;
            }

            // Small yield to prevent busy waiting
            tokio::task::yield_now().await;
        }

        Ok(())
    }

    /// Handle a NostrCommand by signing and sending appropriate events
    /// Returns true if the service should continue, false if it should terminate
    async fn handle_command(
        &mut self,
        cmd: NostrCommand,
        timeline: &mut tokio::sync::broadcast::Receiver<RelayPoolNotification>,
    ) -> Result<bool> {
        log::debug!("Handling NostrCommand: {:?}", cmd);

        match cmd {
            NostrCommand::SendReaction {
                target_event,
                content,
            } => {
                // Now we can use the proper nostr-sdk API with the full Event
                let event =
                    EventBuilder::reaction(&target_event, &content).sign_with_keys(&self.keys)?;
                self.conn.send(event).await?;

                let note_bech32 = target_event.id.to_bech32()?;
                let status = format!("[Reacted {}] {}", content, note_bech32);
                let _ = self.action_tx.send(Action::SystemMessage(status));
            }

            NostrCommand::SendRepost {
                target_event,
                reason,
            } => {
                // Use proper nostr-sdk repost API
                let event = if let Some(relay_url) =
                    reason.as_ref().and_then(|r| RelayUrl::parse(r).ok())
                {
                    EventBuilder::repost(&target_event, Some(relay_url))
                } else {
                    EventBuilder::repost(&target_event, None)
                };
                let signed_event = event.sign_with_keys(&self.keys)?;
                self.conn.send(signed_event).await?;

                let note_bech32 = target_event.id.to_bech32()?;
                let status = if reason.is_some() {
                    format!("[Reposted with comment] {}", note_bech32)
                } else {
                    format!("[Reposted] {}", note_bech32)
                };
                let _ = self.action_tx.send(Action::SystemMessage(status));
            }

            NostrCommand::SendTextNote { content, tags } => {
                log::info!(
                    "NostrService: Processing SendTextNote - content: '{}', tags: {:?}",
                    content,
                    tags
                );
                let event = EventBuilder::text_note(&content)
                    .tags(tags)
                    .sign_with_keys(&self.keys)?;
                // log::debug!("NostrService: Signed event: {}", event.id);
                self.conn.send(event).await?;
                log::info!("NostrService: Successfully sent event to network");

                let status = format!("[Posted] {}", content);
                let _ = self.action_tx.send(Action::SystemMessage(status));
            }

            NostrCommand::ConnectToRelays { relays } => {
                // Dynamic relay connection not supported (same as legacy implementation)
                log::info!("Connect to relays requested: {:?}", relays);
                let status = "Dynamic relay connection not supported. Restart application with new relay config.".to_string();
                let _ = self.action_tx.send(Action::SystemMessage(status));
            }

            NostrCommand::DisconnectFromRelays => {
                // Disconnect all relays and terminate service (same behavior as legacy)
                log::info!("Disconnect from all relays requested");
                let _ = self.action_tx.send(Action::SystemMessage(
                    "Disconnecting from all relays...".to_string(),
                ));
                return Ok(false); // Signal to terminate the service
            }

            NostrCommand::SubscribeToTimeline => {
                // Re-subscribe to timeline
                *timeline = self.conn.subscribe_timeline().await?;
                let _ = self.action_tx.send(Action::SystemMessage(
                    "Timeline subscription refreshed".to_string(),
                ));
            }

            NostrCommand::UpdateProfile { metadata } => {
                let event = EventBuilder::metadata(&metadata).sign_with_keys(&self.keys)?;
                self.conn.send(event).await?;

                let status = "Profile updated".to_string();
                let _ = self.action_tx.send(Action::SystemMessage(status));
            }

            NostrCommand::SendDirectMessage {
                recipient_pubkey,
                content,
            } => {
                // DM feature not available (same as legacy implementation)
                log::info!("DM to {}: {}", recipient_pubkey, content);
                let recipient_hex = recipient_pubkey.to_hex()[0..8].to_string();
                let status = format!("[DM feature not available] to {}", recipient_hex);
                let _ = self.action_tx.send(Action::SystemMessage(status));
            }
        }

        Ok(true) // Continue service by default
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nostr::Connection;
    use tokio::sync::mpsc;

    async fn create_test_connection() -> Connection {
        let keys = Keys::generate();
        let relays = vec!["wss://relay.damus.io".to_string()];
        // Note: This will fail in tests without network, but demonstrates the interface
        Connection::new(keys, relays)
            .await
            .expect("Failed to create test connection")
    }

    fn create_test_keys() -> Keys {
        Keys::generate()
    }

    #[tokio::test]
    async fn test_nostr_service_creation() {
        let conn = create_test_connection().await;
        let keys = create_test_keys();
        let (action_tx, _action_rx) = mpsc::unbounded_channel();

        let result = NostrService::new(conn, keys, action_tx);
        assert!(result.is_ok());

        let (mut event_rx, cmd_tx, terminate_tx, _service) = result.unwrap();

        // Verify channels are created
        assert!(event_rx.try_recv().is_err()); // Should be empty initially
        assert!(cmd_tx.send(NostrCommand::SubscribeToTimeline).is_ok());
        assert!(terminate_tx.send(()).is_ok());
    }

    #[test]
    fn test_nostr_command_creation_helpers() {
        let keys = Keys::generate();
        let event = EventBuilder::text_note("test event")
            .sign_with_keys(&keys)
            .unwrap();

        let like_cmd = NostrCommand::like(event.clone());
        assert_eq!(like_cmd.name(), "SendReaction");

        let text_cmd = NostrCommand::simple_text_note("Hello, Nostr!");
        assert_eq!(text_cmd.name(), "SendTextNote");

        let repost_cmd = NostrCommand::repost(event, Some("Great post!".to_string()));
        assert_eq!(repost_cmd.name(), "SendRepost");
    }

    // Note: Full integration tests with actual network connections
    // should be in integration test files, not unit tests
}
