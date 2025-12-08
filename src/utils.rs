//! Common utilities
//!
//! This module contains shared utility functions and helpers:
//! - Logging configuration
//! - Panic handling
//! - Path management

pub mod logging;
pub mod panic;
pub mod paths;

// Re-export commonly used functions for backward compatibility
pub use logging::initialize_logging;
pub use panic::initialize_panic_handler;
pub use paths::{get_config_dir, get_data_dir, version};
