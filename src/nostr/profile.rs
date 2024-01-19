use nostr_sdk::prelude::*;

use crate::text::shorten_hex;

#[derive(Clone, Debug)]
pub struct Profile {
    pub pubkey: XOnlyPublicKey,
    pub created_at: Timestamp,
    pub metadata: Metadata,
}

impl Profile {
    pub fn new(pubkey: XOnlyPublicKey, created_at: Timestamp, metadata: Metadata) -> Self {
        Self {
            pubkey,
            created_at,
            metadata,
        }
    }

    pub fn name(&self) -> String {
        match (
            self.metadata.display_name.clone(),
            self.metadata.name.clone(),
            self.pubkey.to_bech32(),
        ) {
            (Some(display_name), _, _) if !display_name.is_empty() => display_name,
            (_, Some(name), _) if !name.is_empty() => format!("@{name}"),
            (_, _, Ok(npub)) => npub,
            _ => shorten_hex(&self.pubkey.to_string()),
        }
    }
}
