//! Legacy widgets module
//!
//! Widgets have been moved to presentation::widgets.
//! This module remains for backward compatibility during migration.

pub use crate::presentation::widgets::*;

// Re-export from presentation layer
pub use crate::presentation::widgets::public_key::PublicKey;
pub use crate::presentation::widgets::scrollable_list::ScrollableList;
pub use crate::presentation::widgets::shrink_text::ShrinkText;
pub use crate::presentation::widgets::text_note::TextNote;
