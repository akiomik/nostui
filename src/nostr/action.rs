use nostr_sdk::prelude::*;

pub enum ConnectionAction {
    SendEvent(Event),
    Shutdown,
}
