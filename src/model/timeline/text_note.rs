use chrono::{DateTime, Local};
use nostr_sdk::prelude::*;

use crate::domain::collections::EventSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    ReactionReceived(Event),
    RepostReceived(Event),
    ZapReceiptReceived(Event),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextNote {
    event: Event,
    reactions: EventSet,
    reposts: EventSet,
    zap_receipts: EventSet,
}

impl TextNote {
    pub fn new(event: Event) -> Self {
        Self {
            event,
            reactions: EventSet::new(),
            reposts: EventSet::new(),
            zap_receipts: EventSet::new(),
        }
    }

    pub fn id(&self) -> EventId {
        self.event.id
    }

    pub fn bech32_id(&self) -> String {
        let Ok(note1) = self.event.id.to_bech32();
        note1
    }

    pub fn as_event(&self) -> &Event {
        &self.event
    }

    pub fn author_pubkey(&self) -> PublicKey {
        self.event.pubkey
    }

    pub fn content(&self) -> &String {
        &self.event.content
    }

    pub fn created_at(&self) -> String {
        DateTime::from_timestamp(self.event.created_at.as_secs() as i64, 0)
            .expect("Invalid created_at")
            .with_timezone(&Local)
            .format("%T")
            .to_string()
    }

    pub fn reactions_count(&self) -> usize {
        self.reactions.len()
    }

    pub fn reposts_count(&self) -> usize {
        self.reposts.len()
    }

    fn find_amount(&self, ev: &Event) -> Option<TagStandard> {
        ev.tags.filter_standardized(TagKind::Amount).last().cloned()
    }

    pub fn zap_amount(&self) -> u64 {
        self.zap_receipts.iter().fold(0, |acc, ev| {
            if let Some(TagStandard::Amount { millisats, .. }) = self.find_amount(ev) {
                acc + millisats
            } else {
                acc
            }
        })
    }

    pub fn find_reply_tag(&self) -> Option<&TagStandard> {
        self.event
            .tags
            .filter_standardized(TagKind::SingleLetter(SingleLetterTag::lowercase(
                Alphabet::E,
            )))
            .last()
    }

    pub fn find_client_tag(&self) -> Option<&TagStandard> {
        self.event.tags.find_standardized(TagKind::Client)
    }

    pub fn mentioned_pubkeys(&self) -> impl Iterator<Item = &PublicKey> {
        self.event.tags.public_keys()
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::ReactionReceived(event) => {
                self.reactions.push(event);
            }
            Message::RepostReceived(event) => {
                self.reposts.push(event);
            }
            Message::ZapReceiptReceived(event) => {
                self.zap_receipts.push(event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::nostr::Kind;
    use std::error::Error;

    fn create_test_event(content: &str) -> Result<Event, Box<dyn Error>> {
        let keys = Keys::generate();
        Ok(EventBuilder::text_note(content).sign_with_keys(&keys)?)
    }

    fn create_test_event_with_tags(
        content: &str,
        kind: Kind,
        tags: Vec<Tag>,
    ) -> Result<Event, Box<dyn Error>> {
        let keys = Keys::generate();
        let builder = EventBuilder::new(kind, content).tags(tags);
        Ok(builder.sign_with_keys(&keys)?)
    }

    fn create_zap_receipt_event(
        target_event_id: EventId,
        millisats: u64,
    ) -> Result<Event, Box<dyn Error>> {
        let keys = Keys::generate();
        let amount_tag = Tag::from_standardized(TagStandard::Amount {
            millisats,
            bolt11: None,
        });
        let event_tag = Tag::event(target_event_id);

        Ok(EventBuilder::new(Kind::ZapReceipt, "")
            .tags(vec![amount_tag, event_tag])
            .sign_with_keys(&keys)?)
    }

    #[test]
    fn test_new_text_note() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("Hello, Nostr!")?;
        let text_note = TextNote::new(event.clone());

        assert_eq!(text_note.id(), event.id);
        assert_eq!(text_note.author_pubkey(), event.pubkey);
        assert_eq!(text_note.content(), &event.content);
        assert_eq!(text_note.reactions_count(), 0);
        assert_eq!(text_note.reposts_count(), 0);
        assert_eq!(text_note.zap_amount(), 0);

        Ok(())
    }

    #[test]
    fn test_bech32_id() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("Test")?;
        let text_note = TextNote::new(event);

        let bech32 = text_note.bech32_id();
        assert!(bech32.starts_with("note1"));

        Ok(())
    }

    #[test]
    fn test_created_at_format() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("Test")?;
        let text_note = TextNote::new(event);

        let created_at = text_note.created_at();
        // Format should be HH:MM:SS
        assert!(created_at.contains(':'));
        assert_eq!(created_at.len(), 8);

        Ok(())
    }

    #[test]
    fn test_update_with_reaction() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("Original post")?;
        let mut text_note = TextNote::new(event.clone());

        let reaction =
            create_test_event_with_tags("+", Kind::Reaction, vec![Tag::event(event.id)])?;

        text_note.update(Message::ReactionReceived(reaction));

        assert_eq!(text_note.reactions_count(), 1);
        assert_eq!(text_note.reposts_count(), 0);

        Ok(())
    }

    #[test]
    fn test_update_with_multiple_reactions() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("Original post")?;
        let mut text_note = TextNote::new(event.clone());

        for _ in 0..3 {
            let reaction =
                create_test_event_with_tags("+", Kind::Reaction, vec![Tag::event(event.id)])?;
            text_note.update(Message::ReactionReceived(reaction));
        }

        assert_eq!(text_note.reactions_count(), 3);

        Ok(())
    }

    #[test]
    fn test_update_with_repost() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("Original post")?;
        let mut text_note = TextNote::new(event.clone());

        let repost = create_test_event_with_tags("", Kind::Repost, vec![Tag::event(event.id)])?;

        text_note.update(Message::RepostReceived(repost));

        assert_eq!(text_note.reactions_count(), 0);
        assert_eq!(text_note.reposts_count(), 1);

        Ok(())
    }

    #[test]
    fn test_update_with_zap_receipt() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("Original post")?;
        let mut text_note = TextNote::new(event.clone());

        let zap_receipt = create_zap_receipt_event(event.id, 10000)?;
        text_note.update(Message::ZapReceiptReceived(zap_receipt));

        assert_eq!(text_note.zap_amount(), 10000);

        Ok(())
    }

    #[test]
    fn test_zap_amount_with_multiple_receipts() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("Original post")?;
        let mut text_note = TextNote::new(event.clone());

        let amounts = vec![1000, 5000, 3000];
        for amount in &amounts {
            let zap_receipt = create_zap_receipt_event(event.id, *amount)?;
            text_note.update(Message::ZapReceiptReceived(zap_receipt));
        }

        let expected_total: u64 = amounts.iter().sum();
        assert_eq!(text_note.zap_amount(), expected_total);

        Ok(())
    }

    #[test]
    fn test_find_reply_tag() -> Result<(), Box<dyn Error>> {
        let original_event = create_test_event("Original")?;
        let reply_event = create_test_event_with_tags(
            "Reply",
            Kind::TextNote,
            vec![Tag::event(original_event.id)],
        )?;

        let text_note = TextNote::new(reply_event);
        let reply_tag = text_note.find_reply_tag();

        assert!(reply_tag.is_some());
        if let Some(TagStandard::Event { event_id, .. }) = reply_tag {
            assert_eq!(*event_id, original_event.id);
        } else {
            panic!("Expected Event tag");
        }

        Ok(())
    }

    #[test]
    fn test_find_reply_tag_none() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("Not a reply")?;
        let text_note = TextNote::new(event);

        assert_eq!(text_note.find_reply_tag(), None);

        Ok(())
    }

    #[test]
    fn test_find_client_tag() -> Result<(), Box<dyn Error>> {
        let client_tag = Tag::custom(TagKind::Client, vec!["TestClient", "https://test.com"]);
        let event = create_test_event_with_tags("Hello", Kind::TextNote, vec![client_tag])?;
        let text_note = TextNote::new(event);

        let found_client = text_note.find_client_tag();
        assert!(found_client.is_some());

        Ok(())
    }

    #[test]
    fn test_mentioned_pubkeys() -> Result<(), Box<dyn Error>> {
        let mentioned_keys = Keys::generate();
        let p_tag = Tag::public_key(mentioned_keys.public_key());

        let event = create_test_event_with_tags("Mentioning someone", Kind::TextNote, vec![p_tag])?;

        let text_note = TextNote::new(event);
        let pubkeys: Vec<_> = text_note.mentioned_pubkeys().collect();

        assert_eq!(pubkeys.len(), 1);
        assert_eq!(*pubkeys[0], mentioned_keys.public_key());

        Ok(())
    }

    #[test]
    fn test_combined_updates() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("Popular post")?;
        let mut text_note = TextNote::new(event.clone());

        // Add reactions
        for _ in 0..5 {
            let reaction =
                create_test_event_with_tags("+", Kind::Reaction, vec![Tag::event(event.id)])?;
            text_note.update(Message::ReactionReceived(reaction));
        }

        // Add reposts
        for _ in 0..3 {
            let repost = create_test_event_with_tags("", Kind::Repost, vec![Tag::event(event.id)])?;
            text_note.update(Message::RepostReceived(repost));
        }

        // Add zaps
        for amount in [1000, 2000, 3000] {
            let zap = create_zap_receipt_event(event.id, amount)?;
            text_note.update(Message::ZapReceiptReceived(zap));
        }

        assert_eq!(text_note.reactions_count(), 5);
        assert_eq!(text_note.reposts_count(), 3);
        assert_eq!(text_note.zap_amount(), 6000);

        Ok(())
    }

    #[test]
    fn test_as_event() -> Result<(), Box<dyn Error>> {
        let event = create_test_event("Test")?;
        let text_note = TextNote::new(event.clone());

        let retrieved_event = text_note.as_event();
        assert_eq!(retrieved_event.id, event.id);
        assert_eq!(retrieved_event.content, event.content);

        Ok(())
    }
}
