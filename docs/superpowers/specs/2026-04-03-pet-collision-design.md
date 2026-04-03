# Pet-to-Pet Collision — Design Spec

**Date:** 2026-04-03
**Status:** Approved

---

## Problem

Multiple pets currently ignore each other entirely — they phase through each other with no response. Multi-pet setups are a core differentiator; pets that interact feel alive, pets that phase through each other feel broken.

---

## Goal

Detect AABB overlap between pets and fire a `"collide"` SM interrupt on both pets when overlap begins. The SM author decides how each pet reacts; no physics response is hardcoded.

---

## Non-Goals

- No built-in physics bounce or separation — purely SM-driven
- No per-pixel collision — tight bbox (transparency-aware) is sufficient
- No z-ordering or depth — all pets are on the same plane
- No pet-to-window-surface collision (that is handled by `SurfaceCache`)

---

## Collision Event

A single `"collide"` interrupt event fires on both pets when overlap begins (edge-triggered). Four condition variables are available during interrupt condition evaluation:

| Variable | Type | Description |
|---|---|---|
| `collide_type` | string | Classification of the collision (see below) |
| `collide_vx` | f32 | Relative horizontal velocity (my vx − other vx) |
| `collide_vy` | f32 | Relative vertical velocity (my vy − other vy) |
| `collide_v` | f32 | Magnitude of relative velocity vector |

All four variables are `""` / `0.0` outside of an active `on_collide()` call.

### Collision types

| `collide_type` | Condition | Both pets get same type? |
|---|---|---|
| `"head_on"` | Dominant axis horizontal, pets moving toward each other | Yes |
| `"same_dir"` | Dominant axis horizontal, pets moving same direction | Yes |
| `"fell_on"` | I am above, dominant axis vertical, I am moving downward | No (see pair below) |
| `"landed_on"` | Other pet fell onto me from above | No (paired with `"fell_on"`) |
| `"hit_into_above"` | I am below, dominant axis vertical, I am moving upward | No (see pair below) |
| `"hit_from_below"` | Other pet hit me from below | No (paired with `"hit_into_above"`) |

Paired types are always assigned together — if pet A gets `"fell_on"`, pet B gets `"landed_on"`.

### Edge-triggering

The interrupt fires **once** when the overlap pair first appears. It does not re-fire while the pets remain overlapping. It fires again if the pets separate and overlap again.

`App` maintains `overlapping: HashSet<(String, String)>` (canonical key: `(min_id, max_id)`). Each frame the new overlap set is computed; pairs in the new set but not the previous set trigger `on_collide`.

---

## Tight Bounding Box

Collision uses per-frame transparency-aware bounding boxes rather than full frame rects. This prevents reactions to near-misses where only transparent pixels overlap.

### Precomputation

`TightBbox { dx: u32, dy: u32, w: u32, h: u32 }` is computed for every frame at spritesheet load time in `SpriteSheet::from_json_and_image`. It stores the offset and size of the tightest rectangle containing all non-transparent pixels within the frame rect.

A parallel `tight_bboxes: Vec<TightBbox>` is added to `SpriteSheet`. Lookup is O(1) by frame index.

### World-space query

```rust
pub fn tight_bbox(
    &self,
    frame_idx: usize,
    scale: u32,
    flip_h: bool,
) -> (i32, i32, u32, u32)   // (dx_px, dy_px, w_px, h_px) — offset from pet origin
```

When `flip_h` is true the x offset is mirrored: `dx_flipped = frame_w - (dx + tight_w)`.

---

## Detection Algorithm

Sweep-and-prune along the X axis: O(n log n) sort + O(n + k) scan where k = overlapping pairs.

1. After all pets are ticked, collect `(id, left, right, top, bottom, vx, vy)` from tight bboxes — one entry per pet
2. Sort by `left`
3. Maintain an active list; for each pet sweep left→right:
   - Evict entries from active whose `right < current.left`
   - Check current against every remaining active entry for Y overlap
   - Y-overlapping pairs are confirmed collisions
4. Compute canonical key `(min_id, max_id)` for each pair
5. Compare new set against `self.overlapping`; new pairs fire `on_collide` on both pets
6. `self.overlapping = new_set`

For the typical case of 2–5 pets this is faster than O(n²) brute force and scales cleanly to 20+ pets.

---

## Architecture

### New types — `crates/ferrite-core/src/sprite/sm_runner.rs`

```rust
pub struct CollideData {
    pub collide_type: String,   // one of the 6 type strings
    pub vx: f32,                // relative vx (my − other)
    pub vy: f32,                // relative vy (my − other)
    pub v: f32,                 // magnitude
}
```

`SMRunner` gains:
- `pending_collide: Option<CollideData>` (private field)
- `pub fn on_collide(&mut self, data: CollideData)` — sets pending, calls `self.interrupt("collide", None)`, clears pending
- `pub fn speed(&self) -> (f32, f32)` — returns `(vx, vy)` from current `ActiveState`; zero for non-physics states

`build_condition_vars()` reads `self.pending_collide` to populate the four new `ConditionVars` fields. Outside `on_collide()` all four are `""` / `0.0`.

### New fields — `crates/ferrite-core/src/sprite/sm_expr.rs`

```rust
pub collide_type: String,
pub collide_vx: f32,
pub collide_vy: f32,
pub collide_v: f32,
```

### New data — `crates/ferrite-core/src/sprite/sheet.rs`

```rust
pub struct TightBbox { pub dx: u32, pub dy: u32, pub w: u32, pub h: u32 }
// w == 0 && h == 0 means fully transparent — treated as non-collidable (never overlaps)
```

pub struct SpriteSheet {
    // ... existing fields ...
    pub tight_bboxes: Vec<TightBbox>,   // one per frame, computed at load time
}
```

New method: `pub fn tight_bbox(&self, frame_idx: usize, scale: u32, flip_h: bool) -> (i32, i32, u32, u32)`

### Changes — `src/app.rs`

```rust
pub struct App {
    // ... existing fields ...
    overlapping: HashSet<(String, String)>,
}
```

After the pet tick loop in `App::update()`, a collision pass:
1. Collects bounding box snapshots (avoids dual-mutable-borrow of `pets`)
2. Runs sweep-and-prune
3. Calls `on_collide` on each newly-overlapping pet pair

Helper `classify_collision(a, b) -> (String, String)` determines the collision type for each pet in the pair:
- **Dominant axis:** compare `|rel_vx|` vs `|rel_vy|`; if equal or both zero, default to horizontal
- **Horizontal:** if `rel_vx` and the pets are approaching (centers moving closer) → `"head_on"`; otherwise → `"same_dir"`
- **Vertical:** whichever pet has the lower center_y is "above"; if above pet has `vy > 0` (moving down) → `"fell_on"` / `"landed_on"`; if below pet has `vy < 0` (moving up) → `"hit_into_above"` / `"hit_from_below"`; if neither dominant vy → `"fell_on"` / `"landed_on"` (spatial default)
- `collide_vx` = my vx − other vx; positive means other pet is moving left relative to me

---

## SM Usage Examples

### Simple catch-all

```toml
[[states.wander.interrupts]]
event = "collide"
goto = "react"
```

### Direction-aware head-on bounce (using `pet_vx` to reverse direction)

```toml
[[states.wander.interrupts]]
event = "collide"
condition = "collide_type == \"head_on\" && pet_vx > 0"
goto = "bounce_left"

[[states.wander.interrupts]]
event = "collide"
condition = "collide_type == \"head_on\" && pet_vx < 0"
goto = "bounce_right"

[states.bounce_left]
action = "walk"
dir = "left"
duration = "400ms"
[[states.bounce_left.transitions]]
goto = "wander"

[states.bounce_right]
action = "walk"
dir = "right"
duration = "400ms"
[[states.bounce_right.transitions]]
goto = "wander"
```

### React only to hard impacts

```toml
[[states.wander.interrupts]]
event = "collide"
condition = "collide_v > 80"
goto = "stunned"
```

### React to being fallen on

```toml
[[states.idle.interrupts]]
event = "collide"
condition = "collide_type == \"landed_on\""
goto = "squish_react"

[states.squish_react]
action = "sit"
duration = "600ms"
[[states.squish_react.transitions]]
goto = "idle"
# No interrupts on squish_react — further collisions ignored during recovery
```

Note: states with no `"collide"` interrupt defined silently ignore the event — the SM's existing interrupt machinery handles this with no changes.

---

## Files Changed

| File | Change |
|---|---|
| `crates/ferrite-core/src/sprite/sheet.rs` | Add `TightBbox`, `tight_bboxes` field, `tight_bbox()` method, precompute in `from_json_and_image` |
| `crates/ferrite-core/src/sprite/sm_expr.rs` | Add 4 new `ConditionVars` fields |
| `crates/ferrite-core/src/sprite/sm_runner.rs` | Add `CollideData`, `pending_collide`, `on_collide()`, `speed()`, update `build_condition_vars()` |
| `src/app.rs` | Add `overlapping` field, sweep-and-prune collision pass, `classify_collision` helper |
| `tests/integration/test_collision.rs` | 7 integration tests (see below) |
| `tests/integration.rs` | Register `mod collision` |

No changes to `sm_format.rs`, `sm_compiler.rs`, or the TOML parser — `"collide"` is just another event string.

---

## Testing

### Unit tests (inline in `sheet.rs`)

- `tight_bbox_fully_opaque_frame` — frame with all pixels opaque → tight bbox equals full frame
- `tight_bbox_transparent_border` — frame with transparent border → tight bbox smaller than frame
- `tight_bbox_flip_h_mirrors_offset` — flipped bbox x-offset is `frame_w - (dx + tight_w)`
- `tight_bbox_all_transparent` — fully transparent frame → returns zero-size bbox at origin

### Integration tests (`tests/integration/test_collision.rs`)

1. **`head_on_collision_fires_and_transitions`** — two pets with `collide_type == "head_on"` interrupt, walking toward each other; assert both transition to target state
2. **`same_dir_collision_fires_interrupt`** — faster pet behind slower pet, overlaps; assert `collide_type == "same_dir"` on both
3. **`fell_on_collision_correct_type_per_pet`** — falling pet positioned above resting pet with overlap; assert top pet gets `"fell_on"`, bottom pet gets `"landed_on"`
4. **`edge_trigger_fires_once_not_repeatedly`** — pets remain overlapping across 3 simulated frames; assert interrupt fires exactly once (state changes only on first frame)
5. **`collide_v_available_in_condition`** — interrupt with `condition = "collide_v > 0"` fires; interrupt with `condition = "collide_v > 99999"` does not
6. **`no_interrupt_on_state_ignores_collide`** — pet in a state with no `"collide"` interrupt defined; overlap occurs; assert state does not change
7. **`separation_and_re_overlap_fires_again`** — pets overlap (fires), separate (no fire), overlap again (fires again); assert interrupt count == 2
