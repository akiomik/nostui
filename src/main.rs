#![deny(warnings)]

use clap::Parser;
use color_eyre::eyre::Result;
use nostr_sdk::prelude::*;
use secrecy::ExposeSecret;
use tears::Runtime;

use nostui::{
    application::config::Config,
    infrastructure::cli::Cli,
    runtime::{InitFlags, TearsApp},
    utils::{initialize_logging, initialize_panic_handler},
};

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

    // Parsed for `--help`, `--version`, and to reject anything else; nostui takes no
    // options of its own since #527 removed `--tick-rate`.
    let _ = <Cli as Parser>::parse();

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
