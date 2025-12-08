#![deny(warnings)]
#![allow(dead_code)]

// use clap::Parser; // Not directly needed, used via Cli
use color_eyre::eyre::Result;

use nostui::{
    infrastructure::cli::Cli,
    integration::legacy::app::App,
    utils::{initialize_logging, initialize_panic_handler},
};

async fn tokio_main() -> Result<()> {
    initialize_logging()?;

    initialize_panic_handler()?;

    let args = <Cli as clap::Parser>::parse();
    let mut app = App::new(args.tick_rate, args.frame_rate)?;
    app.run().await?;

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
