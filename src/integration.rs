//! Integration and migration support
//!
//! This module supports gradual migration to Elm architecture:
//! - Elm integration runtime
//! - Legacy code (for gradual removal)

pub mod app_runner;
pub mod cmd_executor;
pub mod coalescer;
pub mod renderer;
pub mod runtime;
pub mod update_executor;
