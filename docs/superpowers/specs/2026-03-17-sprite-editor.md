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

1. **Tags list** (owner-draw listbox) — each row: color swatch | tag name | frame range | behavior state.
2. **Add / Remove** buttons — Add opens an inline section: name field, from/to frame spinners, direction dropdown (Forward / Reverse / PingPong / PingPongReverse).
3. **Live preview** — animates the selected tag's frames using the same GDI preview as the config dialog (`AnimationState` + `WM_TIMER`).
4. **Save** and **Export…** buttons at the bottom.

---

## Opening Flow

The config dialog gains two new controls next to the gallery listbox:

- **"Edit…"** button — enabled when a sprite is selected.
  - *Installed custom sprite*: opens editor with that sprite loaded.
  - *Embedded sprite*: prompts "This is a built-in sprite. Create an editable copy?" → copies PNG to AppData sprites dir, opens editor on the copy.
- **"New from PNG…"** button — opens a file picker; on selection opens the editor with a blank grid.

Only one sprite editor window may be open at a time. If one is already open (`IsWindow` check), `SetForegroundWindow` brings it to front instead of opening a second.

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

Key methods:

- `frame_rect(i: usize) -> (u32, u32, u32, u32)` — returns `(x, y, w, h)` for frame `i` given uniform grid.
- `frames_for_tag(tag_idx: usize) -> Vec<usize>` — frame indices covered by a tag.
- `to_json() -> Vec<u8>` — serialises to Aseprite-format JSON including `myPetTagMap`.
- `to_clean_json() -> Vec<u8>` — same but with `myPetTagMap` stripped (for export/sharing).
- `save_to_dir(dir: &Path) -> Result<()>` — writes JSON (`to_json()`) + copies PNG into `dir`.

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

The editor writes standard Aseprite JSON. The behavior mapping is stored in a non-standard `myPetTagMap` field inside `meta`, ignored by Aseprite and other tools:

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

| Action | Output | `myPetTagMap` | Destination |
|---|---|---|---|
| **Save** | JSON + PNG | ✅ included | AppData sprites dir (immediately visible in gallery) |
| **Export…** | JSON + PNG | ❌ stripped | User-chosen folder via file dialog |

---

## `myPetTagMap` loading

`src/sprite/sheet.rs` gains:

```rust
pub fn load_with_tag_map(json: &[u8], png: &[u8])
    -> Result<(SpriteSheet, Option<AnimTagMap>)>
```

If `meta.myPetTagMap` is present, it is parsed into an `AnimTagMap` and returned alongside the sheet. When a pet is created using this sprite and no explicit `AnimTagMap` override exists in `config.toml`, the app uses the returned mapping as the default.

The existing `load_embedded` is unchanged; callers that do not need the mapping are unaffected.

---

## Files Changed

| File | Change |
|---|---|
| `src/sprite/editor_state.rs` | New — pure-Rust editor state, `to_json`, `to_clean_json`, `save_to_dir` |
| `src/tray/sprite_editor.rs` | New — Win32 editor window (`show_sprite_editor`, `editor_wnd_proc`, `SpriteEditorCtx`) |
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
| `to_json_produces_valid_aseprite` | `to_json()` output parses via `load_embedded` without error |
| `clean_json_strips_tag_map` | `to_clean_json()` output has no `myPetTagMap` field |
| `load_with_tag_map_round_trip` | Save JSON with mapping, reload via `load_with_tag_map`, assert fields match |
| `tag_color_assignment` | 8+ tags each get a distinct color without panic |

### Integration test (`tests/integration/test_sprite_editor.rs`)

- Build `SpriteEditorState` from `test_pet.png`, 1×2 grid
- Define tag "idle" covering frames 0–1
- Call `save_to_dir(tempdir)` → JSON + PNG present
- Reload via `load_embedded` → parses cleanly

---

## Out of Scope

- Interactive frame selection by clicking the canvas (future: manual slicing)
- Per-frame duration editing
- Animated GIF / video export
- Multi-layer sprites
- Undo/redo
