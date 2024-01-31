use crossterm::event::KeyEvent;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use strum::Display;

use crate::nostr::NostrAction;

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
    ReceiveNostrEvent(Event),
    ScrollUp,
    ScrollDown,
    ScrollToTop,
    ScrollToBottom,
    React,
    Repost,
    Unselect,
    NewTextNote,
    ReplyTextNote,
    SubmitTextNote,
    Key(KeyEvent),
    SystemMessage(String),
    SendNostrAction(NostrAction),
}
