//! Tears application messages
//!
//! This module defines the message types for the tears application.
//! These are independent from the existing application::msg system.

use crossterm::event::KeyEvent;
use nowhear::{MediaEvent, MediaSourceError};

use crate::model::nostr_gateway::Message as NostrSubscriptionMessage;

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
    /// A terminal event nostui does not act on
    ///
    /// The mapping from `crossterm::event::Event` has to be total and a subscription
    /// cannot decline to produce a message, so an event with no handler becomes this
    /// and is dropped where it is handled.
    ///
    /// How much reaches it depends on the platform. On unix a mouse, paste or focus
    /// event is delivered only if the application asks for it, and nostui asks for
    /// none of them. A Windows console reports mouse and focus records whether or not
    /// anyone asked — crossterm parses them unconditionally, and `enable_raw_mode`
    /// does not clear `ENABLE_MOUSE_INPUT`, which is on by default — so there they do
    /// arrive. Before #527 they became ticks, which the FPS display counted as such.
    ///
    /// A Windows console also reports key releases, and those come here too since #531:
    /// a release is not someone pressing a key, and treating it as one ran every binding
    /// twice on that platform. See `terminal_event_to_msg`.
    TerminalEventIgnored,
    /// Show an error message
    ShowError(String),
    /// Key input event
    KeyInput(KeyEvent),
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
    /// Open mention timeline
    OpenMentionTab,
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
