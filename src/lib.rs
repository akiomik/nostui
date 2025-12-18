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
//! use nostui::{core::state::AppState, core::msg::Msg, core::update::update};
//! use nostr_sdk::prelude::*;
//!
//! // Initialize state
//! let keys = Keys::generate();
//! let initial_state = AppState::new(keys.public_key());
//!
//! // Process messages
//! use nostui::core::msg::ui::UiMsg;
//! let (new_state, commands) = update(Msg::Ui(UiMsg::ShowNewNote), initial_state);
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
//! - [`core::state`] - Application state definitions
//! - [`core::msg`] - Message types for state transitions
//! - [`core::update()`] - Pure update functions
//! - [`core::cmd`] - Command definitions for side effects
//! - [`integration::elm_integration`] - Integration runtime for existing code
//! - [`presentation::components`] - UI components
//! - [`domain::nostr`] - Nostr protocol implementations
//! - [`infrastructure::config`] - Configuration management

#![deny(warnings)]

// Core Elm architecture modules
pub mod core;

// Infrastructure layer
pub mod infrastructure;

// Presentation layer
pub mod presentation;

// Domain logic
pub mod domain;

// Utilities
pub mod utils;

// Integration and migration support
pub mod integration;

// Test helpers module (available in dev and test builds)
#[cfg(any(test, debug_assertions))]
pub mod test_helpers;

// Re-exports for convenience
pub use core::cmd::Cmd;
pub use core::msg::Msg;
pub use core::raw_msg::RawMsg;
pub use core::state::AppState;
pub use core::translator::translate_raw_to_domain;
pub use core::update::update;
pub use integration::runtime::{Runtime, RuntimeStats};

/// Result type used throughout the library
pub type Result<T> = color_eyre::eyre::Result<T>;

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
