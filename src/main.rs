#![deny(warnings)]

use clap::Parser;
use color_eyre::eyre::{eyre, Result};
use nostr_sdk::prelude::*;
use secrecy::ExposeSecret;
use tears::{subscription::time::Timer, Runtime};

use nostui::{
    application::config::Config,
    infrastructure::{cli::Cli, nostr::PublicKeySigner},
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

    Timer::try_new(interval_ms as u64).ok_or_else(|| {
        eyre!("tick rate is too high to produce a non-zero millisecond timer interval: {tick_rate}")
    })
}

async fn tokio_main() -> Result<()> {
    initialize_logging()?;
    initialize_panic_handler()?;

    let args = <Cli as Parser>::parse();

    // Load configuration
    let config = Config::new()?;

    // Create Nostr client
    let (client, pubkey) = if config.key.expose_secret().starts_with("npub") {
        let pubkey = PublicKey::parse(config.key.expose_secret())?;
        let signer = PublicKeySigner::new(pubkey);
        (Client::new(signer), pubkey)
    } else {
        let keys = Keys::parse(config.key.expose_secret())
            .or(Keys::parse(config.privatekey.expose_secret()))?;
        let pubkey = keys.public_key();
        (Client::new(keys), pubkey)
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
        config,
        nostr_client: client,
        tick_timer: tick_timer_from_rate(args.tick_rate)?,
    };

    // Setup terminal
    let mut terminal = ratatui::init();
    terminal.clear()?;

    // Run the Tears application
    log::info!(
        "Starting Tears application with frame_rate: {}",
        args.frame_rate
    );
    let runtime = Runtime::<TearsApp>::try_new(init_flags, args.frame_rate as u32)?;
    let result = runtime.run(&mut terminal).await;

    // Restore terminal
    ratatui::restore();

    Ok(result?)
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
            Timer::try_new(62).expect("timer interval should be valid")
        );
        assert_eq!(
            tick_timer_from_rate(1000.0).expect("tick rate should be valid"),
            Timer::try_new(1).expect("timer interval should be valid")
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
}
