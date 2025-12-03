//! # Nostui - Nostr TUI Client
//!
//! A terminal user interface client for the Nostr protocol, built with Rust and Ratatui.
//! This library implements an Elm-like architecture for predictable state management.
//!
//! ## Architecture Overview
//!
//! This crate is organized around the Elm architecture pattern:
//!
//! - **Model** (`state`): Immutable application state
//! - **Message** (`msg`): Events that can change the state
//! - **Update** (`update`): Pure functions that transform state
//! - **Command** (`cmd`): Side effects (I/O, network, etc.)
//! - **View** (`components`): UI rendering based on current state
//!
//! ## Example Usage
//!
//! ```rust
//! use nostui::{state::AppState, msg::Msg, update::update};
//! use nostr_sdk::prelude::*;
//!
//! // Initialize state
//! let keys = Keys::generate();
//! let initial_state = AppState::new(keys.public_key());
//!
//! // Process messages
//! let (new_state, commands) = update(Msg::ShowNewNote, initial_state);
//!
//! // State is now updated and commands contain side effects to execute
//! assert!(new_state.ui.show_input);
//! ```
//!
//! ## Key Features
//!
//! - **Predictable State Management**: All state changes go through the update function
//! - **Testable**: Pure functions make testing straightforward
//! - **Type Safety**: Strong typing prevents many runtime errors
//! - **Separation of Concerns**: Side effects are clearly separated from state logic
//!
//! ## Modules
//!
//! - [`state`] - Application state definitions
//! - [`msg`] - Message types for state transitions
//! - [`update()`] - Pure update functions
//! - [`cmd`] - Command definitions for side effects
//! - [`elm_integration`] - Integration runtime for existing code
//! - [`components`] - UI components
//! - [`nostr`] - Nostr protocol implementations
//! - [`config`] - Configuration management

#![deny(warnings)]
#![allow(dead_code)]

// Core Elm architecture modules
pub mod cmd;
pub mod elm_integration;
pub mod msg;
pub mod raw_msg;
pub mod state;
pub mod translator;
pub mod update;

// Legacy modules (for gradual migration)
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

// Re-exports for convenience
pub use cmd::Cmd;
pub use elm_integration::{ElmRuntime, ElmRuntimeStats};
pub use msg::Msg;
pub use raw_msg::RawMsg;
pub use state::AppState;
pub use translator::translate_raw_to_domain;
pub use update::update;

/// Result type used throughout the library
pub type Result<T> = color_eyre::eyre::Result<T>;

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
