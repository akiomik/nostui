use std::time::Duration;

use color_eyre::eyre::Result;

use nostr_sdk::prelude::*;

/// Default timeout for fetching contact lists from relays
const DEFAULT_CONTACT_LIST_TIMEOUT_SECS: u64 = 10;

pub struct Connection {
    keys: Keys,
    client: Client,
}

impl Connection {
    pub async fn new(keys: Keys, relays: Vec<String>) -> Result<Self> {
        let client = Client::new(keys.clone());

        for relay in relays {
            client.add_relay(&relay).await?;
        }
        client.connect().await;

        Ok(Self { keys, client })
    }

    pub async fn subscribe_timeline(
        &self,
    ) -> Result<tokio::sync::broadcast::Receiver<RelayPoolNotification>> {
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
        self.client
            .subscribe(vec![timeline_filter, profile_filter], None)
            .await?;

        Ok(self.client.notifications())
    }

    pub async fn send(&mut self, event: Event) -> Result<()> {
        self.client.send_event(event).await?;
        Ok(())
    }

    pub async fn close(self) -> Result<(), nostr_sdk::client::Error> {
        self.client.shutdown().await
    }
}
