use nostr_sdk::prelude::*;

/// Infrastructure-level Nostr operations.
/// Some names intentionally mirror domain-level NostrCmd (e.g. SendReaction, SendRepost).
/// This duplication reflects different concerns: domain NostrCmd expresses application intent (what),
/// while NostrOperation expresses concrete execution for the Nostr infrastructure (how).
/// Keeping these layers separate avoids leaking external SDK types into the domain and
/// improves testability/substitutability of the infrastructure.
#[derive(Debug, Clone)]
pub enum NostrOperation {
    /// Send a reaction (like/dislike) to a specific event
    SendReaction {
        target_event: Event,
        content: String,
    },

    /// Send a repost of a specific event
    SendRepost {
        target_event: Event,
        reason: Option<String>,
    },

    /// Send a text note
    SendTextNote { content: String, tags: Vec<Tag> },

    /// Connect to specific relays
    ConnectToRelays { relays: Vec<String> },

    /// Disconnect from all relays
    DisconnectFromRelays,

    /// Subscribe to timeline events
    SubscribeToTimeline,

    /// Update user profile metadata
    UpdateProfile { metadata: Metadata },

    /// Send encrypted direct message
    SendDirectMessage {
        recipient_pubkey: PublicKey,
        content: String,
    },
}

impl NostrOperation {
    /// Get a human-readable name for the operation (for logging/debugging)
    pub fn name(&self) -> &'static str {
        match self {
            NostrOperation::SendReaction { .. } => "SendReaction",
            NostrOperation::SendRepost { .. } => "SendRepost",
            NostrOperation::SendTextNote { .. } => "SendTextNote",
            NostrOperation::ConnectToRelays { .. } => "ConnectToRelays",
            NostrOperation::DisconnectFromRelays => "DisconnectFromRelays",
            NostrOperation::SubscribeToTimeline => "SubscribeToTimeline",
            NostrOperation::UpdateProfile { .. } => "UpdateProfile",
            NostrOperation::SendDirectMessage { .. } => "SendDirectMessage",
        }
    }

    /// Create a reaction operation
    pub fn reaction(target_event: Event, content: impl Into<String>) -> Self {
        Self::SendReaction {
            target_event,
            content: content.into(),
        }
    }

    /// Create a like operation (reaction with "+")
    pub fn like(target_event: Event) -> Self {
        Self::reaction(target_event, "+")
    }

    /// Create a dislike operation (reaction with "-")  
    pub fn dislike(target_event: Event) -> Self {
        Self::reaction(target_event, "-")
    }

    /// Create a repost operation
    pub fn repost(target_event: Event, reason: Option<String>) -> Self {
        Self::SendRepost {
            target_event,
            reason,
        }
    }

    /// Create a text note operation
    pub fn text_note(content: impl Into<String>, tags: Vec<Tag>) -> Self {
        Self::SendTextNote {
            content: content.into(),
            tags,
        }
    }

    /// Create a simple text note without tags
    pub fn simple_text_note(content: impl Into<String>) -> Self {
        Self::text_note(content, vec![])
    }

    /// Create a relay connection operation
    pub fn connect_relays(relays: Vec<String>) -> Self {
        Self::ConnectToRelays { relays }
    }
}

#[cfg(test)]
mod tests {
    use color_eyre::Result;

    use super::*;

    fn create_test_event() -> Result<Event> {
        let keys = Keys::generate();
        EventBuilder::text_note("test")
            .sign_with_keys(&keys)
            .map_err(|e| e.into())
    }

    #[test]
    fn test_nostr_operation_names() -> Result<()> {
        let event = create_test_event()?;

        assert_eq!(NostrOperation::like(event.clone()).name(), "SendReaction");
        assert_eq!(NostrOperation::repost(event, None).name(), "SendRepost");
        assert_eq!(
            NostrOperation::simple_text_note("test").name(),
            "SendTextNote"
        );
        assert_eq!(
            NostrOperation::connect_relays(vec!["wss://relay.example.com".to_string()]).name(),
            "ConnectToRelays"
        );
        assert_eq!(
            NostrOperation::DisconnectFromRelays.name(),
            "DisconnectFromRelays"
        );
        assert_eq!(
            NostrOperation::SubscribeToTimeline.name(),
            "SubscribeToTimeline"
        );

        Ok(())
    }

    #[test]
    fn test_reaction_helpers() -> Result<()> {
        let event = create_test_event()?;

        let like = NostrOperation::like(event.clone());
        assert!(matches!(
            like,
            NostrOperation::SendReaction {
                target_event,
                content,
            } if target_event.id == event.id && content == "+"
        ));

        let dislike = NostrOperation::dislike(event.clone());
        assert!(matches!(
            dislike,
            NostrOperation::SendReaction {
                target_event,
                content,
            } if target_event.id == event.id && content == "-"
        ));

        Ok(())
    }

    #[test]
    fn test_text_note_helpers() -> Result<()> {
        let simple = NostrOperation::simple_text_note("Hello, Nostr!");
        assert!(matches!(
            simple,
            NostrOperation::SendTextNote { content, tags } if content == "Hello, Nostr!" && tags.is_empty()
        ));

        let with_tags = NostrOperation::text_note("Tagged note", vec![Tag::parse(["t", "test"])?]);
        assert!(matches!(
            with_tags,
            NostrOperation::SendTextNote { content, tags } if content == "Tagged note" && tags.len() == 1
        ));

        Ok(())
    }

    #[test]
    fn test_repost_operation() -> Result<()> {
        let event = create_test_event()?;

        let simple_repost = NostrOperation::repost(event.clone(), None);
        assert!(matches!(
            simple_repost,
            NostrOperation::SendRepost {
                target_event,
                reason,
            } if target_event.id == event.id && reason.is_none()
        ));

        let repost_with_reason =
            NostrOperation::repost(event.clone(), Some("Great point!".to_string()));
        assert!(matches!(
            repost_with_reason,
            NostrOperation::SendRepost {
                target_event,
                reason,
            } if target_event.id == event.id && reason == Some("Great point!".to_string())
        ));

        Ok(())
    }

    #[test]
    fn test_connect_relays_operation() {
        let relays = vec![
            "wss://relay1.example.com".to_string(),
            "wss://relay2.example.com".to_string(),
        ];

        let cmd = NostrOperation::connect_relays(relays.clone());
        assert!(matches!(
            cmd,
            NostrOperation::ConnectToRelays { relays: cmd_relays } if cmd_relays == relays
        ));
    }
}
