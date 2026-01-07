use tokio::sync::mpsc;

use crate::tears::subscription::nostr::NostrCommand;

#[derive(Debug, Clone, Default)]
pub struct NostrState {
    /// Command sender for NostrEvents subscription
    /// This is set when the subscription emits a Ready message
    pub command_sender: Option<mpsc::UnboundedSender<NostrCommand>>,
}
