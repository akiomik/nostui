#![deny(warnings)]

use clap::Parser;
use color_eyre::eyre::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

use nostui::{
    infrastructure::{cli::Cli, config::Config, tui::real::RealTui},
    integration::app_runner::AppRunner,
    utils::{initialize_logging, initialize_panic_handler},
};

async fn tokio_main() -> Result<()> {
    initialize_logging()?;

    initialize_panic_handler()?;

    let args = <Cli as Parser>::parse();

    // Load configuration (file-based)
    let config = Config::new()?;

    // Override runtime rates from CLI for now (future: move tick/frame rates into Config)
    let tick_rate = args.tick_rate;
    let frame_rate = args.frame_rate;

    // Initialize and run the new Elm AppRunner
    let mut runner = {
        let tui = Arc::new(Mutex::new(
            RealTui::new()?.tick_rate(tick_rate).frame_rate(frame_rate),
        ));
        AppRunner::new_with_real(config.clone(), tui).await?
    };
    runner.run().await?;

    Ok(())
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
