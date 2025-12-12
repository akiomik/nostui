use crate::core::{cmd::Cmd, msg::nostr::NostrMsg};

#[derive(Debug, Clone, Default)]
pub struct NostrState;

impl NostrState {
    /// Handle Nostr operations and produce the corresponding commands.
    /// Note: This state is currently stateless; status messages are set by the coordinator (update.rs).
    pub fn update(&mut self, msg: NostrMsg) -> Vec<Cmd> {
        match msg {
            NostrMsg::SendReaction(target_event) => {
                vec![Cmd::SendReaction { target_event }]
            }
            NostrMsg::SendRepost(target_event) => {
                vec![Cmd::SendRepost { target_event }]
            }
            NostrMsg::SendTextNote(content, tags) => {
                vec![Cmd::SendTextNote { content, tags }]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::*;

    fn create_event() -> Event {
        let keys = Keys::generate();
        EventBuilder::text_note("t").sign_with_keys(&keys).unwrap()
    }

    #[test]
    fn test_nostr_state_send_reaction() {
        let mut ns = NostrState;
        let ev = create_event();
        let cmds = ns.update(NostrMsg::SendReaction(ev.clone()));
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::SendReaction { target_event } => assert_eq!(target_event.id, ev.id),
            _ => panic!("expected SendReaction"),
        }
    }

    #[test]
    fn test_nostr_state_send_repost() {
        let mut ns = NostrState;
        let ev = create_event();
        let cmds = ns.update(NostrMsg::SendRepost(ev.clone()));
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::SendRepost { target_event } => assert_eq!(target_event.id, ev.id),
            _ => panic!("expected SendRepost"),
        }
    }

    #[test]
    fn test_nostr_state_send_text_note() {
        let mut ns = NostrState;
        let cmds = ns.update(NostrMsg::SendTextNote("hello".into(), vec![]));
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::SendTextNote { content, tags } => {
                assert_eq!(content, "hello");
                assert!(tags.is_empty());
            }
            _ => panic!("expected SendTextNote"),
        }
    }
}
