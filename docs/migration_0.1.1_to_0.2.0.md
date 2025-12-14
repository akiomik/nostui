# Migration Guide: 0.1.1 → 0.2.0

This guide explains the changes required to migrate a local setup and user configuration from 0.1.1 to 0.2.0.

The 0.2.0 release completes the switch-over to the Elm-like architecture (AppRunner) and flattens several legacy concepts. Most changes are backwards-compatible at runtime, but there are breaking changes in configuration formats.

## TL;DR
- Keybindings are now a flat map: "<keyseq>" → "ElmAction" (no per-mode mapping)
- Styles are now a flat map: key → style-string (no per-mode mapping)
- Legacy entrypoint is replaced by AppRunner (no action needed for most users)
- A new `UiMode { Normal, Composing }` exists (internal). A legacy `show_input` flag still exists but is slated for removal later.

---

## Keybindings (breaking change)

Prior to 0.2.0, keybindings were nested by legacy Mode. From 0.2.0, keybindings are a single flat map where a key sequence maps to a single action.

Supported actions (case-sensitive):
- `ScrollUp`, `ScrollDown`, `ScrollToTop`, `ScrollToBottom`, `Unselect`
- `NewTextNote`, `ReplyTextNote`, `React`, `Repost`
- `Quit`, `Suspend`, `SubmitTextNote`

Key sequences follow the same string format as before (e.g., "<j>", "<k>", "<esc>", "<C-c>"). Note: multi-key sequences like "<g><g>" (sequential chords) are not supported in 0.2.0; only single-key bindings are matched at runtime.

### Example: old → new

Old (0.1.1):
```json5
keybindings: {
  Home: {
    "<j>": "ScrollDown",
    "<k>": "ScrollUp",
    "<n>": "NewTextNote",
    "<esc>": "Unselect",
  },
}
```

New (0.2.0):
```json5
keybindings: {
  "<j>": "ScrollDown",
  "<k>": "ScrollUp",
  "<n>": "NewTextNote",
  "<esc>": "Unselect",
}
```

Migration steps:
1) Remove the outer object keyed by legacy mode (e.g., `Home`).
2) Move all entries one level up so that key sequences are direct keys in `keybindings`.
3) Ensure actions are spelled as listed above.

Notes:
- Multi-key (sequential) chords like "<g><g>" are not supported in 0.2.0; please map actions to single-key bindings only.
- Internally, the keybinding logic is now interpreted by the Elm translator, not the legacy app.
