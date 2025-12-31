//! Tears framework integration for nostui
//!
//! This module provides the Tears (Elm-like) architecture implementation for nostui.
//! It uses a hybrid pattern where:
//! - Global state (AppState) is managed centrally
//! - Components delegate update/view logic but receive state as parameters
//! - Only the top-level App implements the Application trait

pub mod fps_tracker;
