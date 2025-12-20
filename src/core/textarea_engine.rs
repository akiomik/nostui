use crossterm::event::KeyEvent;

use crate::core::state::ui::TextAreaState;

/// Engine interface that applies a sequence of key events to a textarea snapshot
/// and returns the resulting snapshot. The implementation should be deterministic
/// and free of external side effects so that it can be used from the pure update path.
pub trait TextAreaEngine {
    /// Apply keys to the given snapshot and return the updated snapshot.
    fn apply_keys(&self, snapshot: &TextAreaState, keys: &[KeyEvent]) -> TextAreaState;
}

/// No-op engine used for tests or when no editing should occur.
pub struct NoopTextAreaEngine;

impl TextAreaEngine for NoopTextAreaEngine {
    fn apply_keys(&self, snapshot: &TextAreaState, _keys: &[KeyEvent]) -> TextAreaState {
        snapshot.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ui::CursorPosition;

    #[test]
    fn noop_engine_returns_same_snapshot() {
        let engine = NoopTextAreaEngine;
        let snap = TextAreaState::new("abc".into(), CursorPosition { line: 0, column: 3 }, None);
        let out = engine.apply_keys(&snap, &[]);
        assert_eq!(out, snap);
    }
}
