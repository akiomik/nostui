use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::domain::nostr::Profile;

/// Messages specific to UserState
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UserMsg {
    /// Update user profile information
    UpdateProfile(PublicKey, Profile),
}

impl UserMsg {
    /// Determine if this is a frequent message during debugging
    pub fn is_frequent(&self) -> bool {
        // User messages are generally not frequent
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_profile() -> Result<Profile> {
        Ok(Profile {
            pubkey: Keys::generate().public_key(),
            metadata: nostr_sdk::Metadata::new()
                .name("Test User")
                .display_name("Test Display")
                .about("Test bio")
                .picture("https://example.com/avatar.jpg".parse()?),
            created_at: nostr_sdk::Timestamp::now(),
        })
    }

    #[test]
    fn test_user_msg_frequent_detection() -> Result<()> {
        let pubkey = Keys::generate().public_key();
        let profile = create_test_profile()?;

        assert!(!UserMsg::UpdateProfile(pubkey, profile).is_frequent());

        Ok(())
    }

    #[test]
    fn test_user_msg_equality() -> Result<()> {
        let pubkey = Keys::generate().public_key();
        let profile1 = create_test_profile()?;
        let profile2 = create_test_profile()?;

        let msg1 = UserMsg::UpdateProfile(pubkey, profile1.clone());
        let msg2 = UserMsg::UpdateProfile(pubkey, profile1);
        let msg3 = UserMsg::UpdateProfile(pubkey, profile2);

        assert_eq!(msg1, msg2);
        assert_ne!(msg1, msg3); // Different profile content

        Ok(())
    }

    #[test]
    fn test_user_msg_serialization() -> Result<()> {
        let pubkey = Keys::generate().public_key();
        let profile = create_test_profile()?;
        let msg = UserMsg::UpdateProfile(pubkey, profile);

        let serialized = serde_json::to_string(&msg)?;
        let deserialized: UserMsg = serde_json::from_str(&serialized)?;
        assert_eq!(msg, deserialized);

        Ok(())
    }

    #[test]
    fn test_profile_fields() -> Result<()> {
        let profile = create_test_profile()?;

        assert_eq!(profile.name(), "Test Display".to_string()); // display_name takes precedence
        assert_eq!(
            profile.metadata.display_name,
            Some("Test Display".to_string())
        );
        assert_eq!(profile.metadata.about, Some("Test bio".to_string()));
        assert!(profile.metadata.picture.is_some());

        Ok(())
    }
}
