use std::time::Duration;

use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;

pub struct Connection {
    keys: Keys,
    client: Client,
}

impl Connection {
    pub async fn new(keys: Keys, relays: Vec<String>) -> Result<Self> {
        let client = Client::new(&keys);

        client.add_relays(relays).await?;
        client.connect().await;

        Ok(Self { keys, client })
    }

    pub async fn subscribe_timeline(
        &self,
    ) -> Result<tokio::sync::broadcast::Receiver<RelayPoolNotification>> {
        let followings = self.client.get_contact_list_public_keys(None).await?;
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
            .subscribe(vec![timeline_filter, profile_filter])
            .await;

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
