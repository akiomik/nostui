use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NostrMsg {
    SendReaction(Event),
    SendRepost(Event),
    SendTextNote(String, Vec<Tag>),
}
