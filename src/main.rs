#![deny(warnings)]

use clap::Parser;
use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;
use tears::Runtime;

use nostui::{
    infrastructure::{cli::Cli, config::Config},
    tears::{app::InitFlags, TearsApp},
    utils::{initialize_logging, initialize_panic_handler},
};

async fn tokio_main() -> Result<()> {
    initialize_logging()?;
    initialize_panic_handler()?;

    let _args = <Cli as Parser>::parse();

    // Load configuration
    let config = Config::new()?;

    // Load user keys from config
    let keys = if config.privatekey.is_empty() {
        return Err(color_eyre::eyre::eyre!("Private key not found in config"));
    } else {
        Keys::parse(&config.privatekey)?
    };
    let pubkey = keys.public_key();

    log::info!("Starting nostui with public key: {pubkey}");

    // Create Nostr client
    let client = Client::new(keys.clone());

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
        pubkey: Some(pubkey),
        config,
        nostr_client: client,
        keys,
    };

    // Create Tears runtime
    let runtime = Runtime::<TearsApp>::new(init_flags);

    // Setup terminal
    let mut terminal = ratatui::init();
    terminal.clear()?;

    // Run the Tears application
    log::info!("Starting Tears application...");
    let result = runtime.run(&mut terminal, 60).await;

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
