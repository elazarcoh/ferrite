# Sprite Baseline Offset — Design Spec

**Date:** 2026-04-05

## Problem

Sprites from general-purpose art marketplaces are not designed to the "desktop pet bottom-flush" convention. The character's feet often sit above the bottom edge of the sprite grid, leaving empty transparent space below. With the current floor calculation (`floor_y = surface_top - pet_h`), such pets visibly float above every surface.

Concrete example: the "Ferris the Crab" sprite pack — walking frames have significant empty pink space below the crab's legs.

## Decision

Add a per-sprite `baseline_offset: u32` field (pixels from the bottom of the frame to the actual walking surface). This is a sprite property (not per-instance), so it lives in the sprite's JSON `meta` section and is edited in the sprite editor.

## Data

### Sprite JSON (`meta` section)

```json
{
  "meta": {
    "baseline_offset": 8
  }
}
```

Default: `0`. Fully backwards compatible — existing sprites without the field behave identically.

### `SpriteSheet` struct (`crates/ferrite-core/src/sprite/sheet.rs`)

Add `pub baseline_offset: u32`. Parse from `meta.baseline_offset` (default 0) alongside `parse_chromakey`.

### `SpriteEditorState` (`src/sprite/editor_state.rs`)

Add `pub baseline_offset: u32`. Load/save to/from JSON meta on import/export.

## Runtime

Single formula change in floor calculations:

```rust
// Before
floor_y = surface_top - pet_h

// After
floor_y = surface_top - pet_h + baseline_offset as i32
```

Applies to:
- `find_floor_info()` in `src/window/surfaces.rs`
- Virtual ground fallback in `src/app.rs`

`baseline_offset` is threaded from `SpriteSheet` through `PetInstance`.

## Editor UI (`src/tray/sprite_editor.rs`)

- Numeric drag input alongside Cols/Rows, clamped to `[0, frame_h - 1]`.
- In the spritesheet preview, draw a horizontal line at `frame_h - baseline_offset` pixels from the top, spanning the full sheet width. Visible across all frame rows simultaneously (one line per row in the grid).

## Verification

1. Import the "Ferris the Crab" sprite pack.
2. Set `baseline_offset` to the number of empty pixels below the crab's legs in the walking frames.
3. Confirm the crab lands flush on taskbar/window surfaces (no floating).
4. Confirm the baseline line is visible across all rows in the sprite editor preview.
5. Confirm `baseline_offset: 0` is identical to current behavior for all existing sprites.
