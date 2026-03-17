# Sprite Editor

**Date:** 2026-03-17
**Status:** Approved

## Goal

Add an in-app sprite editor that lets users define a uniform grid over a PNG, assign frame ranges to named tags, map tags to pet behavior states, and save or export the result as a standard Aseprite-compatible JSON + PNG pair.

---

## Overview

The editor opens as a separate non-modal Win32 window launched from the config dialog. It handles two cases:

1. **New sprite from PNG** — user supplies a plain PNG with no JSON; the editor lets them define the grid and tags from scratch.
2. **Edit existing sprite** — an already-installed sprite (custom JSON + PNG) is loaded; the user can adjust its grid, tags, and behavior mapping.

Embedded (built-in) sprites cannot be edited in-place; the app offers to create an editable copy in the AppData sprites directory first.

---

## Window Layout

Two-panel window (~760 × 520):

### Left — Sprite canvas

- Renders the full spritesheet PNG using GDI (`StretchDIBits`).
- Grid lines drawn over the image based on current rows × cols.
- Each defined tag is assigned a distinct color; its frames are highlighted with a semi-transparent fill.
- Selecting a tag in the right panel highlights it on the canvas.
- Below the canvas: **Rows** and **Cols** spinners (range 1–64 each) that redraw the grid live on `EN_KILLFOCUS`.

### Right — Controls

1. **Tags list** (owner-draw listbox) — each row: color swatch | tag name | frame range | behavior state dropdown.
2. **Add / Remove** buttons — Add opens an inline section: name field, from/to frame spinners, direction dropdown (Forward / Reverse / PingPong / PingPongReverse).
3. **Live preview** — animates the selected tag's frames using the same GDI preview as the config dialog (`AnimationState` + `WM_TIMER`).
4. **Save** and **Export…** buttons at the bottom. **Save is disabled until both `idle` and `walk` behavior states are assigned** (they are the two required fields of `AnimTagMap`). A label below the buttons shows "Assign idle and walk to enable Save" until the condition is met.

---

## Opening Flow

The config dialog gains two new controls next to the gallery listbox:

- **"Edit…"** button — enabled when a sprite is selected.
  - *Installed custom sprite*: opens editor with that sprite loaded.
  - *Embedded sprite*: prompts "This is a built-in sprite. Create an editable copy?" → copies PNG to AppData sprites dir, opens editor on the copy.
- **"New from PNG…"** button — opens a file picker; on selection opens the editor with a blank grid.

Only one sprite editor window may be open at a time. The editor HWND is stored in a `static AtomicPtr<c_void>` in `sprite_editor.rs`, set when the window is created and cleared (to null) in `WM_DESTROY`. Before opening a new editor window, the caller checks `IsWindow(stored_hwnd)`; if true, it calls `SetForegroundWindow` and returns without creating a second window.

---

## Data Model

### `src/sprite/editor_state.rs`

Pure Rust — no Win32. Holds the working state of the editor.

```rust
pub struct SpriteEditorState {
    pub png_path: PathBuf,
    pub image: RgbaImage,
    pub rows: u32,
    pub cols: u32,
    pub tags: Vec<EditorTag>,
    pub tag_map: AnimTagMap,        // behavior state → tag name
    pub selected_tag: Option<usize>,
}

pub struct EditorTag {
    pub name: String,
    pub from: usize,
    pub to: usize,
    pub direction: TagDirection,
    pub color: u32,                 // Win32 COLORREF for canvas highlight
}
```

`AnimTagMap` has two required fields (`idle`, `walk`) and nine optional fields (`run`, `sit`, `sleep`, `wake`, `grabbed`, `petted`, `react`, `fall`, `thrown`). The editor allows optional fields to remain unmapped (shown as "— not set —" in the behavior dropdown). A Save is only valid when `tag_map.idle` and `tag_map.walk` are both non-empty strings.

Key methods:

- `frame_rect(i: usize) -> (u32, u32, u32, u32)` — returns `(x, y, w, h)` for frame `i` given uniform grid.
- `frames_for_tag(tag_idx: usize) -> Vec<usize>` — frame indices covered by a tag.
- `is_saveable(&self) -> bool` — returns `true` iff `tag_map.idle` and `tag_map.walk` are non-empty.
- `to_json() -> Vec<u8>` — serialises to Aseprite array-format JSON including `myPetTagMap` (only populated mapping entries are written).
- `to_clean_json() -> Vec<u8>` — same but with `myPetTagMap` stripped (for export/sharing).
- `save_to_dir(dir: &Path) -> Result<()>` — writes JSON (`to_json()`) + copies PNG into `dir`, overwriting any existing file of the same name.

### Uniform grid formula

Given image dimensions `W × H`, `rows` rows and `cols` columns, frame `i`:

```
x = (i % cols) * (W / cols)
y = (i / cols) * (H / rows)
w = W / cols
h = H / rows
```

This is designed for extension: a future `FrameSlice` enum (`Uniform` vs `Manual`) can replace the computed values without changing consumers.

---

## JSON Format

The editor always emits **array-format** Aseprite JSON (simpler to generate; still fully accepted by Aseprite on import and by the app's `parse_frames`). The behavior mapping is stored in a non-standard `myPetTagMap` field inside `meta`, ignored by Aseprite and other tools.

Tag direction serialisation exactly matches the strings accepted by `sheet.rs`'s `parse_direction`:

| `TagDirection` variant | JSON string |
|---|---|
| `Forward` | `"forward"` |
| `Reverse` | `"reverse"` |
| `PingPong` | `"pingpong"` |
| `PingPongReverse` | `"pingpong_reverse"` |

Example output:

```json
{
  "frames": [
    { "frame": {"x":0,"y":0,"w":32,"h":32}, "duration": 100 },
    { "frame": {"x":32,"y":0,"w":32,"h":32}, "duration": 100 }
  ],
  "meta": {
    "frameTags": [
      { "name": "idle_loop", "from": 0, "to": 1, "direction": "pingpong" },
      { "name": "walk_cycle", "from": 2, "to": 5, "direction": "forward" }
    ],
    "myPetTagMap": {
      "idle":  "idle_loop",
      "walk":  "walk_cycle"
    }
  }
}
```

**Export…** produces a clean copy with `myPetTagMap` absent — pure Aseprite format, safe for community sharing.

---

## Save vs Export

| Action | Output | `myPetTagMap` | Destination | Collision |
|---|---|---|---|---|
| **Save** | JSON + PNG | ✅ included | AppData sprites dir (immediately visible in gallery) | Overwrites silently |
| **Export…** | JSON + PNG | ❌ stripped | User-chosen folder via file dialog | Standard OS save dialog handles it |

---

## `SpriteEditorCtx` (Win32 window state)

`src/tray/sprite_editor.rs` stores per-window state in `GWLP_USERDATA` using the same `Box::into_raw` / `Box::from_raw` ownership pattern as `config_window.rs`:

```rust
struct SpriteEditorCtx {
    state: SpriteEditorState,
    preview_sheet: Option<SpriteSheet>,
    preview_anim: AnimationState,
    dark_bg_brush: HBRUSH,
    ctrl_brush: HBRUSH,
}
```

- Allocated in `show_sprite_editor` via `Box::into_raw`; stored in `GWLP_USERDATA`.
- `WM_DESTROY` reclaims via `Box::from_raw`, drops brushes, and clears the global `EDITOR_HWND` atomic.

---

## `myPetTagMap` loading

`src/sprite/sheet.rs` gains:

```rust
pub fn load_with_tag_map(json: &[u8], png: &[u8])
    -> Result<(SpriteSheet, Option<AnimTagMap>)>
```

If `meta.myPetTagMap` is present, it is parsed leniently: optional fields that fail to parse as strings are silently ignored. If the field is absent, or if either `idle` or `walk` is missing or an empty string, `None` is returned (the entire map is dropped, not an error). When a valid map is returned, the app uses it as the default `AnimTagMap` for any new pet created with this sprite and no explicit override in `config.toml`.

The existing `load_embedded` is unchanged; callers that do not need the mapping are unaffected.

---

## Files Changed

| File | Change |
|---|---|
| `src/sprite/editor_state.rs` | New — pure-Rust editor state, `to_json`, `to_clean_json`, `save_to_dir`, `is_saveable` |
| `src/tray/sprite_editor.rs` | New — Win32 editor window (`show_sprite_editor`, `editor_wnd_proc`, `SpriteEditorCtx`, global `EDITOR_HWND`) |
| `src/sprite/sheet.rs` | Add `load_with_tag_map` |
| `src/tray/config_window.rs` | Add "Edit…" and "New from PNG…" buttons; call `show_sprite_editor` |
| `src/tray/mod.rs` | Re-export `sprite_editor` if needed |
| `tests/integration/test_sprite_editor.rs` | New — integration tests for `SpriteEditorState` |

---

## Testing

### Unit tests (in `src/sprite/editor_state.rs`)

| Test | Asserts |
|---|---|
| `frame_rect_uniform_grid` | 64×32, 2 cols 1 row → frame 0 = (0,0,32,32), frame 1 = (32,0,32,32) |
| `to_json_produces_valid_aseprite` | `to_json()` parses via `load_embedded` without error AND parses via `load_with_tag_map` returning `Some(AnimTagMap)` with matching fields |
| `clean_json_strips_tag_map` | `to_clean_json()` output has no `myPetTagMap` field; parses via `load_embedded` without error |
| `direction_round_trip` | All four `TagDirection` variants serialise to the correct JSON string and round-trip through `parse_direction` unchanged |
| `load_with_tag_map_round_trip` | Save JSON with full mapping, reload via `load_with_tag_map`, assert all populated `AnimTagMap` fields match |
| `load_with_tag_map_missing_required_drops_map` | JSON with `myPetTagMap` present but `walk` key absent → returns `None` for the map |
| `load_with_tag_map_empty_required_drops_map` | JSON with `myPetTagMap` where `idle` is `""` → returns `None` for the map |
| `load_with_tag_map_bad_optional_ignored` | JSON with `myPetTagMap` where `run` is `42` (not a string) → returns `Some(AnimTagMap)` with `idle` and `walk` correct, `run` unset |
| `tag_color_assignment` | 8+ tags each get a distinct color without panic |
| `is_saveable_requires_idle_and_walk` | `is_saveable()` returns false until both `idle` and `walk` are set; true after both are assigned |

### Integration test (`tests/integration/test_sprite_editor.rs`)

- Build `SpriteEditorState` from `test_pet.png`, 1×2 grid
- Define tag "idle" covering frames 0–1, map to `idle` behavior; define tag "walk" covering frames 0–1, map to `walk` behavior
- Assert `is_saveable()` returns true
- Call `save_to_dir(tempdir)` → JSON + PNG present
- Reload via `load_embedded` → parses cleanly
- Reload via `load_with_tag_map` → returns `Some(AnimTagMap)` with `idle = "idle"` and `walk = "walk"`

---

## Out of Scope

- Interactive frame selection by clicking the canvas (future: manual slicing)
- Per-frame duration editing
- Animated GIF / video export
- Multi-layer sprites
- Undo/redo
