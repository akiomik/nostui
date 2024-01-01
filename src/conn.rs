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
            let filters = Filter::new()
                .authors(followings)
                .kinds([
                    Kind::TextNote,
                    Kind::Repost,
                    Kind::Reaction,
                    Kind::ZapReceipt,
                ])
                .since(Timestamp::now() - Duration::new(60 * 5, 0)); // 5min
            nostr_client.subscribe(vec![filters]).await;

            nostr_client
                .handle_notifications(|notification| async {
                    if let RelayPoolNotification::Event { event, .. } = notification {
                        match event.kind {
                            Kind::TextNote | Kind::Repost | Kind::Reaction => {
                                tx.send(event)?;
                            }
                            _ => {}
                        }
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
