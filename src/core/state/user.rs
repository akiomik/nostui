use nostr_sdk::prelude::*;
use std::collections::HashMap;

use crate::domain::nostr::Profile;

/// User-related state
#[derive(Debug, Clone)]
pub struct UserState {
    /// Cached profile metadata for public keys
    profiles: HashMap<PublicKey, Profile>,
    /// Current user's public key
    current_user_pubkey: PublicKey,
}

impl Default for UserState {
    /// Creates a new UserState with a randomly generated public key
    fn default() -> Self {
        // TODO: Actual initialization needs proper public key
        let dummy_keys = Keys::generate();
        Self {
            profiles: HashMap::new(),
            current_user_pubkey: dummy_keys.public_key(),
        }
    }
}

impl UserState {
    /// Creates a new UserState with default values
    pub fn new() -> Self {
        Default::default()
    }

    /// Creates a new UserState with a specified public key
    pub fn new_with_pubkey(pubkey: PublicKey) -> Self {
        Self {
            current_user_pubkey: pubkey,
            ..Default::default()
        }
    }

    /// Returns the current user's public key
    pub fn current_user_pubkey(&self) -> PublicKey {
        self.current_user_pubkey
    }

    /// Get profile for a given public key
    pub fn get_profile(&self, pubkey: &PublicKey) -> Option<&Profile> {
        self.profiles.get(pubkey)
    }

    /// Get profile for a public key of current user
    pub fn current_user(&self) -> Option<&Profile> {
        self.profiles.get(&self.current_user_pubkey)
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

    /// Adds or updates a profile if it's newer than the existing one, returning true if updated
    pub fn insert_newer_profile(&mut self, profile: Profile) -> bool {
        let should_update = self
            .get_profile(&profile.pubkey)
            .is_none_or(|existing| profile.created_at > existing.created_at);

        if should_update {
            self.profiles.insert(profile.pubkey, profile);
            true
        } else {
            false
        }
    }

    /// Remove a specific profile
    pub fn remove_profile(&mut self, pubkey: &PublicKey) -> Option<Profile> {
        self.profiles.remove(pubkey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_profile(pubkey: PublicKey, created_at: Timestamp) -> Profile {
        Profile::new(pubkey, created_at, Metadata::default())
    }

    #[test]
    fn test_new() {
        let state = UserState::new();
        assert_eq!(state.profile_count(), 0);
    }

    #[test]
    fn test_new_with_pubkey() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let state = UserState::new_with_pubkey(pubkey);
        assert_eq!(state.current_user_pubkey(), pubkey);
        assert_eq!(state.profile_count(), 0);
    }

    #[test]
    fn test_current_user_pubkey() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let state = UserState::new_with_pubkey(pubkey);
        assert_eq!(state.current_user_pubkey(), pubkey);
    }

    #[test]
    fn test_insert_newer_profile() {
        let mut state = UserState::new();
        let keys = Keys::generate();
        let pubkey = keys.public_key();

        let profile1 = create_test_profile(pubkey, Timestamp::from(100));
        assert!(state.insert_newer_profile(profile1.clone()));
        assert_eq!(state.profile_count(), 1);
        assert_eq!(state.get_profile(&pubkey), Some(&profile1));

        // Insert newer profile
        let profile2 = create_test_profile(pubkey, Timestamp::from(200));
        assert!(state.insert_newer_profile(profile2.clone()));
        assert_eq!(state.profile_count(), 1);
        assert_eq!(state.get_profile(&pubkey), Some(&profile2));

        // Try to insert older profile
        let profile3 = create_test_profile(pubkey, Timestamp::from(150));
        assert!(!state.insert_newer_profile(profile3));
        assert_eq!(state.get_profile(&pubkey), Some(&profile2));
    }

    #[test]
    fn test_get_profile() {
        let mut state = UserState::new();
        let keys = Keys::generate();
        let pubkey = keys.public_key();

        assert_eq!(state.get_profile(&pubkey), None);

        let profile = create_test_profile(pubkey, Timestamp::now());
        state.insert_newer_profile(profile.clone());
        assert_eq!(state.get_profile(&pubkey), Some(&profile));
    }

    #[test]
    fn test_current_user() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let mut state = UserState::new_with_pubkey(pubkey);

        assert_eq!(state.current_user(), None);

        let profile = create_test_profile(pubkey, Timestamp::now());
        state.insert_newer_profile(profile.clone());
        assert_eq!(state.current_user(), Some(&profile));
    }

    #[test]
    fn test_has_profile() {
        let mut state = UserState::new();
        let keys = Keys::generate();
        let pubkey = keys.public_key();

        assert!(!state.has_profile(&pubkey));

        let profile = create_test_profile(pubkey, Timestamp::now());
        state.insert_newer_profile(profile);
        assert!(state.has_profile(&pubkey));
    }

    #[test]
    fn test_profile_count() {
        let mut state = UserState::new();
        assert_eq!(state.profile_count(), 0);

        let keys1 = Keys::generate();
        let profile1 = create_test_profile(keys1.public_key(), Timestamp::now());
        state.insert_newer_profile(profile1);
        assert_eq!(state.profile_count(), 1);

        let keys2 = Keys::generate();
        let profile2 = create_test_profile(keys2.public_key(), Timestamp::now());
        state.insert_newer_profile(profile2);
        assert_eq!(state.profile_count(), 2);
    }

    #[test]
    fn test_all_pubkeys() {
        let mut state = UserState::new();
        let keys1 = Keys::generate();
        let keys2 = Keys::generate();
        let pubkey1 = keys1.public_key();
        let pubkey2 = keys2.public_key();

        let profile1 = create_test_profile(pubkey1, Timestamp::now());
        let profile2 = create_test_profile(pubkey2, Timestamp::now());
        state.insert_newer_profile(profile1);
        state.insert_newer_profile(profile2);

        let pubkeys: Vec<&PublicKey> = state.all_pubkeys().collect();
        assert_eq!(pubkeys.len(), 2);
        assert!(pubkeys.contains(&&pubkey1));
        assert!(pubkeys.contains(&&pubkey2));
    }

    #[test]
    fn test_clear_profiles() {
        let mut state = UserState::new();
        let keys = Keys::generate();
        let profile = create_test_profile(keys.public_key(), Timestamp::now());
        state.insert_newer_profile(profile);
        assert_eq!(state.profile_count(), 1);

        state.clear_profiles();
        assert_eq!(state.profile_count(), 0);
    }

    #[test]
    fn test_remove_profile() {
        let mut state = UserState::new();
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let profile = create_test_profile(pubkey, Timestamp::now());

        state.insert_newer_profile(profile.clone());
        assert_eq!(state.profile_count(), 1);

        let removed = state.remove_profile(&pubkey);
        assert_eq!(removed, Some(profile));
        assert_eq!(state.profile_count(), 0);

        let removed_again = state.remove_profile(&pubkey);
        assert_eq!(removed_again, None);
    }

    #[test]
    fn test_multiple_profiles() {
        let mut state = UserState::new();
        let keys1 = Keys::generate();
        let keys2 = Keys::generate();
        let keys3 = Keys::generate();

        let profile1 = create_test_profile(keys1.public_key(), Timestamp::from(100));
        let profile2 = create_test_profile(keys2.public_key(), Timestamp::from(200));
        let profile3 = create_test_profile(keys3.public_key(), Timestamp::from(300));

        state.insert_newer_profile(profile1.clone());
        state.insert_newer_profile(profile2.clone());
        state.insert_newer_profile(profile3.clone());

        assert_eq!(state.profile_count(), 3);
        assert_eq!(state.get_profile(&keys1.public_key()), Some(&profile1));
        assert_eq!(state.get_profile(&keys2.public_key()), Some(&profile2));
        assert_eq!(state.get_profile(&keys3.public_key()), Some(&profile3));
    }
}
