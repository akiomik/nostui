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
/// Sized against nostr-sdk's own ten-second `wait_for_ok_timeout`: long enough for a
/// publish to responsive relays to land, short enough that an unresponsive one does not
/// make quitting look like a hang.
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
    // `shutdown` rather than `disconnect`, because this client is finished rather than
    // merely idle: it shuts each relay down, empties the pool, and broadcasts
    // `ClientNotification::Shutdown`. That broadcast is what lets the detached worker loop
    // wake and exit on its own; `disconnect` sends nothing, so the loop stays parked until
    // the tokio runtime drops it.
    //
    // Bounded, because `shutdown` takes the relay pool's write lock while an in-flight
    // `send_event` holds the read lock for as long as it waits for `OK` — ten seconds per
    // relay by default. Blocking on that lock is not wasted time: it is the user's own
    // post finishing, and letting it land is what we want. Blocking on it unbounded is
    // not, because by this point the terminal is restored and the delay reads as a hang
    // at the shell prompt with SIGINT already taken over by the runtime's signal handler.
    //
    // Two things it still does not do. It does not wait for a relay's connection task to
    // send the WebSocket close frame, and nostr-sdk 0.45 exposes no way to wait for one,
    // so a task not polled before the runtime is dropped still closes without one. And it
    // cannot flush the worker's outbound *queue*, only whatever send is already in flight:
    // a note still queued when this runs is lost, as it was before. Tracked in #511.
    log::info!("Shutting down the Nostr client...");
    if timeout(SHUTDOWN_TIMEOUT, client.shutdown()).await.is_err() {
        log::warn!(
            "Nostr client did not shut down within {SHUTDOWN_TIMEOUT:?}; exiting without it"
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
