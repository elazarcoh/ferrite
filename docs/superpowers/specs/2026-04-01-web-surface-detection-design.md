# Ferrite Web — DOM Surface Detection Design

**Date:** 2026-04-01
**Scope:** Make the pet walk on DOM elements (buttons, cards, nav bars, etc.) in the ferrite-web wasm app

---

## Context

The Windows desktop version uses `find_floor()` (Win32 `EnumWindows` + `WindowFromPoint`) to find the top edge of real windows below the pet and land on them. The web version currently uses a fixed `floor_y = win_h − pet_h`, meaning the pet always walks on the viewport bottom. This feature ports the same concept to the web: scan DOM elements to discover walkable surfaces, let the pet land on their top edges, and fall off their edges while walking.

---

## Goals

- Pet lands on matching DOM elements when thrown or dropped onto them
- Pet falls off the edge of an element it's standing on (when it walks past the edge)
- Surface list is driven by a configurable CSS selector; the only hardcoded filters are viewport visibility and non-zero area
- Design is parameterised to support a future JS library extraction

---

## Non-Goals

- The pet does not smoothly slide onto surfaces it wasn't thrown onto (initial spawn always at viewport bottom)
- No visual highlight of surfaces
- No touch-surface interaction (pet doesn't respond to element click/hover events)
- No per-pixel or z-index based occlusion (first match at a given y wins)

---

## Architecture

### New module: `crates/ferrite-web/src/pet/surfaces.rs`

Web equivalent of `src/window/surfaces.rs`. Contains:

```
SurfaceConfig          — configurable discovery rules (selector + optional size thresholds)
WebSurface             — (left, right, top: i32) viewport-space rect of one element
SurfaceCache           — entries: Vec<WebSurface>, expires_at: f64 (perf.now ms)
refresh_if_expired()   — queries DOM, rebuilds cache if TTL elapsed
find_floor()           — same algorithm as Windows find_floor(); returns sprite-top floor y
```

### Core addition: `SMRunner::start_fall()`

A one-liner on `ferrite-core/src/sprite/sm_runner.rs` that transitions from any Named state into `ActiveState::Fall { vy: 0.0 }`. Called by the renderer's edge-fall check without reaching into runner internals.

### Modified: `crates/ferrite-web/src/pet/state.rs`

Add `surfaces: SurfaceCache` to `PetWebState`.

### Modified: `crates/ferrite-web/src/pet/renderer.rs`

Each frame:
1. Refresh surface cache if expired
2. Compute `floor_y` via `surfaces::find_floor` instead of `win_h − pet_h`
3. Pass `floor_y` to `runner.tick()`
4. Edge-fall check: if runner is in a Named state, not dragging, and `floor_y > s.y`, call `runner.start_fall()`

---

## `SurfaceConfig`

```rust
pub struct SurfaceConfig {
    /// CSS selector used to discover walkable elements.
    pub selector: &'static str,
    /// Minimum element width in CSS pixels. 0 = no minimum.
    pub min_width: i32,
    /// Minimum element height in CSS pixels. 0 = no minimum.
    pub min_height: i32,
    /// Maximum element height as a fraction of viewport height. 0.0 = no maximum.
    /// Example: 0.5 excludes full-page hero sections.
    pub max_height_ratio: f32,
}

pub const DEFAULT_CONFIG: SurfaceConfig = SurfaceConfig {
    selector: concat!(
        "button, [role=button], a[href], ",
        "input:not([type=hidden]), select, textarea, ",
        "nav, header, ",
        "[class*=card], [class*=box], [class*=panel]"
    ),
    min_width: 0,
    min_height: 0,
    max_height_ratio: 0.0,
};
```

`DEFAULT_CONFIG` applies only the non-arbitrary viewport-visibility filters. Users who want size thresholds set the fields explicitly.

---

## `WebSurface` and `SurfaceCache`

```rust
pub struct WebSurface {
    pub left:  i32,
    pub right: i32,
    pub top:   i32,
}

pub struct SurfaceCache {
    pub entries:    Vec<WebSurface>,
    pub expires_at: f64,  // performance.now() ms
}

impl Default for SurfaceCache {
    fn default() -> Self {
        Self { entries: Vec::new(), expires_at: 0.0 }
    }
}
```

---

## `refresh_if_expired`

```rust
pub fn refresh_if_expired(
    cache:  &mut SurfaceCache,
    doc:    &web_sys::Document,
    win_h:  i32,
    config: &SurfaceConfig,
) {
    let now = web_sys::window().unwrap().performance().unwrap().now();
    if now < cache.expires_at { return; }

    cache.entries.clear();
    let Ok(nodes) = doc.query_selector_all(config.selector) else { return };

    for i in 0..nodes.length() {
        let Some(el) = nodes.item(i) else { continue };
        let Ok(el) = el.dyn_into::<web_sys::Element>() else { continue };
        let r = el.get_bounding_client_rect();

        let top    = r.top()    as i32;
        let left   = r.left()   as i32;
        let right  = r.right()  as i32;
        let width  = r.width()  as i32;
        let height = r.height() as i32;

        // Viewport visibility: must have a top edge inside the visible viewport
        if top < 0 || top >= win_h { continue; }
        // Non-zero area
        if width <= 0 || height <= 0 { continue; }
        // User-configured size thresholds
        if config.min_width  > 0 && width  < config.min_width  { continue; }
        if config.min_height > 0 && height < config.min_height { continue; }
        if config.max_height_ratio > 0.0
            && height as f32 > win_h as f32 * config.max_height_ratio { continue; }

        cache.entries.push(WebSurface { left, right, top });
    }

    cache.expires_at = now + 250.0;
}
```

---

## `find_floor`

Mirrors the Windows `find_floor()` algorithm exactly. `pet_y` is the sprite-top y coordinate (`s.y`); `pet_bottom` is derived internally.

```rust
pub fn find_floor(
    pet_x:  i32,
    pet_y:  i32,   // sprite-top y (s.y)
    pet_w:  i32,
    pet_h:  i32,
    win_h:  i32,
    cache:  &SurfaceCache,
) -> i32 {
    let pet_left   = pet_x;
    let pet_right  = pet_x + pet_w;
    let pet_bottom = pet_y + pet_h;

    // Default floor is the viewport bottom (win_h - pet_h after subtraction below)
    let mut best = win_h;

    for s in &cache.entries {
        // Must overlap horizontally with the pet
        if pet_right <= s.left || pet_left >= s.right { continue; }
        // Surface top must be at or below the pet's current bottom
        if s.top < pet_bottom { continue; }
        // Nearest surface (smallest top y) wins
        if s.top < best { best = s.top; }
    }

    // Return sprite-top y-coordinate when resting on this surface,
    // matching the Windows find_floor() convention (returns best - pet_h).
    best - pet_h
}
```

---

## Renderer integration

```rust
// In tick_and_draw, after computing win_w / win_h and pet_w / pet_h:

let doc = web_sys::window().unwrap().document().unwrap();
surfaces::refresh_if_expired(&mut s.surfaces, &doc, win_h, &surfaces::DEFAULT_CONFIG);
let floor_y = surfaces::find_floor(s.x, s.y, pet_w, pet_h, win_h, &s.surfaces);

// Pass floor_y to physics (unchanged)
let tag_name = s.runner.tick(delta_ms, &mut s.x, &mut s.y,
                              win_w, pet_w, pet_h, floor_y, &s.sheet);

// Edge-fall: if pet walked off edge of surface, start falling
use ferrite_core::sprite::sm_runner::ActiveState;
if matches!(s.runner.active, ActiveState::Named(_)) && !s.is_dragging && floor_y > s.y {
    s.runner.start_fall();
}
```

---

## `SMRunner::start_fall` (ferrite-core)

```rust
/// Transition from any Named state into a free-fall.
/// Called by web renderer when the pet walks off the edge of a DOM surface.
pub fn start_fall(&mut self) {
    self.set_previous_from_current();
    self.active = ActiveState::Fall { vy: 0.0 };
    self.state_time_ms = 0;
}
```

---

## Cache TTL

250 ms, matching Windows `SurfaceCache` TTL. `getBoundingClientRect()` returns live viewport-relative coordinates, so scroll position is naturally captured on each refresh — no scroll-offset bookkeeping needed.

---

## File Map

| Action | Path | What changes |
|--------|------|--------------|
| Create | `crates/ferrite-web/src/pet/surfaces.rs` | New module: `SurfaceConfig`, `WebSurface`, `SurfaceCache`, `refresh_if_expired`, `find_floor` |
| Modify | `crates/ferrite-web/src/pet/mod.rs` | Add `pub mod surfaces;` |
| Modify | `crates/ferrite-web/src/pet/state.rs` | Add `surfaces: SurfaceCache` field |
| Modify | `crates/ferrite-web/src/pet/renderer.rs` | Use `surfaces::find_floor` for `floor_y`; edge-fall check |
| Modify | `crates/ferrite-core/src/sprite/sm_runner.rs` | Add `pub fn start_fall(&mut self)` |

---

## Testing

### Unit tests in `surfaces.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // find_floor(pet_x, pet_y, pet_w, pet_h, win_h, cache) -> i32

    #[test]
    fn find_floor_falls_back_to_viewport_when_no_surfaces() {
        let cache = SurfaceCache::default();
        // pet at y=900, win_h=1000, pet_h=80 → fallback = 1000-80 = 920
        assert_eq!(find_floor(100, 900, 64, 80, 1000, &cache), 920);
    }

    #[test]
    fn find_floor_returns_nearest_surface_below_pet() {
        let mut cache = SurfaceCache::default();
        cache.entries = vec![
            WebSurface { left: 0, right: 800, top: 600 },
            WebSurface { left: 0, right: 800, top: 400 },
        ];
        // pet at y=100, pet_h=80 → pet_bottom=180
        // surfaces at top=400 and top=600 both >= 180; nearest is 400
        // floor_y = 400 - 80 = 320
        assert_eq!(find_floor(100, 100, 64, 80, 1000, &cache), 320);
    }

    #[test]
    fn find_floor_ignores_surfaces_above_pet_bottom() {
        let mut cache = SurfaceCache::default();
        cache.entries = vec![
            WebSurface { left: 0, right: 800, top: 50 },
        ];
        // pet at y=100, pet_h=80 → pet_bottom=180; surface.top=50 < 180 → ignored
        assert_eq!(find_floor(100, 100, 64, 80, 1000, &cache), 920);
    }

    #[test]
    fn find_floor_ignores_non_overlapping_surfaces() {
        let mut cache = SurfaceCache::default();
        cache.entries = vec![
            WebSurface { left: 500, right: 800, top: 400 },
        ];
        // pet at x=100, pet_w=64 → pet_right=164; surface.left=500 > 164 → no overlap
        assert_eq!(find_floor(100, 100, 64, 80, 1000, &cache), 920);
    }
}
```

### Unit test in `sm_runner.rs`

```rust
#[test]
fn start_fall_transitions_named_to_fall() {
    let mut r = make_runner();
    assert!(matches!(&r.active, ActiveState::Named(_)));
    r.start_fall();
    assert!(matches!(&r.active, ActiveState::Fall { vy } if *vy == 0.0));
}
```

### Manual / browser verification

```bash
cd crates/ferrite-web
dx serve
# Open http://localhost:8080/ferrite/
```

1. The pet walks along the viewport bottom as before
2. Drag the pet to above a feature card and release — it lands on the card top edge
3. The pet walks on the card, then falls off the edge when it reaches the end
4. The pet falls to the next surface or the viewport bottom
5. Throw the pet at the nav bar — it lands on the nav bar and walks along it
6. Throw the pet at a Download button — it lands on the button
