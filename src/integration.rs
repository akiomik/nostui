//! Integration and migration support
//!
//! This module supports gradual migration to Elm architecture:
//! - Elm integration runtime
//! - Legacy code (for gradual removal)

pub mod app_runner;
pub mod elm_integration;
pub mod legacy;
