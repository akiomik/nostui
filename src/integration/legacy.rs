//! Legacy code for gradual migration
//!
//! This module contains old code that will be gradually removed
//! as the migration to Elm architecture progresses.

pub mod action;
mod component_trait;
pub mod components;
pub mod mode;
pub mod widgets;

pub use component_trait::Component;
