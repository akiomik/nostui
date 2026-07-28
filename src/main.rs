#![deny(warnings)]

use std::num::{NonZeroU32, NonZeroU64};

use clap::Parser;
use color_eyre::eyre::{eyre, Result};
use nostr_sdk::prelude::*;
use secrecy::ExposeSecret;
use tears::{subscription::time::Timer, FrameRate, Runtime};

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

    let interval_ms = NonZeroU64::new(interval_ms as u64).ok_or_else(|| {
        eyre!("tick rate is too high to produce a non-zero millisecond timer interval: {tick_rate}")
    })?;

    Ok(Timer::new(interval_ms))
}

fn frame_rate_from_value(frame_rate: f64) -> Result<FrameRate> {
    if !frame_rate.is_finite() || frame_rate <= 0.0 {
        return Err(eyre!("frame rate must be a positive finite number"));
    }
    if frame_rate > f64::from(u32::MAX) {
        return Err(eyre!("frame rate is too high: {frame_rate}"));
    }

    let frames_per_second = NonZeroU32::new(frame_rate as u32)
        .ok_or_else(|| eyre!("frame rate is too low to produce a non-zero FPS: {frame_rate}"))?;

    FrameRate::new(frames_per_second).map_err(|e| eyre!("invalid frame rate: {e}"))
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
    let runtime = Runtime::<TearsApp>::new(init_flags, frame_rate_from_value(args.frame_rate)?);
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
}
