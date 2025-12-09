//! Core Elm Architecture implementation
//!
//! This module contains the core components of the Elm architecture:
//! - Messages and raw messages
//! - Application state management
//! - Update logic and command execution
//! - Message translation layer

pub mod cmd;
pub mod cmd_executor;
pub mod msg;
pub mod raw_msg;
pub mod state;
pub mod translator;
pub mod update;
