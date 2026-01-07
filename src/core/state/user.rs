use nostr_sdk::prelude::*;
use std::collections::HashMap;

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
