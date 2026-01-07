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

// Tears framework application
pub mod tears;

// Re-exports for convenience
pub use color_eyre::eyre::Result;
pub use core::state::AppState;

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
