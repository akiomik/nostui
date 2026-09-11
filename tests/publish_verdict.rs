//! The accepting side of the publish verdict, which no unit test can reach.
//!
//! `NostrEvents::relay_verdict` decides whether a publish is reported as done, and its
//! refusing directions are covered in that file's `mod tests`. The accepting one is not:
//! `EventSendStatus::Ack` wraps an `EventSendAcknowledgement` whose constructor is
//! private to nostr-sdk, so a `SendEventOutput` assembled by hand can only carry
//! `EventSendStatus::Sent` — the variant the verdict deliberately refuses. A relay has to
//! answer `OK true` for an `Ack` to exist at all, which is why this test opens a socket
//! (#523).
//!
//! It drives the same path the application does — `NostrEvents::stream()`, a
//! `SendEventBuilder` command, the `EventPublished` message that comes back — rather than
//! calling the verdict directly, which from here is private anyway.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::slice;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use nostr_sdk::prelude::*;
use nostui::infrastructure::subscription::nostr::NostrEvents;
use nostui::model::nostr_gateway::{Message, NostrCommand, PublishId};
use nostui::Result;
use tears::SubscriptionSource;
use tokio::time::timeout;

/// Long enough that a slow machine is not what fails this, short enough that a hung
/// socket is reported as a failure rather than by the harness killing the run.
const PUBLISH_TIMEOUT: Duration = Duration::from_secs(30);

/// A relay that answers `OK false` to everything, so that a send can be refused by one
/// relay while another accepts it.
#[derive(Debug)]
struct RefuseEverything;

impl WritePolicy for RefuseEverything {
    fn admit_event<'a>(
        &'a self,
        _event: &'a Event,
        _addr: &'a SocketAddr,
    ) -> Pin<Box<dyn Future<Output = WritePolicyResult> + Send + 'a>> {
        Box::pin(async move {
            WritePolicyResult::reject(MachineReadablePrefix::Blocked, "refused on purpose")
        })
    }
}

/// Publish one text note through the real subscription and return what the application
/// would have been told.
///
/// The relays are passed in rather than started here so that a caller can also query
/// them afterwards: a partial-success test that never checked the refusing relay refused
/// would be the accepting test written twice.
async fn publish_through_nostr_events(
    relays: &[RelayUrl],
    keys: &Keys,
) -> Result<Result<(), String>> {
    let client = Client::new();
    for url in relays {
        client.add_relay(url).await?;
    }
    client.connect().await;

    let nostr_events = NostrEvents::new(Arc::new(client), keys.public_key(), Some(keys.clone()));
    let mut stream = nostr_events.stream();

    let Some(Message::Ready { sender }) = stream.next().await else {
        panic!("the subscription's first message is Ready, carrying the command sender");
    };

    sender.send(NostrCommand::SendEventBuilder {
        id: Some(PublishId(1)),
        event_builder: EventBuilder::new(Kind::TextNote, "published by an integration test"),
    })?;

    // Relay notifications arrive on the same stream, so read past them.
    let outcome = timeout(PUBLISH_TIMEOUT, async {
        while let Some(message) = stream.next().await {
            if let Message::EventPublished { id, result } = message {
                assert_eq!(id, PublishId(1), "the outcome names the publish it settles");
                return result;
            }
        }
        panic!("the subscription ended without reporting the publish");
    })
    .await?;

    Ok(outcome)
}

/// What one relay is holding for an author, asked over a connection of its own.
async fn notes_held_by(url: &RelayUrl, author: PublicKey) -> Result<usize> {
    let client = Client::new();
    client.add_relay(url).await?;
    client.connect().await;

    let events = client
        .fetch_events(Filter::new().author(author).kind(Kind::TextNote))
        .timeout(PUBLISH_TIMEOUT)
        .await?;

    client.disconnect().await;

    Ok(events.len())
}

#[tokio::test]
async fn a_send_a_relay_acknowledged_is_a_publish() -> Result<()> {
    let relay = MockRelay::run().await?;
    let url = relay.url().await;
    let keys = Keys::generate();

    let outcome = publish_through_nostr_events(slice::from_ref(&url), &keys).await?;

    assert_eq!(outcome, Ok(()));
    // And the relay really is holding it, so the `Ok` is not a verdict on nothing.
    assert_eq!(notes_held_by(&url, keys.public_key()).await?, 1);

    Ok(())
}

#[tokio::test]
async fn one_relay_acknowledging_is_a_publish_even_when_another_refuses() -> Result<()> {
    let accepting = MockRelay::run().await?;
    let accepting_url = accepting.url().await;

    let refusing = LocalRelay::builder().write_policy(RefuseEverything).build();
    refusing.run().await?;
    let refusing_url = refusing.url().await;

    let keys = Keys::generate();
    let outcome =
        publish_through_nostr_events(&[accepting_url.clone(), refusing_url.clone()], &keys).await?;

    // One relay holding the event is enough: it exists on the network, and the refusal
    // is logged rather than reported.
    assert_eq!(outcome, Ok(()));

    // The halves really were different, which is the whole point of this case — without
    // these the test is the accepting one with a second relay attached.
    assert_eq!(notes_held_by(&accepting_url, keys.public_key()).await?, 1);
    assert_eq!(notes_held_by(&refusing_url, keys.public_key()).await?, 0);

    Ok(())
}
