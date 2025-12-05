use nostr_sdk::prelude::*;

/// Commands for NostrService - rich data approach
/// Contains complete Event objects to support nostr-sdk API requirements
#[derive(Debug, Clone)]
pub enum NostrCommand {
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

impl NostrCommand {
    /// Get a human-readable name for the command (for logging/debugging)
    pub fn name(&self) -> &'static str {
        match self {
            NostrCommand::SendReaction { .. } => "SendReaction",
            NostrCommand::SendRepost { .. } => "SendRepost",
            NostrCommand::SendTextNote { .. } => "SendTextNote",
            NostrCommand::ConnectToRelays { .. } => "ConnectToRelays",
            NostrCommand::DisconnectFromRelays => "DisconnectFromRelays",
            NostrCommand::SubscribeToTimeline => "SubscribeToTimeline",
            NostrCommand::UpdateProfile { .. } => "UpdateProfile",
            NostrCommand::SendDirectMessage { .. } => "SendDirectMessage",
        }
    }

    /// Create a reaction command
    pub fn reaction(target_event: Event, content: impl Into<String>) -> Self {
        Self::SendReaction {
            target_event,
            content: content.into(),
        }
    }

    /// Create a like command (reaction with "+")
    pub fn like(target_event: Event) -> Self {
        Self::reaction(target_event, "+")
    }

    /// Create a dislike command (reaction with "-")  
    pub fn dislike(target_event: Event) -> Self {
        Self::reaction(target_event, "-")
    }

    /// Create a repost command
    pub fn repost(target_event: Event, reason: Option<String>) -> Self {
        Self::SendRepost {
            target_event,
            reason,
        }
    }

    /// Create a text note command
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

    /// Create a relay connection command
    pub fn connect_relays(relays: Vec<String>) -> Self {
        Self::ConnectToRelays { relays }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_event() -> Event {
        let keys = Keys::generate();
        EventBuilder::text_note("test")
            .sign_with_keys(&keys)
            .unwrap()
    }

    #[test]
    fn test_nostr_command_names() {
        let event = create_test_event();

        assert_eq!(NostrCommand::like(event.clone()).name(), "SendReaction");
        assert_eq!(NostrCommand::repost(event, None).name(), "SendRepost");
        assert_eq!(
            NostrCommand::simple_text_note("test").name(),
            "SendTextNote"
        );
        assert_eq!(
            NostrCommand::connect_relays(vec!["wss://relay.example.com".to_string()]).name(),
            "ConnectToRelays"
        );
        assert_eq!(
            NostrCommand::DisconnectFromRelays.name(),
            "DisconnectFromRelays"
        );
        assert_eq!(
            NostrCommand::SubscribeToTimeline.name(),
            "SubscribeToTimeline"
        );
    }

    #[test]
    fn test_reaction_helpers() {
        let event = create_test_event();

        let like = NostrCommand::like(event.clone());
        match like {
            NostrCommand::SendReaction {
                target_event,
                content,
            } => {
                assert_eq!(target_event.id, event.id);
                assert_eq!(content, "+");
            }
            _ => panic!("Expected SendReaction"),
        }

        let dislike = NostrCommand::dislike(event.clone());
        match dislike {
            NostrCommand::SendReaction {
                target_event,
                content,
            } => {
                assert_eq!(target_event.id, event.id);
                assert_eq!(content, "-");
            }
            _ => panic!("Expected SendReaction"),
        }
    }

    #[test]
    fn test_text_note_helpers() {
        let simple = NostrCommand::simple_text_note("Hello, Nostr!");
        match simple {
            NostrCommand::SendTextNote { content, tags } => {
                assert_eq!(content, "Hello, Nostr!");
                assert!(tags.is_empty());
            }
            _ => panic!("Expected SendTextNote"),
        }

        let with_tags =
            NostrCommand::text_note("Tagged note", vec![Tag::parse(["t", "test"]).unwrap()]);
        match with_tags {
            NostrCommand::SendTextNote { content, tags } => {
                assert_eq!(content, "Tagged note");
                assert_eq!(tags.len(), 1);
            }
            _ => panic!("Expected SendTextNote"),
        }
    }

    #[test]
    fn test_repost_command() {
        let event = create_test_event();

        let simple_repost = NostrCommand::repost(event.clone(), None);
        match simple_repost {
            NostrCommand::SendRepost {
                target_event,
                reason,
            } => {
                assert_eq!(target_event.id, event.id);
                assert!(reason.is_none());
            }
            _ => panic!("Expected SendRepost"),
        }

        let repost_with_reason =
            NostrCommand::repost(event.clone(), Some("Great point!".to_string()));
        match repost_with_reason {
            NostrCommand::SendRepost {
                target_event,
                reason,
            } => {
                assert_eq!(target_event.id, event.id);
                assert_eq!(reason, Some("Great point!".to_string()));
            }
            _ => panic!("Expected SendRepost"),
        }
    }

    #[test]
    fn test_connect_relays_command() {
        let relays = vec![
            "wss://relay1.example.com".to_string(),
            "wss://relay2.example.com".to_string(),
        ];

        let cmd = NostrCommand::connect_relays(relays.clone());
        match cmd {
            NostrCommand::ConnectToRelays { relays: cmd_relays } => {
                assert_eq!(cmd_relays, relays);
            }
            _ => panic!("Expected ConnectToRelays"),
        }
    }
}
