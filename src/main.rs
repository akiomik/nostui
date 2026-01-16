#![deny(warnings)]

use clap::Parser;
use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;
use secrecy::ExposeSecret;
use tears::Runtime;

use nostui::{
    app::{InitFlags, TearsApp},
    infrastructure::{cli::Cli, config::Config, nostr::PublicKeySigner},
    utils::{initialize_logging, initialize_panic_handler},
};

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
        tick_rate: args.tick_rate,
    };

    // Setup terminal
    let mut terminal = ratatui::init();
    terminal.clear()?;

    // Run the Tears application
    log::info!(
        "Starting Tears application with frame_rate: {}",
        args.frame_rate
    );
    let runtime = Runtime::<TearsApp>::new(init_flags, args.frame_rate as u32);
    let result = runtime.run(&mut terminal).await;

    // Restore terminal
    ratatui::restore();

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
