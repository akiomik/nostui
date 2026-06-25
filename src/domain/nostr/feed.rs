//! Feed identity.
//!
//! A *feed* defines **what** content a timeline shows — the author set and event
//! kinds to request — independent of any UI. A `timeline` (in `model`) is the
//! stateful surface that materialises and displays a feed.

use nostr_sdk::prelude::*;

/// Identifies which feed a timeline displays.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FeedKind {
    /// The home feed (the followed authors, plus the user themselves).
    Home,
    /// The mention feed (kind-1 events that tag the current user via `#p`).
    Mention,
    /// A single author's feed.
    Author(PublicKey),
}
