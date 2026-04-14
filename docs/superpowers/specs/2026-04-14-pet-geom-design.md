# PetGeom Struct Design

## Context

When `baseline_offset` was added to sprites, the `min_surface` filter in `find_floor_info` was not updated to account for it. The bug: at landing, the pet's window bottom (`y + h`) extends `baseline_offset` pixels past the surface top, pushing `min_surface` above that surface and causing the filter to exclude it. The pet then falls through every elevated surface to the screen bottom.

The root cause is architectural: `baseline_offset` was a loose parameter threaded through 5-argument lists. Any derived formula that forgot to subtract it compiled and ran silently wrong.

**Fix already applied** (`min_surface = effective_bottom.max(h)`), but the class of bug remains possible. This spec closes it structurally.

## Goal

Make floor-related geometry formulas live in exactly one place so future features cannot silently introduce the same error.

## Design

### New: `PetGeom` value struct

**File:** `crates/ferrite-core/src/geometry.rs`

```rust
pub struct PetGeom {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub baseline_offset: i32,  // already scaled to pixels
}

impl PetGeom {
    /// y-coordinate of the pet's visual contact point with the surface.
    pub fn effective_bottom(&self) -> i32 {
        self.y + self.h - self.baseline_offset
    }

    /// Minimum surface top-y for a surface to be considered below the pet.
    pub fn min_surface_threshold(&self) -> i32 {
        self.effective_bottom().max(self.h)
    }

    /// Window top-y the pet should sit at when standing on `surface_top`.
    pub fn floor_landing_y(&self, surface_top: i32) -> i32 {
        surface_top - self.h + self.baseline_offset
    }
}
```

Exported from `ferrite-core` via `pub mod geometry` in `crates/ferrite-core/src/lib.rs`.

### Changed: `find_floor_info` / `find_floor` signatures

**File:** `src/window/surfaces.rs`

5 loose geometry params collapse into one `&PetGeom`:

```rust
pub fn find_floor_info(
    geom: &PetGeom,
    screen_w: i32,
    screen_h: i32,
    cache: &mut SurfaceCache,
) -> SurfaceHit

pub fn find_floor(
    geom: &PetGeom,
    screen_w: i32,
    screen_h: i32,
    cache: &mut SurfaceCache,
) -> i32
```

Inside `find_floor_info`, the key calculations become:

```rust
let pet_left  = geom.x;
let pet_right = geom.x + geom.w;
let min_surface = geom.min_surface_threshold();   // formula in one place
// ...
let floor_y = geom.floor_landing_y(best);         // formula in one place
```

`SMRunner::tick` is **unchanged** — it already receives a computed `floor_y: i32`.

### Changed: `app.rs` call sites

**File:** `src/app.rs`

Construct `PetGeom` once per tick; pass to both floor calls and to the `virtual_ground` calculation:

```rust
let geom = PetGeom {
    x: self.x, y: self.y,
    w: pet_w, h: pet_h,
    baseline_offset: baseline_offset_px,
};
let hit  = find_floor_info(&geom, screen_w, screen_h, cache);
// ...
let new_floor = find_floor(&geom, screen_w, screen_h, cache);
// virtual ground
let virtual_ground = geom.floor_landing_y(screen_h - 4);
```

## Tests

### Unit tests on `PetGeom` — `crates/ferrite-core/src/geometry.rs`

- `effective_bottom` returns `y + h - baseline_offset`
- `min_surface_threshold` at landing equals `surface_top` (regression invariant: this is the scenario that was broken)
- `floor_landing_y` round-trips: place pet at `floor_landing_y(surface_top)` → `effective_bottom() == surface_top`

### Update existing `surfaces.rs` unit tests

Pass `&PetGeom` instead of loose params. No logic change.

### New E2E scenario — `tests/e2e/test_surfaces_e2e.rs`

A pet with `baseline_offset > 0` (e.g. 16 px) placed above a real test HWND at a known position. Assert `find_floor_info` returns a floor within 2 px of the expected landing y. This is the test that would have caught the original bug.

## Files to Touch

| File | Change |
|------|--------|
| `crates/ferrite-core/src/geometry.rs` | **New** — `PetGeom` struct + unit tests |
| `crates/ferrite-core/src/lib.rs` | Add `pub mod geometry` |
| `src/window/surfaces.rs` | Change `find_floor_info` / `find_floor` signatures; update internal formulas; update existing unit tests |
| `src/app.rs` | Construct `PetGeom`; update 3 call sites + `virtual_ground` |
| `tests/e2e/test_surfaces_e2e.rs` | Add baseline_offset > 0 scenario |
| `benches/surfaces.rs` | Update to `&PetGeom` (compile fix, no logic change) |

## Out of Scope

- `SMRunner::tick` signature — it takes `floor_y: i32` which is already correct
- Web (`ferrite-web`) surface detection — uses a different DOM-based path with no `baseline_offset`
