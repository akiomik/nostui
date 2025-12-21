use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;
use tokio::sync::{
    broadcast::{self, error::RecvError},
    mpsc,
};
use tokio_util::sync::CancellationToken;

use crate::{domain::nostr::Connection, infrastructure::nostr::NostrOperation, RawMsg};

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
    op_rx: mpsc::UnboundedReceiver<NostrOperation>,
    cancel_token: CancellationToken,
    // Outgoing channels
    raw_tx: mpsc::UnboundedSender<RawMsg>, // For RawMsg notifications
}

pub type NewNostrService = (
    mpsc::UnboundedSender<NostrOperation>, // op_tx - operations to send
    CancellationToken,                     // shutdown signal
    NostrService,
);

impl NostrService {
    /// Create a new NostrService
    pub fn new(
        conn: Connection,
        keys: Keys,
        raw_tx: mpsc::UnboundedSender<RawMsg>,
    ) -> Result<NewNostrService> {
        let (op_tx, op_rx) = mpsc::unbounded_channel();
        let cancel_token = CancellationToken::new();

        Ok((
            op_tx,
            cancel_token.clone(),
            Self {
                conn,
                keys,
                op_rx,
                cancel_token,
                raw_tx,
            },
        ))
    }

    /// Run the NostrService in background task
    pub fn run(mut self) {
        tokio::spawn(async move {
            let result = self.run_service().await;
            if let Err(e) = result {
                log::error!("NostrService error: {e}");
                let _ = self
                    .raw_tx
                    .send(RawMsg::Error(format!("NostrService error: {e}",)));
            }
        });
    }

    /// Main service loop
    async fn run_service(&mut self) -> Result<()> {
        let mut timeline = self.conn.subscribe_timeline().await?;

        loop {
            tokio::select! {
                // Handle received events from timeline
                result = timeline.recv() => {
                    match result {
                        Ok(notification) => {
                            if let RelayPoolNotification::Event { event, .. } = notification {
                                let _ = self.raw_tx.send(RawMsg::ReceiveEvent(*event));
                            }
                        }
                        Err(RecvError::Lagged(n)) => {
                            log::warn!("Missed {n} messages from timeline");
                        }
                        Err(RecvError::Closed) => {
                            log::error!("Timeline channel closed");
                            break;
                        }
                    }
                }

                // Handle outgoing operations
                result = self.op_rx.recv() => {
                    if let Some(op) = result {
                        match self.handle_operation(op, &mut timeline).await {
                            Ok(should_continue) => {
                                if !should_continue {
                                    // Operation requested service termination
                                    break;
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to handle operation: {e}");
                                let _ = self
                                    .raw_tx
                                    .send(RawMsg::Error(format!("Operation failed: {e}",)));
                            }
                        }
                    }
                }

                // Check for termination signal
                _ = self.cancel_token.cancelled() => {
                    log::info!("NostrService received cancellation signal");
                    break;
                }
            }
        }

        // Ensure we close the underlying connection when terminating the service loop
        log::info!("NostrService: closing Nostr connection");
        self.conn.close().await;
        let _ = self.raw_tx.send(RawMsg::SystemMessage(
            "Disconnected from all relays".to_string(),
        ));

        Ok(())
    }

    /// Handle a NostrOperation by signing and sending appropriate events
    /// Returns true if the service should continue, false if it should terminate
    async fn handle_operation(
        &mut self,
        op: NostrOperation,
        timeline: &mut broadcast::Receiver<RelayPoolNotification>,
    ) -> Result<bool> {
        log::debug!("Handling NostrOperation: {op:?}");

        match op {
            NostrOperation::SendReaction {
                target_event,
                content,
            } => {
                // Now we can use the proper nostr-sdk API with the full Event
                let event =
                    EventBuilder::reaction(&target_event, &content).sign_with_keys(&self.keys)?;
                self.conn.send(event).await?;

                let note_bech32 = target_event.id.to_bech32()?;
                let status = format!("[Reacted {content}] {note_bech32}");
                let _ = self.raw_tx.send(RawMsg::SystemMessage(status));
            }

            NostrOperation::SendRepost {
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
                    format!("[Reposted with comment] {note_bech32}")
                } else {
                    format!("[Reposted] {note_bech32}")
                };
                let _ = self.raw_tx.send(RawMsg::SystemMessage(status));
            }

            NostrOperation::SendTextNote { content, tags } => {
                log::info!(
                    "NostrService: Processing SendTextNote - content: '{content}', tags: {tags:?}"
                );
                let event = EventBuilder::text_note(&content)
                    .tags(tags)
                    .sign_with_keys(&self.keys)?;
                // log::debug!("NostrService: Signed event: {}", event.id);
                self.conn.send(event).await?;
                log::info!("NostrService: Successfully sent event to network");

                let status = format!("[Posted] {content}");
                let _ = self.raw_tx.send(RawMsg::SystemMessage(status));
            }

            NostrOperation::ConnectToRelays { relays } => {
                // Dynamic relay connection not supported (same as legacy implementation)
                log::info!("Connect to relays requested: {relays:?}");
                let status = "Dynamic relay connection not supported. Restart application with new relay config.".to_string();
                let _ = self.raw_tx.send(RawMsg::SystemMessage(status));
            }

            NostrOperation::DisconnectFromRelays => {
                // Disconnect all relays and terminate service (same behavior as legacy)
                log::info!("Disconnect from all relays requested");
                let _ = self.raw_tx.send(RawMsg::SystemMessage(
                    "Disconnecting from all relays...".to_string(),
                ));
                // Explicitly close before terminating to avoid resource leak
                self.conn.close().await;
                return Ok(false); // Signal to terminate the service
            }

            NostrOperation::SubscribeToTimeline => {
                // Re-subscribe to timeline
                *timeline = self.conn.subscribe_timeline().await?;
                let _ = self.raw_tx.send(RawMsg::SystemMessage(
                    "Timeline subscription refreshed".to_string(),
                ));
            }

            NostrOperation::UpdateProfile { metadata } => {
                let event = EventBuilder::metadata(&metadata).sign_with_keys(&self.keys)?;
                self.conn.send(event).await?;

                let status = "Profile updated".to_string();
                let _ = self.raw_tx.send(RawMsg::SystemMessage(status));
            }

            NostrOperation::SendDirectMessage {
                recipient_pubkey,
                content,
            } => {
                // DM feature not available (same as legacy implementation)
                log::info!("DM to {recipient_pubkey}: {content}");
                let recipient_hex = recipient_pubkey.to_hex()[0..8].to_string();
                let status = format!("[DM feature not available] to {recipient_hex}");
                let _ = self.raw_tx.send(RawMsg::SystemMessage(status));
            }
        }

        Ok(true) // Continue service by default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::nostr::Connection;
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
    async fn test_nostr_service_creation() -> Result<()> {
        let conn = create_test_connection().await;
        let keys = create_test_keys();
        let (raw_tx, _raw_rx) = mpsc::unbounded_channel();

        let (op_tx, cancel_token, _service) = NostrService::new(conn, keys, raw_tx)?;

        // Verify channels are created
        assert!(op_tx.send(NostrOperation::SubscribeToTimeline).is_ok());
        cancel_token.cancel();

        Ok(())
    }

    #[test]
    fn test_nostr_operation_creation_helpers() -> Result<()> {
        let keys = Keys::generate();
        let event = EventBuilder::text_note("test event").sign_with_keys(&keys)?;

        let like_op = NostrOperation::like(event.clone());
        assert_eq!(like_op.name(), "SendReaction");

        let text_op = NostrOperation::simple_text_note("Hello, Nostr!");
        assert_eq!(text_op.name(), "SendTextNote");

        let repost_op = NostrOperation::repost(event, Some("Great post!".to_string()));
        assert_eq!(repost_op.name(), "SendRepost");

        Ok(())
    }

    // Note: Full integration tests with actual network connections
    // should be in integration test files, not unit tests
}
