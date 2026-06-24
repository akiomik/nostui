#![deny(warnings)]

// TEA runtime driver (imperative shell)
pub mod runtime;

// Application layer (use cases, messages, app state)
pub mod application;

// Infrastructure layer
pub mod infrastructure;

// Presentation layer
pub mod presentation;

// Domain logic
pub mod domain;

// Utilities
pub mod utils;

// Models
pub mod model;

// Re-exports for convenience
pub use application::state::AppState;
pub use color_eyre::eyre::Result;

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
