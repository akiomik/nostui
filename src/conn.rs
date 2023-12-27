use nostr_sdk::prelude::*;

pub struct Conn {
    rx: tokio::sync::mpsc::UnboundedReceiver<Event>,
}

impl Default for Conn {
    fn default() -> Self {
        Self::new()
    }
}

impl Conn {
    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        tokio::spawn(async move {
            let keys = Keys::from_pk_str(
                "npub12gtrhfv04634qsyfm6l3m7a06l04qta6yefkuwezwcw6z4qe5nvqddy5qj",
            )?;
            let nostr_client = Client::new(&keys);
            nostr_client.add_relays(["wss://yabu.me"]).await?;
            nostr_client.connect().await;

            let filters = Filter::new()
                // .author(keys.public_key())
                .kind(Kind::TextNote)
                .since(Timestamp::now());
            nostr_client.subscribe(vec![filters]).await;

            nostr_client
                .handle_notifications(|notification| async {
                    if let RelayPoolNotification::Event { event, .. } = notification {
                        if event.kind == Kind::TextNote {
                            tx.send(event)?;
                        };
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
