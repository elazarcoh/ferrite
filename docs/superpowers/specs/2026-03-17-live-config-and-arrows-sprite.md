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

The dialog becomes **non-modal**: `show_config_dialog` creates the window and returns the dialog `HWND` immediately (or `null` on failure). The app's existing `GetMessageW` loop routes dialog messages to the dialog wndproc naturally (all windows on one thread share the message queue).

```
App::run()  ←─── existing Win32 message loop (GetMessage / DispatchMessage)
    │
    ├── pet windows: WM_TIMER → UpdateLayeredWindow (unchanged)
    └── config dialog: WM_COMMAND, WM_DRAWITEM, etc. → config_wnd_proc
                           │
                           └── on any config change:
                               send AppEvent::ConfigChanged(config) via crossbeam Sender
                               App::run() drains channel → apply_config() + config::save()
```

### DialogCtx ownership (critical)

Currently `DialogCtx` is a `Box<DialogCtx>` allocated in the `show_config_dialog` stack frame, with a raw pointer stored in `GWLP_USERDATA`. The inner message loop kept the stack frame alive. With the modal loop removed, `show_config_dialog` returns immediately and the `Box` would be dropped, leaving a dangling pointer.

**Fix:** Use `Box::into_raw()` to transfer ownership to the Win32 system. `show_config_dialog` calls `Box::into_raw(ctx)` and stores the raw pointer in `GWLP_USERDATA`. `WM_DESTROY` reclaims ownership via `Box::from_raw(ptr)` and drops the box — this is the sole point of deallocation.

```rust
// in show_config_dialog:
let ctx = DialogCtx::new(config.clone(), tx.clone());
let ctx_ptr = Box::into_raw(ctx);
SetWindowLongPtrW(hwnd, GWLP_USERDATA, ctx_ptr as isize);

// in WM_DESTROY:
let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DialogCtx;
if !ptr.is_null() {
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
    let ctx = Box::from_raw(ptr);
    ctx.gallery.destroy_thumbnails();
    ctx.destroy_brushes();
    // Box dropped here
}
```

### Sender plumbing

`DialogCtx` gains a `tx: crossbeam_channel::Sender<AppEvent>` field. `show_config_dialog` receives the sender as a parameter:

```rust
pub fn show_config_dialog(parent: HWND, config: &Config, tx: Sender<AppEvent>) -> HWND
```

`App` passes its existing event sender when calling `show_config_dialog`.

### Config persistence

On every `ConfigChanged` event, `App` both applies and saves:

```rust
AppEvent::ConfigChanged(cfg) => {
    self.apply_config(cfg.clone())?;
    if let Err(e) = config::save(&config::config_path(), &cfg) {
        log::warn!("auto-save config failed: {e}");
    }
}
```

### File watcher feedback loop

Saving on every `ConfigChanged` will trigger the `notify` file watcher, which fires `ConfigReloaded`. Because `apply_config` only rebuilds pets where `inst.cfg != pet_cfg`, and by the time `ConfigReloaded` arrives the in-memory pets already match the saved config, the second apply is a no-op (no windows are recreated). This is acceptable — no special suppression needed.

### Config dialog changes

| What | How |
|---|---|
| Remove inner message loop | Delete the `loop { GetMessageW... }` block from `show_config_dialog` |
| Remove `EnableWindow` calls | No longer needed without modal blocking |
| Remove Save/Cancel buttons | Delete `DLG_OK` / `DLG_CANCEL` controls and their draw/command handlers |
| Sprite gallery click | Calls `send_config_changed()` immediately on `LBN_SELCHANGE` |
| Numeric fields (scale/X/Y/speed) | Apply on `EN_KILLFOCUS`; `WM_CLOSE` also calls `read_fields()` then `send_config_changed()` before `DestroyWindow` to capture the last edit |
| Add pet | Calls `send_config_changed()` immediately |
| Remove pet | Calls `send_config_changed()` immediately |
| WM_CLOSE | Calls `read_fields()` → `send_config_changed()` → `DestroyWindow` |
| WM_DESTROY | Reclaims `Box<DialogCtx>` via `Box::from_raw`; destroys thumbnails + brushes |

`send_config_changed()` is a helper inside `config_wnd_proc`:

```rust
fn send_config_changed(ctx: &DialogCtx) {
    let _ = ctx.tx.send(AppEvent::ConfigChanged(ctx.state.config.clone()));
}
```

### App changes

| What | How |
|---|---|
| `show_config_dialog` return type | `HWND` (null on failure) |
| `TrayOpenConfig` handler | If stored dialog HWND is valid (`IsWindow`), bring it to front via `SetForegroundWindow`; otherwise call `show_config_dialog` and store the HWND |
| New `AppEvent::ConfigChanged(Config)` | Added to `event.rs`; handler calls `apply_config` + `config::save` |
| App stores dialog HWND | `config_dialog_hwnd: HWND` field on `App` (or equivalent); cleared when `WM_DESTROY` fires (app can listen for `WM_DESTROY` via a sentinel `AppEvent::ConfigDialogClosed` sent from `WM_DESTROY`, or just check `IsWindow` before each use) |

### No cancel / no revert

Closing the dialog via the × button commits all changes (they were applied live). There is no revert mechanism.

### Field apply timing

Numeric fields (`Scale`, `X`, `Y`, `Speed`) apply on **`EN_KILLFOCUS`**. Additionally, `WM_CLOSE` calls `read_fields()` and `send_config_changed()` before destroying the window, ensuring the last typed value is applied even if the user clicks × without moving focus away from the field first.

---

## Files Changed

| File | Change |
|---|---|
| `src/window/sprite_gallery.rs` | Remove `test_pet` filter; map display name to `"arrows"` |
| `src/tray/config_window.rs` | Remove modal loop; remove Save/Cancel; add `tx` to DialogCtx; add `EN_KILLFOCUS` and `WM_CLOSE` handling; use `Box::into_raw` / `Box::from_raw`; send `ConfigChanged` on all state changes |
| `src/event.rs` | Add `AppEvent::ConfigChanged(Config)` |
| `src/app.rs` | Update `TrayOpenConfig`; add `ConfigChanged` handler (apply + save); store dialog HWND; pass `tx` to `show_config_dialog` |

---

## Out of Scope

- Revert/undo
- Keyboard shortcut to open config
- Animations while typing in numeric fields
- File-watcher suppression during auto-save
- Sprite editor (separate spec)
