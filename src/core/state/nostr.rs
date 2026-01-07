use tokio::sync::mpsc;

use crate::core::{
    cmd::{Cmd, NostrCmd},
    msg::nostr::NostrMsg,
};
use crate::tears::subscription::nostr::NostrCommand;

#[derive(Debug, Clone, Default)]
pub struct NostrState {
    /// Command sender for NostrEvents subscription
    /// This is set when the subscription emits a Ready message
    pub command_sender: Option<mpsc::UnboundedSender<NostrCommand>>,
}

impl NostrState {
    /// Handle Nostr operations and produce the corresponding commands.
    /// Note: This state is currently stateless; status messages are set by the coordinator (update.rs).
    pub fn update(&mut self, msg: NostrMsg) -> Vec<Cmd> {
        match msg {
            NostrMsg::SendReaction(target_event) => {
                vec![Cmd::Nostr(NostrCmd::SendReaction { target_event })]
            }
            NostrMsg::SendRepost(target_event) => {
                vec![Cmd::Nostr(NostrCmd::SendRepost { target_event })]
            }
            NostrMsg::SendTextNote(content, tags) => {
                vec![Cmd::Nostr(NostrCmd::SendTextNote { content, tags })]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use color_eyre::Result;
    use nostr_sdk::prelude::*;

    fn create_event() -> Result<Event> {
        let keys = Keys::generate();
        EventBuilder::text_note("t")
            .sign_with_keys(&keys)
            .map_err(|e| e.into())
    }

    #[test]
    fn test_nostr_state_send_reaction() -> Result<()> {
        let mut ns = NostrState::default();
        let ev = create_event()?;
        let cmds = ns.update(NostrMsg::SendReaction(ev.clone()));

        assert!(matches!(
            cmds.as_slice(),
            [Cmd::Nostr(NostrCmd::SendReaction { target_event })] if target_event.id == ev.id
        ));

        Ok(())
    }

    #[test]
    fn test_nostr_state_send_repost() -> Result<()> {
        let mut ns = NostrState::default();
        let ev = create_event()?;
        let cmds = ns.update(NostrMsg::SendRepost(ev.clone()));

        assert!(matches!(
            cmds.as_slice(),
            [Cmd::Nostr(NostrCmd::SendRepost { target_event })] if target_event.id == ev.id
        ));

        Ok(())
    }

    #[test]
    fn test_nostr_state_send_text_note() {
        let mut ns = NostrState::default();
        let cmds = ns.update(NostrMsg::SendTextNote("hello".into(), vec![]));

        assert!(matches!(
            cmds.as_slice(),
            [Cmd::Nostr(NostrCmd::SendTextNote { content, tags })] if content == "hello" && tags.is_empty()
        ));
    }
}
