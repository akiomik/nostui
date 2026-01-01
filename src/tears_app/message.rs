//! Tears application messages
//!
//! This module defines the message types for the tears application.
//! These are independent from the existing core::msg system.

use crossterm::event::KeyEvent;

use crate::tears::subscription::nostr::Message as NostrSubscriptionMessage;

/// Main application message type for tears
#[derive(Debug, Clone)]
pub enum AppMsg {
    /// System-level messages
    System(SystemMsg),
    /// Timeline-related messages
    Timeline(TimelineMsg),
    /// UI-related messages
    Ui(UiMsg),
    /// Nostr-related messages
    Nostr(NostrMsg),
}

/// System messages
#[derive(Debug, Clone)]
pub enum SystemMsg {
    /// Quit the application
    Quit,
    /// Terminal resize event
    Resize(u16, u16),
    /// Tick for FPS calculation
    Tick,
    /// Show an error message
    ShowError(String),
    /// Key input event
    KeyInput(KeyEvent),
    /// Terminal event error
    TerminalError(String),
    /// Suspend the application (Ctrl+Z)
    Suspend,
}

/// Timeline messages
#[derive(Debug, Clone)]
pub enum TimelineMsg {
    /// Scroll up in the timeline
    ScrollUp,
    /// Scroll down in the timeline
    ScrollDown,
    /// Select a specific note
    Select(usize),
    /// Deselect the current note
    Deselect,
}

/// UI messages
#[derive(Debug, Clone)]
pub enum UiMsg {
    /// Start composing a new note
    StartComposing,
    /// Start replying to the selected note
    StartReply,
    /// Cancel composing
    CancelComposing,
    /// Submit the composed note
    SubmitNote,
    /// Process textarea input
    ProcessTextAreaInput(KeyEvent),
    /// React to the selected note
    ReactToSelected,
    /// Repost the selected note
    RepostSelected,
}

/// Nostr messages
#[derive(Debug, Clone)]
pub enum NostrMsg {
    /// Connect to relays
    Connect,
    /// Disconnect from relays
    Disconnect,
    /// Received an event (boxed to reduce enum size)
    EventReceived(Box<nostr_sdk::Event>),
    /// NostrEvents subscription message
    SubscriptionMessage(NostrSubscriptionMessage),
}
