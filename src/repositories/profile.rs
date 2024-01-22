use std::{collections::HashMap, sync::Arc};

use nostr_sdk::prelude::*;
use tokio::sync::Mutex;

use crate::nostr::Profile;

#[derive(Clone)]
pub struct ProfileRepository {
    cache: Arc<Mutex<HashMap<XOnlyPublicKey, Profile>>>,
}

impl ProfileRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn find(&self, pubkey: &XOnlyPublicKey) -> Option<Profile> {
        let cache = self.cache.lock().await;
        cache.get(pubkey).cloned()
    }

    pub async fn update(&self, profile: Profile) {
        self.updated(profile).await;
    }

    pub async fn updated(&self, profile: Profile) -> Profile {
        if let Some(existing_profile) = self.find(&profile.pubkey).await {
            if existing_profile.created_at >= profile.created_at {
                return existing_profile;
            }
        }

        let mut cache = self.cache.lock().await;
        cache.insert(profile.pubkey, profile.clone());
        profile
    }
}

impl Default for ProfileRepository {
    fn default() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, time::Duration};

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::nostr::Profile;

    #[tokio::test]
    async fn test_profile_repository_updated_not_exist() {
        let pubkey = XOnlyPublicKey::from_str(
            "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
        )
        .unwrap();
        let metadata = Metadata::from_json(r#"{"name": "foobar"}"#).unwrap();
        let repo = ProfileRepository::new();
        let created_at = Timestamp::now();
        let profile = Profile::new(pubkey, created_at, metadata);
        let actual = repo.updated(profile.clone()).await;
        assert_eq!(actual, profile);

        let cache = repo.cache.lock().await;
        assert_eq!(
            cache.keys().cloned().collect::<Vec<XOnlyPublicKey>>(),
            vec![pubkey]
        );
    }

    #[tokio::test]
    async fn test_profile_repository_updated_same_event() {
        let pubkey = XOnlyPublicKey::from_str(
            "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
        )
        .unwrap();
        let metadata = Metadata::from_json(r#"{"name": "foobar"}"#).unwrap();
        let repo = ProfileRepository::new();
        let created_at = Timestamp::now();
        let profile = Profile::new(pubkey, created_at, metadata);
        repo.update(profile.clone()).await;
        let actual = repo.updated(profile.clone()).await;
        assert_eq!(actual, profile);
    }

    #[tokio::test]
    async fn test_profile_repository_updated_update_event() {
        let pubkey = XOnlyPublicKey::from_str(
            "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
        )
        .unwrap();
        let metadata = Metadata::from_json(r#"{"name": "foobar"}"#).unwrap();
        let repo = ProfileRepository::new();
        let old_profile = Profile::new(
            pubkey,
            Timestamp::now() - Duration::from_secs(1),
            metadata.clone(),
        );
        let new_profile = Profile::new(pubkey, Timestamp::now(), metadata);
        repo.update(old_profile).await;
        let actual = repo.updated(new_profile.clone()).await;
        assert_eq!(actual, new_profile);
    }

    #[tokio::test]
    async fn test_profile_repository_updated_does_not_update_event() {
        let pubkey = XOnlyPublicKey::from_str(
            "4d39c23b3b03bf99494df5f3a149c7908ae1bc7416807fdd6b34a31886eaae25",
        )
        .unwrap();
        let metadata = Metadata::from_json(r#"{"name": "foobar"}"#).unwrap();
        let repo = ProfileRepository::new();
        let old_profile = Profile::new(
            pubkey,
            Timestamp::now() - Duration::from_secs(1),
            metadata.clone(),
        );
        let new_profile = Profile::new(pubkey, Timestamp::now(), metadata);
        repo.update(new_profile.clone()).await;
        let actual = repo.updated(old_profile).await;
        assert_eq!(actual, new_profile);
    }
}
