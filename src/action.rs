use std::{fmt, string::ToString};

use crossterm::event::KeyEvent;
use nostr_sdk::prelude::*;
use serde::{
    de::{self, Deserializer, Visitor},
    Deserialize, Serialize,
};
use strum::Display;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Display, Deserialize)]
pub enum Action {
    Tick,
    Render,
    Resize(u16, u16),
    Suspend,
    Resume,
    Quit,
    Refresh,
    Error(String),
    Help,
    ReceiveEvent(Event),
    ScrollUp,
    ScrollDown,
    ScrollToTop,
    ScrollToBottom,
    React,
    SendReaction((EventId, XOnlyPublicKey)),
    Repost,
    SendRepost((EventId, XOnlyPublicKey)),
    Unselect,
    NewTextNote,
    SubmitTextNote,
    SendTextNote(String),
    Key(KeyEvent),
    MetadataUpdated(Metadata),
}
