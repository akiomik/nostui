//! Tears application messages
//!
//! This module defines the message types for the tears application.
//! These are independent from the existing core::msg system.

use crossterm::event::KeyEvent;
use nowhear::{MediaEvent, MediaSourceError};

use crate::infrastructure::subscription::nostr::Message as NostrSubscriptionMessage;

/// Main application message type for tears
#[derive(Debug, Clone)]
pub enum AppMsg {
    /// System-level messages
    System(SystemMsg),
    /// Timeline-related messages
    Timeline(TimelineMsg),
    /// Editor-related messages
    Editor(EditorMsg),
    /// Nostr-related messages
    Nostr(NostrMsg),
    /// Media-related messages
    Media(Result<MediaEvent, MediaSourceError>),
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
    /// Select the first note in the timeline
    SelectFirst,
    /// Select the last note in the timeline
    SelectLast,
    /// Load more older events when reaching bottom
    LoadMore,
    /// React to the selected note
    ReactToSelected,
    /// Repost the selected note
    RepostSelected,
    /// Select a specific tab by index
    SelectTab(usize),
    /// Switch to the next tab
    NextTab,
    /// Switch to the previous tab
    PrevTab,
    /// Open author timeline for the selected note's author
    OpenAuthorTimeline,
    /// Close the current tab
    CloseCurrentTab,
}

/// Editor messages
#[derive(Debug, Clone)]
pub enum EditorMsg {
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
}

/// Nostr messages
#[derive(Debug, Clone)]
pub enum NostrMsg {
    /// Connect to relays
    Connect,
    /// Disconnect from relays
    Disconnect,
    /// NostrEvents subscription message
    SubscriptionMessage(NostrSubscriptionMessage),
}
