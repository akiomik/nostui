use nostr_sdk::prelude::*;

use crate::domain::text::shorten_hex;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Profile {
    pub pubkey: PublicKey,
    pub created_at: Timestamp,
    pub metadata: Metadata,
}

impl Profile {
    pub fn new(pubkey: PublicKey, created_at: Timestamp, metadata: Metadata) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_profile_new() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let created_at = Timestamp::now();
        let metadata = Metadata::new();

        let profile = Profile::new(pubkey, created_at, metadata.clone());

        assert_eq!(profile.pubkey, pubkey);
        assert_eq!(profile.created_at, created_at);
        assert_eq!(profile.metadata, metadata);
    }

    #[test]
    fn test_name_returns_display_name_when_set() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let created_at = Timestamp::now();
        let mut metadata = Metadata::new();
        metadata.display_name = Some(String::from("Alice"));
        metadata.name = Some(String::from("alice123")); // name is also set but display_name takes precedence

        let profile = Profile::new(pubkey, created_at, metadata);

        assert_eq!(profile.name(), "Alice");
    }

    #[test]
    fn test_name_returns_name_with_at_when_display_name_empty() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let created_at = Timestamp::now();
        let mut metadata = Metadata::new();
        metadata.display_name = Some(String::from("")); // Empty display_name
        metadata.name = Some(String::from("bob456"));

        let profile = Profile::new(pubkey, created_at, metadata);

        assert_eq!(profile.name(), "@bob456");
    }

    #[test]
    fn test_name_returns_name_with_at_when_display_name_none() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let created_at = Timestamp::now();
        let mut metadata = Metadata::new();
        metadata.display_name = None;
        metadata.name = Some(String::from("charlie"));

        let profile = Profile::new(pubkey, created_at, metadata);

        assert_eq!(profile.name(), "@charlie");
    }

    #[test]
    fn test_name_returns_npub_when_both_names_empty() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let created_at = Timestamp::now();
        let mut metadata = Metadata::new();
        metadata.display_name = Some(String::from(""));
        metadata.name = Some(String::from(""));

        let profile = Profile::new(pubkey, created_at, metadata);

        let name = profile.name();
        assert!(
            name.starts_with("npub1"),
            "Expected npub format, got: {name}"
        );
    }

    #[test]
    fn test_name_returns_npub_when_both_names_none() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let created_at = Timestamp::now();
        let metadata = Metadata::new(); // Both display_name and name are None

        let profile = Profile::new(pubkey, created_at, metadata);

        let name = profile.name();
        assert!(
            name.starts_with("npub1"),
            "Expected npub format, got: {name}"
        );
    }

    #[test]
    fn test_name_empty_string_display_name_is_skipped() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let created_at = Timestamp::now();
        let mut metadata = Metadata::new();
        metadata.display_name = Some(String::from("   ")); // Whitespace only
        metadata.name = None;

        let profile = Profile::new(pubkey, created_at, metadata);

        let name = profile.name();
        // Whitespace-only display_name is not considered empty by is_empty(), so it should be returned
        assert_eq!(name, "   ");
    }

    #[test]
    fn test_profile_clone() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let created_at = Timestamp::now();
        let mut metadata = Metadata::new();
        metadata.display_name = Some(String::from("Test User"));

        let profile = Profile::new(pubkey, created_at, metadata);
        let cloned = profile.clone();

        assert_eq!(profile, cloned);
        assert_eq!(profile.name(), cloned.name());
    }

    #[test]
    fn test_profile_serialization() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let created_at = Timestamp::now();
        let mut metadata = Metadata::new();
        metadata.display_name = Some(String::from("Serialization Test"));

        let profile = Profile::new(pubkey, created_at, metadata);

        // Test serialization and deserialization
        let serialized = serde_json::to_string(&profile).expect("Failed to serialize");
        let deserialized: Profile =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(profile, deserialized);
    }
}
