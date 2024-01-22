use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use strum::Display;

pub enum ConnectionAction {
    SendEvent(Event),
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Display, Deserialize)]
pub enum NostrAction {
    SendTextNote(String, Vec<Tag>),
    SendReaction(EventId, XOnlyPublicKey),
    SendRepost(EventId, XOnlyPublicKey),
}
