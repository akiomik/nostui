use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use super::NostrAction;
use crate::action::Action;
use crate::repositories::EventRepository;

pub struct NostrActionHandler {
    repo: EventRepository,
    action_tx: UnboundedSender<Action>,
}

impl NostrActionHandler {
    pub fn new(repo: EventRepository, action_tx: UnboundedSender<Action>) -> Self {
        Self { repo, action_tx }
    }

    pub fn handle(&self, action: NostrAction) -> Result<()> {
        let message = match action {
            NostrAction::SendTextNote(content, tags) => {
                let ev = self.repo.send_text_note(&content, tags.clone())?;
                log::info!("Send text note: {ev:?}");
                Action::SystemMessage(format!("[Posted] {content}"))
            }
            NostrAction::SendReaction(id, pubkey) => {
                let ev = self.repo.send_reaction(id, pubkey)?;
                log::info!("Send reaction: {ev:?}");
                let note1 = id.to_bech32()?;
                Action::SystemMessage(format!("[Liked] {note1}"))
            }
            NostrAction::SendRepost(id, pubkey) => {
                let ev = self.repo.send_repost(id, pubkey)?;
                log::info!("Send repost: {ev:?}");
                let note1 = id.to_bech32()?;
                Action::SystemMessage(format!("[Reposted] {note1}"))
            }
        };
        self.action_tx.send(message)?;

        Ok(())
    }
}
