# Live Config Dialog & Arrows Sprite

**Date:** 2026-03-17
**Status:** Approved

## Goal

1. Add the test sprite to the user-visible gallery under the friendly name "arrows".
2. Make config dialog changes apply live — no Save/Cancel buttons; changes take effect immediately.

---

## Item 1 — "arrows" sprite in gallery

### Change

`embedded_stems()` in `src/window/sprite_gallery.rs` currently filters out `"test_pet"`:

```rust
.filter(|s| *s != "test_pet")
```

Remove that filter so `test_pet` is included. Then map its display name: when the gallery entry's key stem is `"test_pet"`, set `display_name = "arrows"` instead of `"test_pet"`.

### Scope

- `src/window/sprite_gallery.rs` — two lines changed in `load()`
- No asset renames; no test changes

---

## Item 2 — Non-modal live config dialog

### Current architecture

`show_config_dialog` runs an inner blocking `GetMessageW` loop, making the dialog modal. It returns `Option<Config>` when the user clicks Save or Cancel.

### New architecture

The dialog becomes **non-modal**: `show_config_dialog` creates the window and returns `None` immediately. The app's existing `GetMessageW` loop routes dialog messages to the dialog wndproc naturally (all windows on one thread share the message queue).

```
App::run()  ←─── existing Win32 message loop (GetMessage / DispatchMessage)
    │
    ├── pet windows: WM_TIMER → UpdateLayeredWindow (unchanged)
    └── config dialog: WM_COMMAND, WM_DRAWITEM, etc. → config_wnd_proc
                           │
                           └── on any config change:
                               send AppEvent::ConfigChanged(config) via crossbeam channel
                               App::run() drains channel → apply_config()
```

### Config dialog changes

| What                          | How                                                                 |
|-------------------------------|---------------------------------------------------------------------|
| Remove inner message loop     | Delete the `loop { GetMessageW... }` block from `show_config_dialog` |
| Remove `EnableWindow` calls   | No longer needed without modal blocking                             |
| Remove Save/Cancel buttons    | Delete `DLG_OK` / `DLG_CANCEL` controls and their draw/command handlers |
| Sprite gallery click          | Sends `ConfigChanged` immediately on `LBN_SELCHANGE`               |
| Numeric fields (scale/X/Y/speed) | Send `ConfigChanged` on `EN_KILLFOCUS` (focus loss) or Enter key (`EN_CHANGE` + validate) |
| Add pet                       | Sends `ConfigChanged` immediately                                   |
| Remove pet                    | Sends `ConfigChanged` immediately                                   |
| WM_CLOSE / WM_DESTROY         | No special action needed — changes already applied                  |
| Multiple open prevention      | App stores the dialog HWND; if it is valid (`IsWindow`), bring it to front instead of creating new |

### App changes

| What                            | How                                                                   |
|---------------------------------|-----------------------------------------------------------------------|
| `show_config_dialog` signature  | Returns `()` (or `HWND`) instead of `Option<Config>`                 |
| `TrayOpenConfig` handler        | Creates dialog, stores HWND; does not block                           |
| New `AppEvent::ConfigChanged`   | Variant added to `event.rs`; handler calls `apply_config`             |
| `apply_config` (existing)       | Already updated to rebuild changed existing pets                      |

### No cancel / no revert

Closing the dialog via the × button makes no special action — all changes have already been applied live. There is no revert mechanism (YAGNI).

### Field apply timing

Numeric fields (`Scale`, `X`, `Y`, `Speed`) apply on **`EN_KILLFOCUS`** (when the field loses focus). This avoids applying mid-type values (e.g., clearing a field to retype a number). The same validation logic as `read_fields` is reused.

---

## Files Changed

| File | Change |
|---|---|
| `src/window/sprite_gallery.rs` | Remove `test_pet` filter; map display name to `"arrows"` |
| `src/tray/config_window.rs` | Remove modal loop, Save/Cancel controls, add `EN_KILLFOCUS` handling, send `ConfigChanged` on changes |
| `src/event.rs` | Add `AppEvent::ConfigChanged(Config)` |
| `src/app.rs` | Update `TrayOpenConfig` handler; add `ConfigChanged` handler; store dialog HWND |

---

## Out of Scope

- Revert/undo
- Keyboard shortcut to open config
- Animations while typing in numeric fields
- Sprite editor (separate spec)
