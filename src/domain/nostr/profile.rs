use nostr_sdk::prelude::*;

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

    pub fn display_name(&self) -> Option<&String> {
        if let Some(name) = &self.metadata.display_name {
            if !name.is_empty() {
                return Some(name);
            }
        }

        None
    }

    pub fn handle(&self) -> Option<String> {
        if let Some(name) = &self.metadata.name {
            if !name.is_empty() {
                return Some(format!("@{name}"));
            }
        }

        None
    }

    pub fn name(&self) -> String {
        if let Some(name) = self.display_name() {
            name.clone()
        } else if let Some(handle) = self.handle() {
            handle
        } else {
            // TODO: Use shortened when fallback
            let Ok(npub) = self.pubkey.to_bech32();
            npub
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::*;
    use std::str::FromStr;

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

    #[rstest]
    #[case(Metadata::new(), None)]
    #[case(Metadata::new().name("foo"), None)]
    #[case(Metadata::new().display_name("foo"), Some(&String::from("foo")))]
    #[case(Metadata::new().display_name(""), None)]
    #[case(Metadata::new().display_name("").name(""), None)]
    #[case(Metadata::new().display_name("").name("hoge"), None)]
    fn test_display_name(
        #[case] metadata: Metadata,
        #[case] expected: Option<&String>,
    ) -> Result<()> {
        let key = nostr_sdk::PublicKey::from_str(
            "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
        )?;
        let profile = Profile::new(key, Timestamp::now(), metadata);

        assert_eq!(profile.display_name(), expected);

        Ok(())
    }

    #[rstest]
    #[case(Metadata::new(), None)]
    #[case(Metadata::new().name("foo"), Some("@foo".to_owned()))]
    #[case(Metadata::new().display_name("foo"), None)]
    #[case(Metadata::new().name(""), None)]
    #[case(Metadata::new().name("foo").display_name("foo"), Some("@foo".to_owned()))]
    fn test_name(#[case] metadata: Metadata, #[case] expected: Option<String>) -> Result<()> {
        let key = nostr_sdk::PublicKey::from_str(
            "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
        )?;
        let profile = Profile::new(key, Timestamp::now(), metadata);

        assert_eq!(profile.handle(), expected);

        Ok(())
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
