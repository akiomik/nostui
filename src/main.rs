#![deny(warnings)]

use std::num::NonZeroU64;
use std::time::Duration;

use clap::Parser;
use color_eyre::eyre::{eyre, Result};
use nostr_sdk::prelude::*;
use secrecy::ExposeSecret;
use tears::{subscription::time::Timer, Runtime};
use tokio::time::timeout;

/// How long to wait for the Nostr client to shut down before exiting without it.
///
/// The wait exists so an in-flight publish can finish; the bound exists so a relay that
/// never answers cannot turn quitting into an apparent hang. This is deliberately shorter
/// than nostr-sdk's own ten-second `wait_for_ok_timeout`, which means it is a compromise
/// rather than a guarantee: a relay that acks promptly keeps its publish, and one slower
/// than this loses it exactly as it did before the wait existed.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

use nostui::{
    application::config::Config,
    infrastructure::cli::Cli,
    runtime::{InitFlags, TearsApp},
    utils::{initialize_logging, initialize_panic_handler},
};

fn tick_timer_from_rate(tick_rate: f64) -> Result<Timer> {
    if !tick_rate.is_finite() || tick_rate <= 0.0 {
        return Err(eyre!("tick rate must be a positive finite number"));
    }

    let interval_ms = 1000.0 / tick_rate;
    if interval_ms > u64::MAX as f64 {
        return Err(eyre!(
            "tick rate is too low to convert to a timer interval: {tick_rate}"
        ));
    }

    let interval_ms = NonZeroU64::new(interval_ms as u64).ok_or_else(|| {
        eyre!("tick rate is too high to produce a non-zero millisecond timer interval: {tick_rate}")
    })?;

    Ok(Timer::new(interval_ms))
}

/// Take over the terminal, run the application on it, and restore it on every path out —
/// including a failure to clear it, which used to leave the caller on the alternate
/// screen in raw mode.
async fn run_on_terminal(init_flags: InitFlags) -> Result<()> {
    let mut terminal = ratatui::init();

    let result = async {
        terminal.clear()?;
        log::info!("Starting Tears application");
        Runtime::<TearsApp>::new(init_flags)
            .run(&mut terminal)
            .await
    }
    .await;

    ratatui::restore();

    Ok(result?)
}

async fn tokio_main() -> Result<()> {
    initialize_logging()?;
    initialize_panic_handler()?;

    let args = <Cli as Parser>::parse();

    // Validate the tick rate before the client exists, so a bad value fails without
    // having connected to any relay.
    let tick_timer = tick_timer_from_rate(args.tick_rate)?;

    // Load configuration
    let config = Config::new()?;

    // Create Nostr client
    let (client, pubkey, keys) = if config.key.expose_secret().starts_with("npub") {
        let pubkey = PublicKey::parse(config.key.expose_secret())?;
        (Client::new(), pubkey, None)
    } else {
        let keys = Keys::parse(config.key.expose_secret())
            .or(Keys::parse(config.privatekey.expose_secret()))?;
        let pubkey = keys.public_key();
        (Client::new(), pubkey, Some(keys))
    };
    log::info!("Starting nostui with public key: {pubkey}");

    // Add relays from config
    for relay_url in &config.relays {
        log::info!("Adding relay: {relay_url}");
        client.add_relay(relay_url).await?;
    }

    // Connect to relays
    log::info!("Connecting to relays...");
    client.connect().await;

    // Create initialization flags for TearsApp. `Client` is reference-counted internally,
    // so the clone handed to the application shares one connection pool with the binding
    // kept here — which is what stays reachable to signal termination once `run` returns.
    let init_flags = InitFlags {
        pubkey,
        keys,
        config,
        nostr_client: client.clone(),
        tick_timer,
    };

    let result = run_on_terminal(init_flags).await;

    // Tear the client down. Quitting asks the subscription worker to disconnect, but a
    // quit returned from `update` now terminates the runtime at that same dispatch, so the
    // worker may never be polled before `run` returns. Doing it here does not depend on
    // that.
    //
    // `shutdown` rather than `disconnect`, for the lock it takes. `shutdown` acquires the
    // relay pool's write lock, which an in-flight `send_event` holds for reading until it
    // has its `OK` — ten seconds per relay by default. `disconnect` takes only a read lock
    // and so never waits for one. Waiting is the point: what it waits for is the user's
    // own post finishing.
    //
    // Not for the worker loop's sake — that exits either way. `Runtime::run` consumes the
    // application, so by the time this line runs the command sender it held is dropped and
    // the loop's `cmd_rx.recv()` has already returned `None`.
    //
    // Bounded, because waiting on that lock unbounded is not free: by this point the
    // terminal is restored, so the delay reads as a hang at the shell prompt, with SIGINT
    // already taken over by the runtime's signal handler for the rest of the process
    // lifetime.
    //
    // `SHUTDOWN_TIMEOUT` buys the publish a grace period rather than a guarantee: it is
    // shorter than that ten-second wait, so a relay slow to ack still loses the post here.
    // Bounding the exit is worth that; not bounding it would mean a quit that can sit
    // silent for twenty seconds with no way to interrupt it.
    //
    // Two things it still does not do. It does not wait for a relay's connection task to
    // send the WebSocket close frame, and nostr-sdk 0.45 exposes no way to wait for one,
    // so a task not polled before the runtime is dropped still closes without one. And it
    // cannot flush the worker's outbound *queue*, only whatever send is already in flight:
    // a note still queued when this runs is lost, as it was before. Tracked in #511.
    log::info!("Shutting down the Nostr client...");
    if timeout(SHUTDOWN_TIMEOUT, client.shutdown()).await.is_err() {
        // On stderr, not only in the log. The user has been looking at a restored prompt
        // for `SHUTDOWN_TIMEOUT` with no way to interrupt, and the thing that most likely
        // held it there is an unacknowledged publish — which they can act on, and cannot
        // learn any other way.
        //
        // Stated as a condition, not a diagnosis. A publish is the likeliest holder of the
        // read lock, but `subscribe` and `unsubscribe` take it too, so the wait does not
        // prove anything was being published — and when something was, it need not be a
        // note: reactions, reposts, and NIP-38 status events from a track change all take
        // the same path. Report what is known, and let the reader decide if it applies.
        log::warn!("Nostr client did not shut down within {SHUTDOWN_TIMEOUT:?}");
        eprintln!(
            "{}: relays did not finish shutting down within {}s; if anything was \
             published just before quitting, it may not have reached them",
            env!("CARGO_PKG_NAME"),
            SHUTDOWN_TIMEOUT.as_secs()
        );
    }

    result
}

#[tokio::main]
async fn main() -> Result<()> {
    if let Err(e) = tokio_main().await {
        eprintln!("{} error: Something went wrong", env!("CARGO_PKG_NAME"));
        Err(e)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_timer_from_rate_accepts_positive_tick_rate() {
        assert_eq!(
            tick_timer_from_rate(16.0).expect("tick rate should be valid"),
            Timer::new(NonZeroU64::new(62).expect("non-zero"))
        );
        assert_eq!(
            tick_timer_from_rate(1000.0).expect("tick rate should be valid"),
            Timer::new(NonZeroU64::new(1).expect("non-zero"))
        );
    }

    #[test]
    fn tick_timer_from_rate_rejects_invalid_tick_rate() {
        assert!(tick_timer_from_rate(0.0).is_err());
        assert!(tick_timer_from_rate(-1.0).is_err());
        assert!(tick_timer_from_rate(f64::NAN).is_err());
        assert!(tick_timer_from_rate(f64::INFINITY).is_err());
        assert!(tick_timer_from_rate(1000.1).is_err());
    }

    #[test]
    fn tick_timer_from_rate_rejects_a_rate_whose_interval_overflows_u64_millis() {
        // Positive and finite, so it clears the first guard, but 1000 / 1e-20 is far past
        // `u64::MAX` milliseconds. Pin the message so this asserts the overflow guard
        // rather than passing on whichever guard happens to fire.
        let error = tick_timer_from_rate(1e-20).expect_err("tick rate should be rejected");
        assert!(
            error.to_string().contains("too low to convert"),
            "expected the interval-overflow guard, got: {error}"
        );
    }
}
