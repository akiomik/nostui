#![deny(warnings)]

use std::num::NonZeroU64;

use clap::Parser;
use color_eyre::eyre::{eyre, Result};
use nostr_sdk::prelude::*;
use secrecy::ExposeSecret;
use tears::{subscription::time::Timer, Runtime};

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

    // Create initialization flags for TearsApp
    let init_flags = InitFlags {
        pubkey,
        keys,
        config,
        nostr_client: client,
        tick_timer: tick_timer_from_rate(args.tick_rate)?,
    };

    run_on_terminal(init_flags).await
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
