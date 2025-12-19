use std::time::Duration;

use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;
use tokio::sync::broadcast;

/// Default timeout for fetching contact lists from relays
const DEFAULT_CONTACT_LIST_TIMEOUT_SECS: u64 = 10;

pub struct Connection {
    client: Client,
}

impl Connection {
    pub async fn new(keys: Keys, relays: Vec<String>) -> Result<Self> {
        let client = Client::new(keys.clone());

        for relay in relays {
            client.add_relay(&relay).await?;
        }
        client.connect().await;

        Ok(Self { client })
    }

    pub async fn subscribe_timeline(&self) -> Result<broadcast::Receiver<RelayPoolNotification>> {
        let followings = self
            .client
            .get_contact_list_public_keys(Duration::from_secs(DEFAULT_CONTACT_LIST_TIMEOUT_SECS))
            .await?;
        let timeline_filter = Filter::new()
            .authors(followings.clone())
            .kinds([
                Kind::TextNote,
                Kind::Repost,
                Kind::Reaction,
                Kind::ZapReceipt,
            ])
            .since(Timestamp::now() - Duration::new(60 * 5, 0)); // 5min
        let profile_filter = Filter::new().authors(followings).kinds([Kind::Metadata]);

        // Subscribe to both timeline and profile data concurrently
        tokio::try_join!(
            self.client.subscribe(timeline_filter, None),
            self.client.subscribe(profile_filter, None)
        )?;

        Ok(self.client.notifications())
    }

    pub async fn send(&mut self, event: Event) -> Result<()> {
        self.client.send_event(&event).await?;
        Ok(())
    }

    pub async fn close(self) {
        self.client.shutdown().await
    }
}
