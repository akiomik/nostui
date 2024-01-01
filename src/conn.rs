use nostr_sdk::prelude::*;
use std::time::Duration;

use crate::config::Config;

pub struct Conn {
    rx: tokio::sync::mpsc::UnboundedReceiver<Event>,
}

impl Conn {
    pub fn new(privatekey: String, relays: Vec<String>) -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        tokio::spawn(async move {
            let keys = Keys::from_sk_str(&privatekey)?;
            let nostr_client = Client::new(&keys);

            nostr_client.add_relays(relays).await?;
            nostr_client.connect().await;

            let followings = nostr_client.get_contact_list_public_keys(None).await?;
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
            nostr_client
                .subscribe(vec![timeline_filter, profile_filter])
                .await;

            nostr_client
                .handle_notifications(|notification| async {
                    if let RelayPoolNotification::Event { event, .. } = notification {
                        tx.send(event)?;
                    };

                    Ok(false)
                })
                .await?;

            Ok::<(), nostr_sdk::client::Error>(())
        });

        Conn { rx }
    }

    pub fn recv(&mut self) -> Result<Event, tokio::sync::mpsc::error::TryRecvError> {
        self.rx.try_recv()
    }
}
