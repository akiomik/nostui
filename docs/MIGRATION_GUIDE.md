# Migration Guide

This document describes breaking changes and how to migrate your configuration.

## v0.1.1 → v0.2.0

### Breaking Changes

#### Changed: Keybindings are now grouped by screen

**What changed:**
- Keybindings are now nested under a screen name (currently `Home`) instead of being defined directly under `keybindings`
- Flat keybindings from older configs are silently ignored after upgrading. The application still works because the default keybindings are merged in, but any custom keybindings you defined will no longer take effect until you move them under `Home`

**Migration steps:**

Wrap your existing keybindings in a `Home` object:

```diff
{
  "keybindings": {
-   "<q>": "Quit",
-   "<n>": "NewTextNote",
-   "<Ctrl-p>": "SubmitTextNote"
+   "Home": {
+     "<q>": "Quit",
+     "<n>": "NewTextNote",
+     "<Ctrl-p>": "SubmitTextNote"
+   }
  }
}
```

---

#### Removed: Suspend functionality

**What changed:**
- The `Suspend` key action has been removed from keybindings
- `SystemMsg::Suspend` has been removed from the message system
- `Ctrl-z` keybinding support has been removed from the default configuration

**Migration steps:**

1. **Remove Suspend keybinding from your config file**

   If you have a custom configuration file (e.g., `~/.config/nostui/config.json5`), remove any `Suspend` action mappings:

   ```diff
   {
     "keybindings": {
       "Home": {
         "<q>": "Quit",
   -     "<Ctrl-z>": "Suspend",
         "<n>": "NewTextNote",
         ...
       }
     }
   }
   ```

2. **Use terminal emulator's suspend feature instead**

   Until proper suspend support is implemented in the Tears framework, you can use your terminal emulator's built-in suspend functionality if available. Note that this may cause terminal display corruption and require manual terminal reset (`reset` command) after resuming.

**Workaround:**

If you need to temporarily background the application:
- Use `q` or `Ctrl-c` to quit cleanly
- Restart the application when needed

---

#### Changed: FPS display now shows only ticks per second

**What changed:**
- The FPS counter now displays only "X.XX ticks per sec" instead of "X.XX ticks per sec (app) X.XX frames per sec (render)"
- The `Fps` struct no longer contains `render_fps` and `render_frames` fields
- Only `app_fps` and `app_frames` remain in `Fps`

**Reason:**
The Tears framework handles frame rendering internally, making accurate frame-level FPS measurement impossible from the application layer. The render FPS value was never accurate because the `Terminal` is owned by the `Runtime`, not the `Application`. Therefore, we removed the misleading "frames per sec" display and the associated unused fields.

**Impact:**

This is primarily an internal change. If you were:

1. **Using the FPS display as a user**: You will now see a simpler display showing only tick rate, which accurately reflects the application's update frequency.

2. **Accessing `Fps` in custom code**: Remove any references to `render_fps` and `render_frames`:

   ```diff
   let fps = &state.fps;
   - println!("Render FPS: {}", fps.render_fps);
   + // Only the app tick rate is available now, via the getter
   println!("App FPS: {:?}", fps.app_fps());
   ```

**Note:**
The "ticks per sec" value still provides useful performance information, representing how frequently the application processes events and updates state.

### Deprecations

#### Deprecated: `privatekey` config field

**What changed:**
- The `privatekey` config field has been replaced by `key`
- `key` also enables readonly mode by accepting an `npub...` public key, which `privatekey` does not support
- `privatekey` still works as a fallback (it is used only when `key` is empty or fails to parse), so existing configs keep working for now. It may be removed in a future release

**Migration steps:**

Rename `privatekey` to `key` in your config file:

```diff
{
- "privatekey": "nsec1...",
+ "key": "nsec1...",
  "relays": ["wss://nos.lol"]
}
```
