# egui Migration

**Date:** 2026-03-17
**Status:** Approved

## Goal

Replace the hand-rolled Win32 config dialog and sprite editor windows with egui-based UIs,
while keeping the pet windows as raw Win32 layered windows.

---

## Overview

The app currently drives a Win32 `PeekMessageW` loop. The migration replaces that loop with
`eframe::run_native`, which owns the message pump. A hidden main eframe window acts as the
event-loop host; the config dialog and sprite editor each open as **deferred viewports**
(`ctx.show_viewport_deferred`). Pet windows remain raw Win32 (`WS_EX_LAYERED |
WS_EX_TOPMOST | WS_EX_NOACTIVATE`), which winit/eframe's underlying Win32 pump dispatches
automatically. Pet windows are entirely separate HWNDs and never share a rendering surface
with eframe's host window — they continue to use `UpdateLayeredWindow` directly.
winit's internal message pump (`PeekMessage`/`DispatchMessage`) dispatches to all HWNDs
on the main thread including the pet windows, so no additional message handling is needed.

This is Option B from the design discussion: full event-loop migration, no hybrid timer
hacks, best long-term performance and maintainability.

---

## Architecture

### Event Loop

`eframe::run_native` replaces the Win32 `PeekMessageW` loop in `app.rs`. The `App` struct
implements `eframe::App`:

```rust
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Drain crossbeam channel (config reload, tray events, etc.)
        // 2. Compute delta_ms from last_tick_ms
        // 3. Tick all pet instances
        // 4. Show deferred viewports (config, sprite editor) if open
        // 5. Schedule next tick
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}
```

The hidden host window is created with:

```rust
eframe::NativeOptions {
    viewport: ViewportBuilder::default()
        .with_visible(false)
        .with_taskbar(false),
    ..Default::default()
}
```

`with_taskbar(false)` suppresses the taskbar button on most Windows configurations. If it
still appears at runtime (possible on some Windows 11 setups), apply `WS_EX_TOOLWINDOW`
via `raw_window_handle` after creation. This is a known limitation and acceptable for the
initial migration.

The fields `timer_id: usize` and `config_dialog_hwnd: HWND` are **removed** from `App`.
`timer_id` is replaced by `ctx.request_repaint_after`. `config_dialog_hwnd` is replaced by
`config_window_state` (see below). Any `windows-sys` imports that were only used for the
`SetTimer`/`PeekMessageW` loop or `config_dialog_hwnd` are dropped from `app.rs`; the
`windows-sys` dependency itself is retained because `PetInstance::tick` still calls Win32
APIs.

### Renderer Backend

eframe is used with the `wgpu` backend (not the default `glow`/OpenGL backend). `wgpu` is
chosen because it supports both DX12 and Vulkan on Windows and avoids OpenGL driver
compatibility issues on some Windows systems. This requires `default-features = false` plus
`features = ["wgpu"]` in the eframe dependency entry (see Dependency Changes). Pet windows'
`UpdateLayeredWindow` calls are entirely independent of eframe's wgpu swap chain.

### Files Changed

| File | Change |
|---|---|
| `src/app.rs` | Implement `eframe::App`; replace Win32 message loop with eframe update loop; remove `timer_id` and `config_dialog_hwnd` fields |
| `src/tray/config_window.rs` | Rewrite UI with egui; `ConfigWindowState` replaces `ConfigDialogState` as the per-window working state |
| `src/tray/sprite_editor.rs` | Rewrite UI with egui; `SpriteEditorViewport` wraps `SpriteEditorState` with egui-specific fields |
| `src/tray/mod.rs` | Ensure `pub mod config_window;` and `pub mod sprite_editor;` are declared so `app.rs` can import the new state types |
| `src/window/sprite_gallery.rs` | Remove Win32 GDI thumbnail code (`load_thumbnail`, `destroy_thumbnails`); keep discovery logic (`SpriteGallery`, `GalleryEntry`, `SourceKind`); thumbnail `HBITMAP` field dropped from `GalleryEntry` |
| `Cargo.toml` | Add `eframe`, `rfd` dependencies |

The following files are **not changed**: `src/sprite/editor_state.rs`, `src/sprite/sheet.rs`,
`src/sprite/animation.rs`, `src/sprite/behavior.rs`, `src/config/schema.rs`,
`src/config/mod.rs`, `src/config/watcher.rs`, `src/window/pet_window.rs`,
`src/window/blender.rs`, `src/window/wndproc.rs`, `src/window/surfaces.rs`,
`src/event.rs`, `src/assets/`.

`src/config/dialog_state.rs` is **deleted** in the same commit that moves `SpriteKey`
into `src/window/sprite_gallery.rs` — the two must happen together to avoid a duplicate
definition. `SpriteKey` is a `pub` type. `src/window/mod.rs` already declares
`pub mod sprite_gallery;` so no change to `mod.rs` is needed. Both consumers
(`sprite_gallery.rs` and `config_window.rs`) update their import path to
`crate::window::sprite_gallery::SpriteKey`.

`tests/e2e/test_config_dialog_e2e.rs` is **deleted** — it tests `ConfigDialogState` and
`DialogResult`, which no longer exist. The behaviour it covered (pet add/remove, field
validation, selection logic) is carried forward in `ConfigWindowState` and verified by the
manual smoke test. No replacement automated test file is added (the egui UI layer is visual
and cannot be headlessly unit-tested).

---

## Section 1 — Architecture & Event Loop

### Shared State Pattern

Config dialog and sprite editor each require `Arc<Mutex<T>>` for their state, because
deferred viewport closures must be `'static + Send + Sync`.

```rust
// In App:
config_window_state: Option<Arc<Mutex<ConfigWindowState>>>,
sprite_editor_state: Option<Arc<Mutex<SpriteEditorViewport>>>,
```

Opening a window: allocate `Arc<Mutex<...>>`, store in `App`, pass a clone into the viewport
closure. Closing: the closure sets `should_close = true`; `update()` checks the flag and
drops the `Arc`.

### Pet Window Ticks

`ctx.request_repaint_after(Duration::from_millis(16))` at the end of every `update()` call
drives the ~60 fps tick. `delta_ms` is computed from `std::time::Instant`, capped at 200 ms,
identical to today.

### Single-Instance Windows

Config dialog: guarded by `config_window_state.is_some()` in `App`.
Sprite editor: guarded by `sprite_editor_state.is_some()` in `App`. Opening a second time
focuses the existing viewport (`ctx.send_viewport_cmd(ViewportId, ViewportCommand::Focus)`).

---

## Section 2 — Config Dialog

### State

`ConfigWindowState` replaces `ConfigDialogState` (the old Win32-centric working state).
It contains the same logical fields, restructured for egui:

```rust
pub struct ConfigWindowState {
    pub config: Config,                 // working copy; live-applied on every change
    pub selected_pet_idx: Option<usize>,
    pub gallery: SpriteGallery,         // available sprites (embedded + custom); no thumbnails
    pub tx: Sender<AppEvent>,           // sends ConfigChanged
    pub should_close: bool,
}
```

`SpriteGallery` is used for its discovery logic (finding sprites in AppData and embedded
assets). The Win32 `HBITMAP` thumbnail field is removed from `GalleryEntry`; the gallery
shows text names only (thumbnails are out of scope for this migration).

`selected_pet_idx` is initialized to `Some(0)` when `config.pets` is non-empty, `None`
otherwise. On "Remove": if the removed index was the last in the list, select `Some(len-1)`;
otherwise keep the same index (which now points to the next pet). On "Add": select the newly
added pet's index (`Some(config.pets.len() - 1)` after push).

### Layout

Two-column layout inside a single egui window (~600×480, resizable).

**Left column — pet list**
- `egui::ScrollArea` containing one `SelectableLabel` per pet (pet id as label).
- "Add Pet" and "Remove" buttons below the list.
- "Edit…" (enabled when selection is non-None) and "New from PNG…" buttons — open sprite editor.

**Right column — pet settings**
Shown when a pet is selected; blank otherwise.

| Setting | Widget |
|---|---|
| Sheet | `ComboBox` listing gallery entries (display names of embedded + custom sprites); selecting calls `SpriteGallery::key_to_path` to resolve the path |
| Scale | `DragValue` (range 1–4, matching existing `parse_scale` validation) |
| Walk speed | `DragValue` (range 1.0–500.0, suffix " px/s") |
| X position | `DragValue` (i32) |
| Y position | `DragValue` (i32) |
| Flip walk left | `Checkbox` |
| Tag map fields (idle, walk, + 9 optional) | `ComboBox` per field; options = tag names from loaded sheet + "— not set —" for optional |

Every widget change immediately calls:
```rust
tx.send(AppEvent::ConfigChanged(state.config.clone())).ok();
```

No Save/Cancel buttons. This matches the existing live-apply pattern.

---

## Section 3 — Sprite Editor

### State

`SpriteEditorState` (defined in `src/sprite/editor_state.rs`) is unchanged. The egui layer
wraps it in `SpriteEditorViewport`, which also holds egui-specific fields:

```rust
pub struct SpriteEditorViewport {
    pub state: SpriteEditorState,
    pub texture: Option<egui::TextureHandle>,            // uploaded on first frame or on PNG path change; reused until viewport closes
    pub anim: crate::sprite::animation::AnimationState,  // for live preview
    pub preview_sheet: Option<crate::sprite::sheet::SpriteSheet>, // rebuilt when rows, cols, or tags change
    pub should_close: bool,
}
```

`App` holds `sprite_editor_state: Option<Arc<Mutex<SpriteEditorViewport>>>`. The entire
viewport struct is what gets wrapped in `Arc<Mutex<>>`, not just the inner state.

### Layout

Two-column window (~900×600, resizable).

**Left column — canvas**

`egui::Painter` inside a `Frame`/`ScrollArea`:

1. Draw spritesheet texture scaled to fit column width.
2. Draw grid lines (gray, 1 px) over the image based on `state.rows × state.cols`.
3. For each tag, fill its frame cells with a semi-transparent rect using the tag's color.
4. Selected tag gets a brighter/opaque border.
5. Row and Col spinners (`DragValue`, range 1–64) below the canvas; changing either causes
   immediate grid redraw (no button needed — egui rerenders on every change). When rows or
   cols change, validate that all existing tag `from`/`to` indices are still within range
   (`< rows * cols`); clamp any out-of-range indices to `rows * cols - 1`.

Y-axis: `painter` uses top-left origin matching PNG coordinates. `state.frame_rect(i)` returns
`(x, y, w, h)` measured from the top-left — no inversion. This fixes the y-coordinate bug
present in the Win32 version.

Maximize: egui windows are resizable by default. The canvas scales to fill available space.

**Right column — controls**

- **Tag list**: `egui::ScrollArea` with one row per tag. Each row: color swatch (small filled
  rect), tag name, frame range (e.g. `0–3`), `ComboBox` for behavior state assignment.
  Clicking a row sets `state.selected_tag`.

- **Add tag** (inline, no separate dialog):
  - `TextEdit` for name
  - `DragValue` from / to (0 to `state.rows * state.cols - 1`)
  - `ComboBox` for direction (Forward / Reverse / PingPong / PingPongReverse)
  - "Add" button — pushes directly to `state.tags` (same pattern as the Win32
    implementation; no `add_tag` method needed). Duplicate tag names are permitted
    (same behavior as Win32 version; no deduplication check required).

- **Remove** button — removes `state.selected_tag` from `state.tags`.

- **Live preview**: small `Image` widget updated each frame. `AnimationState::tick` is called
  from within the sprite editor viewport closure. To drive per-frame timing, issue
  `ctx.request_repaint_after(Duration::from_millis(frame_duration_ms))` from inside that
  same viewport closure. When `frame_duration_ms < 16`, the main loop's 16 ms repaint
  already covers it, so no extra call is needed — but issuing a redundant shorter call is
  also harmless (eframe takes the minimum).

- **Save** button (disabled + tooltip "Assign idle and walk to enable Save" when
  `!state.is_saveable()`): calls `state.save_to_dir(sprites_dir)`.

- **Export…** button: `rfd::FileDialog` save dialog → `state.to_clean_json()` + copy PNG.

### Behavior mapping

`ComboBox` per tag, options = all 11 behavior slots ("— not set —" for unassigned optional
slots; `idle` and `walk` show "(required)" hint). Selecting a behavior calls
`set_behavior_mapping`, a module-level free function in `sprite_editor.rs` copied from the
Win32 implementation:

The 9 optional fields of `AnimTagMap` are: `run`, `sit`, `sleep`, `wake`, `grabbed`,
`petted`, `react`, `fall`, `thrown`.

```rust
fn set_behavior_mapping(tm: &mut AnimTagMap, behavior: &str, tag_name: &str) {
    // Clear any slot that already maps to this tag name (prevents duplicates).
    if tm.idle == tag_name  { tm.idle  = String::new(); }
    if tm.walk == tag_name  { tm.walk  = String::new(); }
    for opt in [
        &mut tm.run, &mut tm.sit, &mut tm.sleep, &mut tm.wake,
        &mut tm.grabbed, &mut tm.petted, &mut tm.react, &mut tm.fall, &mut tm.thrown,
    ] {
        if opt.as_deref() == Some(tag_name) { *opt = None; }
    }
    // Write the new mapping.
    match behavior {
        "idle"    => tm.idle    = tag_name.to_string(),
        "walk"    => tm.walk    = tag_name.to_string(),
        "run"     => tm.run     = Some(tag_name.to_string()),
        "sit"     => tm.sit     = Some(tag_name.to_string()),
        "sleep"   => tm.sleep   = Some(tag_name.to_string()),
        "wake"    => tm.wake    = Some(tag_name.to_string()),
        "grabbed" => tm.grabbed = Some(tag_name.to_string()),
        "petted"  => tm.petted  = Some(tag_name.to_string()),
        "react"   => tm.react   = Some(tag_name.to_string()),
        "fall"    => tm.fall    = Some(tag_name.to_string()),
        "thrown"  => tm.thrown  = Some(tag_name.to_string()),
        _         => { /* "— not set —": old slot already cleared, nothing written */ }
    }
}
```

When the user selects "— not set —" from the `ComboBox`, pass `behavior = "— not set —"`
(the exact display string) as the sentinel. The `_` match arm handles it: the tag's old
slot is still cleared (preventing the tag from appearing under its old behavior), and no
new slot is written — effectively unassigning the tag from all behaviors.

---

## Dependency Changes

```toml
[dependencies]
eframe = { version = "0.33", default-features = false, features = [
    "default_fonts", "wgpu",
] }
# egui is a transitive dep of eframe; no direct entry needed
rfd = "0.14"   # Rusty File Dialog — for Export… save dialog
```

`tray-icon` and `muda` are retained (system tray stays Win32-native).

Note: verify `rfd` version against `crates.io` at implementation time; the latest published
version may be newer than `"0.14"`.

---

## Testing

### Existing tests (unchanged)

130 tests pass after migration (156 current − 26 deleted from `test_config_dialog_e2e.rs`). They test `SpriteEditorState`, `sheet.rs`,
`animation.rs`, `behavior.rs`, `config`, and `window` — none of which change.

### New tests

No new automated tests for the egui UI layer itself (egui's immediate-mode rendering is
inherently visual; headless rendering is out of scope). Correctness is verified by:

1. Cargo build succeeds with no warnings.
2. All 130 remaining tests pass (`cargo test`).
3. Manual smoke test: open config dialog, change settings, verify live apply; open sprite
   editor, add tags, save, reload via config dialog.

---

## Out of Scope

- Per-frame duration editing in the sprite editor.
- Undo/redo.
- Animated GIF / video export.
- Migrating the system tray itself to egui (stays `tray-icon` + `muda`).
- Dark/light theme toggle (egui default dark theme used throughout).
- Sprite gallery thumbnails in the config dialog (gallery shows text names only).
