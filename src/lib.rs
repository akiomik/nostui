//! # Nostui - Nostr TUI Client

#![deny(warnings)]
#![allow(dead_code)]

pub mod action;
pub mod app;
pub mod cli;
pub mod collections;
pub mod components;
pub mod config;
pub mod mode;
pub mod nostr;
pub mod text;
pub mod tui;
pub mod utils;
pub mod widgets;

/// Result type used throughout the library
pub type Result<T> = color_eyre::eyre::Result<T>;

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
