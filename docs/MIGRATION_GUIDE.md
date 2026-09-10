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

#### Removed: FPS counter

**What changed:**
- The row that displayed "X.XX ticks per sec (app) X.XX frames per sec (render)" at the top of the screen is gone, and the line it occupied belongs to the timeline
- `model::fps` and `presentation::widgets::fps` have been removed, along with `AppState::fps` and `AppState::record_tick`
- With them go the application tick they measured: `SystemMsg::Tick`, the `Timer` subscription, `InitFlags::tick_timer`, and `--tick-rate` (see *Removed: `--tick-rate` command line option* below)
- `infrastructure::cli::Cli` loses its `tick_rate` field and now has none
- `SystemMsg` gains `TerminalEventIgnored` in the same change. A terminal event nostui does not act on used to be reported as a tick; it now has its own variant, which is handled by doing nothing

**Reason:**
The counter was a debugging aid and a study of what the framework offers; it never drove a decision a user makes. What it did do was keep the process awake.

Tears 0.11 parks the render loop when an update pass leaves it with nothing to do, and a tick is something to do: the timer fires, a pass runs, the counter is bumped. So an idle nostui woke up 16 times a second forever in order to refresh a number nobody was reading. Removing the counter removes its tick, which is the only periodic wakeup nostui had — everything else it listens to is event-driven.

The render half of this was already addressed: nostui stopped redrawing for ticks that changed no displayed value. That took idle renders from 16 a second to one. This takes them, and the passes, to none.

**Impact:**

1. **As a user**: the counter is gone from the screen, and an idle nostui now genuinely idles. If you were watching it to see whether the application was keeping up, there is no replacement; the logs are the remaining diagnostic.

   **If you use `nip-38`**, one thing to know: the tick used to re-check nostui's subscriptions sixteen times a second, which quietly restarted a media source that had stopped. Nothing does that now. A media source that starts and later dies — a session bus restarting, say — stops reporting, and "now playing" stays stopped for the rest of the run rather than coming back on its own. Restart nostui to get it back. A source that cannot be started at all was never reliably restarted either, and is unchanged. Both are [#529](https://github.com/akiomik/nostui/issues/529).

2. **In custom code**: remove references to `state.fps`. There is no equivalent to read.

3. **Using `Cli`**: `cli.tick_rate` no longer exists, and neither does a `Cli { tick_rate }` literal. The struct is now empty.

4. **Matching on `SystemMsg`**: an exhaustive `match` needs an arm for `TerminalEventIgnored` as well as the removal of the `Tick` one, or it stops compiling. There is nothing to do in it.

5. **Constructing `InitFlags`**: drop the `tick_timer` field. A struct literal that still sets it no longer compiles.

   ```diff
   let init_flags = InitFlags {
       pubkey,
       keys,
       config,
       nostr_client: client,
   -   tick_timer: tick_timer_from_rate(args.tick_rate)?,
   };
   ```

---

#### Removed: `--frame-rate` command line option

**What changed:**
- The `-f` / `--frame-rate` option has been removed. Passing it now fails argument parsing with `error: unexpected argument '-f' found`
- There is no replacement option

**Reason:**
The Tears framework removed its frame rate in 0.11.0. Render cadence is now bounded by the update pass: the loop renders when a pass leaves the view dirty and parks when it has no work, so there is no period left to configure. Keeping the option would mean accepting a value and ignoring it.

**Migration steps:**

Drop the option wherever nostui is launched — a shell alias, a `.desktop` entry, a systemd unit, or a wrapper script:

```diff
- nostui --frame-rate 30
+ nostui
```

`--tick-rate` went the same way; see the next entry.

**Note:**
There is no direct replacement, and no remaining option that behaves like the old throttle — nostui takes no options at all now beyond `--help` and `--version`.

Nothing is needed in the idle direction: with the FPS counter and its tick gone, an idle nostui neither renders nor runs an update pass — barring the `nip-38` cases in the impact list above. What has no ceiling any more is relay traffic. Redraws follow inbound events instead of being clamped to 16 fps, so a busy home feed can redraw more often than it used to.

---

#### Removed: `--tick-rate` command line option

**What changed:**
- The `-t` / `--tick-rate` option has been removed. Passing it now fails argument parsing with `error: unexpected argument '-t' found`
- There is no replacement option

**Reason:**
It configured the interval of the application tick, and the tick existed only to feed the FPS counter (see above). With nothing left to drive, an option that accepted a value and changed nothing would be worse than none — the same reasoning that removed `--frame-rate`.

**Migration steps:**

Drop the option wherever nostui is launched — a shell alias, a `.desktop` entry, a systemd unit, or a wrapper script:

```diff
- nostui --tick-rate 16
+ nostui
```

**Note:**
Nothing about nostui's responsiveness depended on this value. Input, relay events, and media events were never delivered on the tick; they have always arrived on their own subscriptions.

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
