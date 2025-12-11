use nostr_sdk::prelude::*;
use std::collections::HashMap;

use crate::core::{cmd::Cmd, msg::user::UserMsg};
use crate::domain::nostr::Profile;

/// User-related state
#[derive(Debug, Clone)]
pub struct UserState {
    pub profiles: HashMap<PublicKey, Profile>,
    pub current_user_pubkey: PublicKey,
}

impl Default for UserState {
    fn default() -> Self {
        // Temporary implementation - actual initialization needs proper public key
        let dummy_keys = Keys::generate();
        Self {
            profiles: HashMap::new(),
            current_user_pubkey: dummy_keys.public_key(),
        }
    }
}

impl UserState {
    /// User-specific update function
    /// Returns: Generated commands
    pub fn update(&mut self, msg: UserMsg) -> Vec<Cmd> {
        match msg {
            UserMsg::UpdateProfile(pubkey, profile) => {
                // Only update if this is newer than existing profile
                let should_update = self.profiles.get(&pubkey).is_none_or(|existing| {
                    // Compare timestamps - Profile.created_at is always present
                    profile.created_at > existing.created_at
                });

                if should_update {
                    self.profiles.insert(pubkey, profile);
                }

                // UserState operations don't generate commands
                vec![]
            }
        }
    }

    /// Get profile for a given public key
    pub fn get_profile(&self, pubkey: &PublicKey) -> Option<&Profile> {
        self.profiles.get(pubkey)
    }

    /// Check if we have a profile for a given public key
    pub fn has_profile(&self, pubkey: &PublicKey) -> bool {
        self.profiles.contains_key(pubkey)
    }

    /// Get the total number of profiles stored
    pub fn profile_count(&self) -> usize {
        self.profiles.len()
    }

    /// Get all stored public keys
    pub fn all_pubkeys(&self) -> impl Iterator<Item = &PublicKey> {
        self.profiles.keys()
    }

    /// Clear all profiles
    pub fn clear_profiles(&mut self) {
        self.profiles.clear();
    }

    /// Remove a specific profile
    pub fn remove_profile(&mut self, pubkey: &PublicKey) -> Option<Profile> {
        self.profiles.remove(pubkey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_profile(name: &str, timestamp_offset: i64) -> Profile {
        let base_time = nostr_sdk::Timestamp::now().as_secs() as i64;
        let new_time = (base_time + timestamp_offset) as u64;

        Profile {
            pubkey: Keys::generate().public_key(),
            metadata: nostr_sdk::Metadata::new()
                .name(name)
                .display_name(format!("{} Display", name))
                .about(format!("Bio for {}", name))
                .picture(
                    format!("https://example.com/{}.jpg", name.to_lowercase())
                        .parse()
                        .unwrap(),
                ),
            created_at: nostr_sdk::Timestamp::from(new_time),
        }
    }

    // Profile update tests
    #[test]
    fn test_update_profile_new_unit() {
        let mut user = UserState::default();
        let pubkey = Keys::generate().public_key();
        let profile = create_test_profile("Alice", 0);

        assert_eq!(user.profile_count(), 0);

        let cmds = user.update(UserMsg::UpdateProfile(pubkey, profile.clone()));

        assert!(cmds.is_empty()); // UserState doesn't generate commands
        assert_eq!(user.profile_count(), 1);
        assert!(user.has_profile(&pubkey));
        assert_eq!(user.get_profile(&pubkey).unwrap().name(), profile.name());
    }

    #[test]
    fn test_update_profile_newer_unit() {
        let mut user = UserState::default();
        let pubkey = Keys::generate().public_key();

        // Add older profile
        let old_profile = create_test_profile("Alice Old", -100);
        user.update(UserMsg::UpdateProfile(pubkey, old_profile));

        // Add newer profile
        let new_profile = create_test_profile("Alice New", 100);
        let cmds = user.update(UserMsg::UpdateProfile(pubkey, new_profile.clone()));

        assert!(cmds.is_empty());
        assert_eq!(user.profile_count(), 1); // Still only one profile
        assert_eq!(
            user.get_profile(&pubkey).unwrap().name(),
            new_profile.name()
        );
    }

    #[test]
    fn test_update_profile_older_ignored_unit() {
        let mut user = UserState::default();
        let pubkey = Keys::generate().public_key();

        // Add newer profile first
        let new_profile = create_test_profile("Alice New", 100);
        user.update(UserMsg::UpdateProfile(pubkey, new_profile.clone()));

        // Try to add older profile - should be ignored
        let old_profile = create_test_profile("Alice Old", -100);
        let cmds = user.update(UserMsg::UpdateProfile(pubkey, old_profile));

        assert!(cmds.is_empty());
        assert_eq!(user.profile_count(), 1);
        assert_eq!(
            user.get_profile(&pubkey).unwrap().name(),
            new_profile.name()
        ); // Should keep newer
    }

    #[test]
    fn test_update_profile_same_timestamp_unit() {
        let mut user = UserState::default();
        let pubkey = Keys::generate().public_key();

        let profile1 = create_test_profile("Alice", 0);
        user.update(UserMsg::UpdateProfile(pubkey, profile1.clone()));
        assert_eq!(user.get_profile(&pubkey).unwrap().name(), profile1.name());

        let profile2 = create_test_profile("Alice Updated", 1); // Use newer timestamp
        user.update(UserMsg::UpdateProfile(pubkey, profile2.clone()));
        assert_eq!(user.get_profile(&pubkey).unwrap().name(), profile2.name()); // Should update with newer timestamp
    }

    // Helper method tests
    #[test]
    fn test_helper_methods_unit() {
        let mut user = UserState::default();
        let pubkey1 = Keys::generate().public_key();
        let pubkey2 = Keys::generate().public_key();
        let profile1 = create_test_profile("Alice", 0);
        let profile2 = create_test_profile("Bob", 0);

        // Initially empty
        assert_eq!(user.profile_count(), 0);
        assert!(!user.has_profile(&pubkey1));
        assert!(user.get_profile(&pubkey1).is_none());

        // Add profiles
        user.update(UserMsg::UpdateProfile(pubkey1, profile1.clone()));
        user.update(UserMsg::UpdateProfile(pubkey2, profile2.clone()));

        assert_eq!(user.profile_count(), 2);
        assert!(user.has_profile(&pubkey1));
        assert!(user.has_profile(&pubkey2));

        let all_keys: Vec<_> = user.all_pubkeys().collect();
        assert_eq!(all_keys.len(), 2);
        assert!(all_keys.contains(&&pubkey1));
        assert!(all_keys.contains(&&pubkey2));
    }

    #[test]
    fn test_remove_profile_unit() {
        let mut user = UserState::default();
        let pubkey = Keys::generate().public_key();
        let profile = create_test_profile("Alice", 0);

        user.update(UserMsg::UpdateProfile(pubkey, profile.clone()));
        assert!(user.has_profile(&pubkey));

        let removed = user.remove_profile(&pubkey);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().name(), profile.name());
        assert!(!user.has_profile(&pubkey));
        assert_eq!(user.profile_count(), 0);
    }

    #[test]
    fn test_clear_profiles_unit() {
        let mut user = UserState::default();

        // Add multiple profiles
        for i in 0..5 {
            let pubkey = Keys::generate().public_key();
            let profile = create_test_profile(&format!("User{}", i), i);
            user.update(UserMsg::UpdateProfile(pubkey, profile));
        }

        assert_eq!(user.profile_count(), 5);

        user.clear_profiles();
        assert_eq!(user.profile_count(), 0);
    }

    // Integration test
    #[test]
    fn test_user_complete_flow_unit() {
        let mut user = UserState::default();

        // 1. Add multiple users
        let alice_key = Keys::generate().public_key();
        let bob_key = Keys::generate().public_key();
        let charlie_key = Keys::generate().public_key();

        user.update(UserMsg::UpdateProfile(
            alice_key,
            create_test_profile("Alice", 100),
        ));
        user.update(UserMsg::UpdateProfile(
            bob_key,
            create_test_profile("Bob", 200),
        ));
        user.update(UserMsg::UpdateProfile(
            charlie_key,
            create_test_profile("Charlie", 300),
        ));

        assert_eq!(user.profile_count(), 3);

        // 2. Update existing user with newer profile
        user.update(UserMsg::UpdateProfile(
            alice_key,
            create_test_profile("Alice Updated", 400),
        ));
        assert_eq!(user.profile_count(), 3); // Same count
        assert_eq!(
            user.get_profile(&alice_key).unwrap().name(),
            "Alice Updated Display".to_string() // display_name takes precedence
        );

        // 3. Try to update with older profile (should be ignored)
        user.update(UserMsg::UpdateProfile(
            bob_key,
            create_test_profile("Bob Old", 50),
        ));
        assert_eq!(
            user.get_profile(&bob_key).unwrap().name(),
            "Bob Display".to_string() // display_name takes precedence, unchanged
        );

        // 4. Remove one user
        user.remove_profile(&charlie_key);
        assert_eq!(user.profile_count(), 2);
        assert!(!user.has_profile(&charlie_key));

        // 5. Check remaining users
        assert!(user.has_profile(&alice_key));
        assert!(user.has_profile(&bob_key));
    }
}
