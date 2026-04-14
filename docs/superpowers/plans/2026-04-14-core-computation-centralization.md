# Core Computation Centralization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move all engine computation into `ferrite-core` so that platform code (Win32 desktop, WASM webapp) only supplies inputs via named structs and receives outputs — eliminating silent divergence between platforms and preventing future bugs from scattered formulas.

**Architecture:** Six focused tasks, each independently compilable. Tasks 1–2 establish the platform-contract types (`PlatformBounds`, `EnvironmentSnapshot`). Tasks 3–4 move portable computation into core (`compute_flip`, collision API). Tasks 5–6 improve core's own internals (`Airborne` state unification, `AnimationState` field encapsulation). Each task ends with a green build and a commit.

**Tech Stack:** Rust, `ferrite-core` (no Win32 deps), `windows-sys` for Win32, `egui` in webapp.

---

## File Map

| File | Action |
|------|--------|
| `crates/ferrite-core/src/geometry.rs` | Modify — add `PlatformBounds` struct + test |
| `crates/ferrite-core/src/sprite/sm_runner.rs` | Modify — `tick(&PlatformBounds)`, `EnvironmentSnapshot`+`update_env`, `compute_flip()`, `Airborne` variant |
| `crates/ferrite-core/src/sprite/collision.rs` | **Create** — portable collision detection API |
| `crates/ferrite-core/src/sprite/animation.rs` | Modify — `current_tag` private + `current_tag()` getter |
| `crates/ferrite-core/src/sprite/mod.rs` | Modify — export `pub mod collision` |
| `src/window/surfaces.rs` | Modify — `find_floor_info`/`find_floor` accept `&PlatformBounds` |
| `src/app.rs` | Modify — construct `PlatformBounds`+`EnvironmentSnapshot`; delegate `compute_flip`; use core collision API; update `Airborne` match arms |
| `crates/ferrite-webapp/src/simulation.rs` | Modify — use `PlatformBounds`, `EnvironmentSnapshot`, `compute_flip`, `current_tag()` getter; apply flip in render |
| `tests/e2e/test_surfaces_e2e.rs` | Modify — `find_floor` calls take `&PlatformBounds` |
| `benches/surfaces.rs` | Modify — `find_floor` calls take `&PlatformBounds` |

---

## Task 1: `PlatformBounds` — centralise screen geometry

**What this fixes:** `screen_w` and `screen_h` are passed as loose `i32` parameters through `find_floor_info` and `SMRunner::tick`. The virtual-ground formula `screen_h - 4` is duplicated in `surfaces.rs` and `app.rs`. The webapp hardcodes `SIM_FLOOR_Y` in a way that can't be expressed consistently. `PlatformBounds` becomes the single named type that the platform provides for screen geometry.

**Files:**
- Modify: `crates/ferrite-core/src/geometry.rs`
- Modify: `crates/ferrite-core/src/sprite/sm_runner.rs`
- Modify: `src/window/surfaces.rs`
- Modify: `src/app.rs`
- Modify: `crates/ferrite-webapp/src/simulation.rs`
- Modify: `tests/e2e/test_surfaces_e2e.rs`
- Modify: `benches/surfaces.rs`

- [ ] **Step 1: Add `PlatformBounds` to `crates/ferrite-core/src/geometry.rs` with a failing test**

Append after the existing `PetGeom` impl block:

```rust
/// Screen-level geometry provided by the platform.
///
/// On Win32:  `screen_w = GetSystemMetrics(SM_CXSCREEN)`,
///            `screen_h = GetSystemMetrics(SM_CYSCREEN)`.
/// On WASM:   set `screen_h = SIM_FLOOR_Y + 4` so `virtual_ground_y()` == `SIM_FLOOR_Y`.
#[derive(Debug, Clone, Copy)]
pub struct PlatformBounds {
    pub screen_w: i32,
    pub screen_h: i32,
}

impl PlatformBounds {
    /// Y-coordinate of the virtual ground — the fallback floor used when no
    /// real surface is detected below the pet.
    /// 4 px above the raw screen bottom avoids pixel-level clipping at the taskbar.
    pub fn virtual_ground_y(&self) -> i32 {
        self.screen_h - 4
    }
}
```

In the existing `#[cfg(test)]` block in `geometry.rs`, add:

```rust
#[test]
fn virtual_ground_y_is_four_px_above_bottom() {
    let b = PlatformBounds { screen_w: 1920, screen_h: 1080 };
    assert_eq!(b.virtual_ground_y(), 1076);
}

#[test]
fn virtual_ground_y_for_sim_floor_500() {
    // Webapp: screen_h = SIM_FLOOR_Y + 4 = 504
    let b = PlatformBounds { screen_w: 800, screen_h: 504 };
    assert_eq!(b.virtual_ground_y(), 500);
}
```

- [ ] **Step 2: Run tests — expect FAIL (PlatformBounds not yet in module)**

```
cargo test -p ferrite-core virtual_ground_y
```

Expected: compile error — no `PlatformBounds` in scope (test is in same file so it should compile; if geometry.rs didn't have tests before, just run `cargo build -p ferrite-core` and confirm it compiles, then run the new tests).

- [ ] **Step 3: Confirm tests pass after adding the struct**

```
cargo test -p ferrite-core virtual_ground_y
```

Expected:
```
test geometry::tests::virtual_ground_y_is_four_px_above_bottom ... ok
test geometry::tests::virtual_ground_y_for_sim_floor_500 ... ok
```

- [ ] **Step 4: Update `src/window/surfaces.rs` — replace `screen_w, screen_h` with `&PlatformBounds`**

Change the import line at the top:

```rust
use ferrite_core::geometry::{PetGeom, PlatformBounds};
```

Replace the `find_floor_info` function signature and body. Replace:

```rust
pub fn find_floor_info(
    geom: &PetGeom,
    screen_w: i32,
    screen_h: i32,
    cache: &mut SurfaceCache,
) -> SurfaceHit {
    // Refresh cache if expired.
    if cache.is_expired() {
        let mut fill = FillState { screen_w, entries: Vec::new() };
```

with:

```rust
pub fn find_floor_info(
    geom: &PetGeom,
    bounds: &PlatformBounds,
    cache: &mut SurfaceCache,
) -> SurfaceHit {
    // Refresh cache if expired.
    if cache.is_expired() {
        let mut fill = FillState { screen_w: bounds.screen_w, entries: Vec::new() };
```

And replace:

```rust
    let virtual_ground_top = screen_h - 4;
```

with:

```rust
    let virtual_ground_top = bounds.virtual_ground_y();
```

Replace `find_floor` signature and body:

```rust
pub fn find_floor(
    geom: &PetGeom,
    bounds: &PlatformBounds,
    cache: &mut SurfaceCache,
) -> i32 {
    find_floor_info(geom, bounds, cache).floor_y
}
```

Update the unit tests inside `surfaces.rs` — replace every `find_floor(&geom, screen_w, screen_h, &mut cache)` call with `find_floor(&geom, &PlatformBounds { screen_w, screen_h }, &mut cache)` and update the floor bounds check from `floor < screen_h` to `floor < bounds.screen_h`:

Full updated test block:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::geometry::{PetGeom, PlatformBounds};

    #[test]
    fn surface_cache_default_is_expired() {
        let cache = SurfaceCache::default();
        assert!(cache.is_expired(), "default cache must be expired so first call always re-fetches");
    }

    #[test]
    fn baseline_offset_does_not_filter_landing_surface() {
        let surface_top = 1040i32;
        let template = PetGeom { x: 500, y: 0, w: 137, h: 137, baseline_offset: 29 };
        let at_landing = PetGeom { y: template.floor_landing_y(surface_top), ..template };
        assert!(
            at_landing.min_surface_threshold() <= surface_top,
            "surface_top ({surface_top}) must pass min_surface filter at landing (threshold={})",
            at_landing.min_surface_threshold()
        );
    }

    #[test]
    fn surface_cache_find_floor_returns_plausible_value() {
        let mut cache = SurfaceCache::default();
        let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) };
        let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) };
        let bounds = PlatformBounds { screen_w, screen_h };
        let geom = PetGeom { x: 0, y: 0, w: 32, h: 32, baseline_offset: 0 };
        let floor = find_floor(&geom, &bounds, &mut cache);
        assert!(floor >= 0, "floor y must be non-negative, got {floor}");
        assert!(floor < bounds.screen_h, "floor y must be above screen bottom, got {floor}");
    }

    #[test]
    fn surface_cache_warm_returns_same_result() {
        let mut cache = SurfaceCache::default();
        let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) };
        let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) };
        let bounds = PlatformBounds { screen_w, screen_h };
        let geom = PetGeom { x: 100, y: 0, w: 32, h: 32, baseline_offset: 0 };
        let floor1 = find_floor(&geom, &bounds, &mut cache);
        assert!(!cache.is_expired(), "cache must be warm after first call");
        let floor2 = find_floor(&geom, &bounds, &mut cache);
        assert_eq!(floor1, floor2, "warm cache must return same floor as cold call");
    }
}
```

- [ ] **Step 5: Update `crates/ferrite-core/src/sprite/sm_runner.rs` — `tick` takes `&PlatformBounds`**

Add import at top of sm_runner.rs (alongside existing imports):

```rust
use crate::geometry::PlatformBounds;
```

Replace the `tick` public function signature:

```rust
#[allow(clippy::too_many_arguments)]
pub fn tick(
    &mut self,
    delta_ms: u32,
    x: &mut i32,
    y: &mut i32,
    bounds: &PlatformBounds,
    pet_w: i32,
    pet_h: i32,
    floor_y: i32,
    sheet: &SpriteSheet,
) -> &str {
```

Inside `tick`, replace:

```rust
        self.last_vars.screen_w = screen_w as f32;
```

with:

```rust
        self.last_vars.screen_w = bounds.screen_w as f32;
        self.last_vars.screen_h = bounds.screen_h as f32;
```

(The `screen_h` line is new — previously `screen_h` was set via `update_env_vars`; it now comes from `PlatformBounds`. Remove the `screen_h` parameter from `update_env_vars` in the same edit — see below.)

Replace the `execute_action` call:

```rust
        self.execute_action(dt, x, y, bounds, pet_w, pet_h, floor_y);
```

Replace the `try_transitions` call:

```rust
        self.try_transitions(bounds, pet_w, pet_h, floor_y);
```

Update `execute_action` private function signature and all internal uses of `screen_w`:

```rust
#[allow(clippy::too_many_arguments)]
fn execute_action(&mut self, dt: f32, x: &mut i32, y: &mut i32, bounds: &PlatformBounds, pet_w: i32, _pet_h: i32, floor_y: i32) {
```

Inside `execute_action`, replace every `screen_w` with `bounds.screen_w`.

Update `try_transitions` private function signature and all internal uses of `screen_w`:

```rust
fn try_transitions(&mut self, bounds: &PlatformBounds, pet_w: i32, pet_h: i32, floor_y: i32) {
```

Inside `try_transitions`, replace every `screen_w` with `bounds.screen_w`.

Also remove `screen_h` from `update_env_vars` (it is now set by `tick` via `PlatformBounds`). Change:

```rust
    #[allow(clippy::too_many_arguments)]
    pub fn update_env_vars(
        &mut self,
        cursor_dist: f32,
        hour: u32,
        focused_app: String,
        screen_h: f32,
        pet_count: u32,
        other_pet_dist: f32,
        surface_w: f32,
        surface_label: String,
    ) {
        self.last_vars.cursor_dist = cursor_dist;
        self.last_vars.hour = hour;
        self.last_vars.focused_app = focused_app;
        self.last_vars.screen_h = screen_h;
        self.last_vars.pet_count = pet_count;
        self.last_vars.other_pet_dist = other_pet_dist;
        self.last_vars.surface_w = surface_w;
        self.last_vars.surface_label = surface_label;
    }
```

to:

```rust
    #[allow(clippy::too_many_arguments)]
    pub fn update_env_vars(
        &mut self,
        cursor_dist: f32,
        hour: u32,
        focused_app: String,
        pet_count: u32,
        other_pet_dist: f32,
        surface_w: f32,
        surface_label: String,
    ) {
        self.last_vars.cursor_dist = cursor_dist;
        self.last_vars.hour = hour;
        self.last_vars.focused_app = focused_app;
        // screen_h is set by tick() via PlatformBounds — not here.
        self.last_vars.pet_count = pet_count;
        self.last_vars.other_pet_dist = other_pet_dist;
        self.last_vars.surface_w = surface_w;
        self.last_vars.surface_label = surface_label;
    }
```

- [ ] **Step 6: Update `src/app.rs`**

Add import alongside existing `PetGeom` import:

```rust
use ferrite_core::geometry::{PetGeom, PlatformBounds};
```

In `PetInstance::tick`, construct `PlatformBounds` once after the `screen_w`/`screen_h` let-bindings:

```rust
        let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        let bounds = PlatformBounds { screen_w, screen_h };
```

Replace both `find_floor_info` and `find_floor` calls:

```rust
        let hit = crate::window::surfaces::find_floor_info(&geom, &bounds, cache);
```

```rust
            let new_floor = crate::window::surfaces::find_floor(&geom_post_tick, &bounds, cache);
```

Replace:

```rust
        let virtual_ground = geom_post_tick.floor_landing_y(screen_h - 4);
```

with:

```rust
        let virtual_ground = geom_post_tick.floor_landing_y(bounds.virtual_ground_y());
```

Replace the `runner.tick` call (the `screen_w` arg becomes `&bounds`):

```rust
        let tag = self.runner.tick(
            delta_ms,
            &mut self.x,
            &mut self.y,
            &bounds,
            pet_w,
            pet_h,
            floor_y,
            &self.sheet,
        );
```

Remove `screen_h` from the `update_env_vars` call in `App::update()`. Change:

```rust
                pet.runner.update_env_vars(
                    cursor_dist,
                    hour,
                    focused_app.clone(),
                    screen_h,
                    pet_count,
                    other_pet_dist,
                    pet.last_surface_hit.surface_w,
                    pet.last_surface_hit.surface_label.clone(),
                );
```

to:

```rust
                pet.runner.update_env_vars(
                    cursor_dist,
                    hour,
                    focused_app.clone(),
                    pet_count,
                    other_pet_dist,
                    pet.last_surface_hit.surface_w,
                    pet.last_surface_hit.surface_label.clone(),
                );
```

Also remove `let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) } as f32;` in `App::update()` — it was only used for `update_env_vars`. (Make sure `screen_h` isn't used elsewhere in that function first; if it is, keep the binding.)

- [ ] **Step 7: Update `crates/ferrite-webapp/src/simulation.rs`**

Add import at top:

```rust
use ferrite_core::geometry::PlatformBounds;
```

In `SimulationState::tick`, construct `PlatformBounds`:

```rust
        let bounds = PlatformBounds {
            screen_w: SIM_SCREEN_W,
            screen_h: SIM_FLOOR_Y + 4,  // virtual_ground_y() == SIM_FLOOR_Y
        };
```

Replace the `pet.sm.tick` call:

```rust
            let tag = pet.sm.tick(
                delta_ms,
                &mut pet.x,
                &mut pet.y,
                &bounds,
                pet_w,
                pet_h,
                SIM_FLOOR_Y,
                &pet.sheet,
            );
```

Remove `screen_h` from `update_env_vars` call:

```rust
            pet.sm.update_env_vars(
                f32::MAX,       // cursor_dist: no cursor in headless web sim
                0,              // hour
                String::new(),  // focused_app
                pet_count,
                f32::MAX,       // other_pet_dist
                SIM_SCREEN_W as f32, // surface_w
                String::new(),  // surface_label
            );
```

- [ ] **Step 8: Update `tests/e2e/test_surfaces_e2e.rs`**

Add `PlatformBounds` import:

```rust
use ferrite_core::geometry::{PetGeom, PlatformBounds};
```

In every test that calls `find_floor(...)`, replace the signature. There are 5 calls total, each with the pattern `find_floor(&geom, screen_w, screen_h, &mut cache)`. Replace all with:

```rust
find_floor(&geom, &PlatformBounds { screen_w, screen_h }, &mut cache)
```

- [ ] **Step 9: Update `benches/surfaces.rs`**

Add `PlatformBounds` import:

```rust
use ferrite_core::geometry::{PetGeom, PlatformBounds};
```

Construct bounds once per bench function and use it:

```rust
fn bench_find_floor_cold(c: &mut Criterion) {
    let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) };
    let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) };
    let bounds = PlatformBounds { screen_w, screen_h };
    let geom = PetGeom { x: 100, y: 0, w: 32, h: 32, baseline_offset: 0 };
    // Reduced sample size: EnumWindows is a blocking syscall with OS scheduling jitter.
    c.bench_function("find_floor_cold", |b| {
        b.iter(|| {
            let mut cache = SurfaceCache::default();
            find_floor(&geom, &bounds, &mut cache)
        })
    });
}

fn bench_find_floor_cached(c: &mut Criterion) {
    let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) };
    let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) };
    let bounds = PlatformBounds { screen_w, screen_h };
    let geom = PetGeom { x: 100, y: 0, w: 32, h: 32, baseline_offset: 0 };
    let mut cache = SurfaceCache::default();
    find_floor(&geom, &bounds, &mut cache);
    c.bench_function("find_floor_cached", |b| {
        b.iter(|| find_floor(&geom, &bounds, &mut cache))
    });
}
```

- [ ] **Step 10: Build and test**

```
cargo build
cargo test
```

Expected: all tests pass, no warnings promoted to errors.

- [ ] **Step 11: Commit**

```
git add crates/ferrite-core/src/geometry.rs \
        crates/ferrite-core/src/sprite/sm_runner.rs \
        src/window/surfaces.rs \
        src/app.rs \
        crates/ferrite-webapp/src/simulation.rs \
        tests/e2e/test_surfaces_e2e.rs \
        benches/surfaces.rs
git commit -m "refactor(core): introduce PlatformBounds; thread screen geometry through surfaces and SMRunner::tick"
```

---

## Task 2: `EnvironmentSnapshot` — formalise the SM input contract

**What this fixes:** `update_env_vars` takes 7 loose parameters. Adding a new condition variable (e.g., `is_raining`) requires changing the function signature, all call sites, and `ConditionVars` struct simultaneously. The webapp hardcodes several values with no visible contract. `EnvironmentSnapshot` makes the platform's obligations explicit: fields with doc comments, `Default` impl for headless/minimal platforms.

**Files:**
- Modify: `crates/ferrite-core/src/sprite/sm_runner.rs`
- Modify: `src/app.rs`
- Modify: `crates/ferrite-webapp/src/simulation.rs`

- [ ] **Step 1: Add `EnvironmentSnapshot` to `sm_runner.rs` and replace `update_env_vars`**

Add the struct above `SMRunner`:

```rust
/// All externally-sourced inputs the state machine can observe each frame.
///
/// Construct once per pet per frame in the platform layer; pass to
/// [`SMRunner::update_env`]. Use `EnvironmentSnapshot::default()` as a
/// starting point and override only the fields your platform can observe.
///
/// `screen_w` and `screen_h` are NOT here — they come via `PlatformBounds`
/// in `SMRunner::tick`.
#[derive(Debug, Clone)]
pub struct EnvironmentSnapshot {
    /// Distance from the cursor to the pet's center, in screen pixels.
    /// Set to `f32::MAX` when there is no cursor (headless / WASM sim).
    pub cursor_dist: f32,
    /// Local hour of day [0, 23].
    pub hour: u32,
    /// Title of the foreground window, or empty string.
    pub focused_app: String,
    /// Total number of live pets on screen (including this one).
    pub pet_count: u32,
    /// Distance to the nearest other pet's center, in screen pixels.
    /// Set to `f32::MAX` when this is the only pet.
    pub other_pet_dist: f32,
    /// Width of the surface the pet is standing on, in screen pixels.
    /// 0.0 when on virtual ground.
    pub surface_w: f32,
    /// Classification of the current surface: `"taskbar"`, `"window"`, or `""`.
    pub surface_label: String,
}

impl Default for EnvironmentSnapshot {
    fn default() -> Self {
        Self {
            cursor_dist: f32::MAX,
            hour: 0,
            focused_app: String::new(),
            pet_count: 1,
            other_pet_dist: f32::MAX,
            surface_w: 0.0,
            surface_label: String::new(),
        }
    }
}
```

Replace `update_env_vars` with `update_env`:

```rust
    /// Apply a platform-sourced environment snapshot.
    /// Called once per pet per frame, after `tick`.
    pub fn update_env(&mut self, env: EnvironmentSnapshot) {
        self.last_vars.cursor_dist   = env.cursor_dist;
        self.last_vars.hour          = env.hour;
        self.last_vars.focused_app   = env.focused_app;
        self.last_vars.pet_count     = env.pet_count;
        self.last_vars.other_pet_dist = env.other_pet_dist;
        self.last_vars.surface_w     = env.surface_w;
        self.last_vars.surface_label = env.surface_label;
        // screen_w / screen_h are set by tick() via PlatformBounds.
    }
```

Delete the old `update_env_vars` function entirely.

- [ ] **Step 2: Update `src/app.rs`**

Add import:

```rust
use ferrite_core::sprite::sm_runner::EnvironmentSnapshot;
```

Replace the `pet.runner.update_env_vars(...)` call in `App::update()`:

```rust
                pet.runner.update_env(EnvironmentSnapshot {
                    cursor_dist,
                    hour,
                    focused_app: focused_app.clone(),
                    pet_count,
                    other_pet_dist,
                    surface_w: pet.last_surface_hit.surface_w,
                    surface_label: pet.last_surface_hit.surface_label.clone(),
                });
```

- [ ] **Step 3: Update `crates/ferrite-webapp/src/simulation.rs`**

Add import:

```rust
use ferrite_core::sprite::sm_runner::EnvironmentSnapshot;
```

Replace the `pet.sm.update_env_vars(...)` call:

```rust
            pet.sm.update_env(EnvironmentSnapshot {
                pet_count,
                surface_w: SIM_SCREEN_W as f32,
                // No cursor, no app focus, no time-of-day in headless sim.
                ..EnvironmentSnapshot::default()
            });
```

- [ ] **Step 4: Build and test**

```
cargo build
cargo test
```

Expected: clean. The `#[allow(clippy::too_many_arguments)]` attribute on the old `update_env_vars` should be gone; verify clippy passes too:

```
cargo clippy -- -D warnings -A dead-code
```

- [ ] **Step 5: Commit**

```
git add crates/ferrite-core/src/sprite/sm_runner.rs src/app.rs crates/ferrite-webapp/src/simulation.rs
git commit -m "refactor(core): replace update_env_vars(7 args) with EnvironmentSnapshot struct"
```

---

## Task 3: `SMRunner::compute_flip` — move sprite-flip logic to core

**What this fixes:** `compute_flip(runner, sheet)` is a free function in `src/app.rs` (desktop-only). It reads only `SMRunner` state and `SpriteSheet` data — zero Win32 deps. The webapp never calls it, so all webapp pets always face right regardless of the SM's `Facing` state. Moving it to `SMRunner` as a method makes the formula available to all platforms.

**Files:**
- Modify: `crates/ferrite-core/src/sprite/sm_runner.rs`
- Modify: `src/app.rs`
- Modify: `crates/ferrite-webapp/src/simulation.rs`

- [ ] **Step 1: Add `compute_flip` to `SMRunner` in `sm_runner.rs`**

Add after the `current_facing` method:

```rust
    /// Returns `true` if the current animation frame should be rendered
    /// flipped horizontally.
    ///
    /// Sprites are authored facing **right** by default (`flip_h = false`).
    /// A tag with `flip_h = true` is authored facing **left**; the logic
    /// inverts so the result still tracks the `Facing` direction correctly.
    pub fn compute_flip(&self, sheet: &SpriteSheet) -> bool {
        let tag_name = self.current_state_name();
        let tag_flip_h = sheet.tags.iter()
            .find(|t| t.name == tag_name)
            .map(|t| t.flip_h)
            .unwrap_or(false);
        match self.facing {
            Facing::Right => tag_flip_h,
            Facing::Left  => !tag_flip_h,
        }
    }
```

- [ ] **Step 2: Add a unit test for `compute_flip` in `sm_runner.rs`**

At the bottom of `sm_runner.rs`, add or extend a `#[cfg(test)]` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprite::sheet::load_embedded;

    fn test_sheet() -> SpriteSheet {
        load_embedded(
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/test_pet.json")),
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/test_pet.png")),
        )
        .unwrap()
    }

    #[test]
    fn compute_flip_right_facing_no_tag_flip() {
        let mut runner = SMRunner::new(load_default_sm(), 100.0);
        runner.facing = Facing::Right;
        let sheet = test_sheet();
        // Default tag has flip_h=false; facing right → no flip.
        assert!(!runner.compute_flip(&sheet));
    }

    #[test]
    fn compute_flip_left_facing_no_tag_flip() {
        let mut runner = SMRunner::new(load_default_sm(), 100.0);
        runner.facing = Facing::Left;
        let sheet = test_sheet();
        // Default tag has flip_h=false; facing left → flip.
        assert!(runner.compute_flip(&sheet));
    }
}
```

- [ ] **Step 3: Run core tests**

```
cargo test -p ferrite-core compute_flip
```

Expected:
```
test sprite::sm_runner::tests::compute_flip_right_facing_no_tag_flip ... ok
test sprite::sm_runner::tests::compute_flip_left_facing_no_tag_flip ... ok
```

- [ ] **Step 4: Update `src/app.rs` — delegate to the new method**

In `PetInstance::compute_flip`, replace the body:

```rust
    pub fn compute_flip(&self) -> bool {
        self.runner.compute_flip(&self.sheet)
    }
```

Delete the free function `compute_flip(runner: &SMRunner, sheet: &SpriteSheet) -> bool` at the bottom of `app.rs` (roughly lines 939–951). The method now calls the core impl directly.

- [ ] **Step 5: Update `crates/ferrite-webapp/src/simulation.rs` — apply flip in render**

In the render loop inside `SimulationState::render`, after computing `abs_frame`, add:

```rust
                let flip = pet.sm.compute_flip(&pet.sheet);
```

Then replace the UV rect that is currently:

```rust
                    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
```

with:

```rust
                    let (uv_x0, uv_x1) = if flip { (1.0_f32, 0.0_f32) } else { (0.0_f32, 1.0_f32) };
                    let uv = egui::Rect::from_min_max(
                        egui::pos2(uv_x0, 0.0),
                        egui::pos2(uv_x1, 1.0),
                    );
```

- [ ] **Step 6: Build and test**

```
cargo build
cargo test
```

- [ ] **Step 7: Commit**

```
git add crates/ferrite-core/src/sprite/sm_runner.rs src/app.rs crates/ferrite-webapp/src/simulation.rs
git commit -m "refactor(core): move compute_flip() to SMRunner; webapp now renders facing direction correctly"
```

---

## Task 4: Collision API in `ferrite-core`

**What this fixes:** `PetBox`, `collect_boxes`, `classify_collision`, `canonical_key`, and `make_collide_data` are five private helpers in `src/app.rs`. They are pure geometry with no Win32 deps. If the webapp ever adds multi-pet interaction, they would need to be duplicated. Moving them to `ferrite_core::sprite::collision` makes the API portable and testable in isolation.

**Files:**
- Create: `crates/ferrite-core/src/sprite/collision.rs`
- Modify: `crates/ferrite-core/src/sprite/mod.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Write the failing tests first — create `collision.rs` with tests only**

Create `crates/ferrite-core/src/sprite/collision.rs`:

```rust
use std::collections::HashSet;
use crate::sprite::sm_runner::CollideData;

/// The collision geometry for one pet in a given frame.
/// Build this from `SpriteSheet::tight_bbox` output plus the SM runner's
/// current velocity (`SMRunner::speed()`).
#[derive(Debug, Clone)]
pub struct Collidable {
    pub id: String,
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
    /// Vertical midpoint of the bounding box.
    pub center_y: i32,
    pub vx: f32,
    pub vy: f32,
}

/// Both sides of a freshly-started collision.
pub struct CollisionPair {
    pub id_a: String,
    pub data_a: CollideData,
    pub id_b: String,
    pub data_b: CollideData,
}

/// Returns the sorted canonical key for a pair of pet IDs.
/// Used by callers to maintain the `previously_overlapping` set.
pub fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a <= b { (a.to_string(), b.to_string()) } else { (b.to_string(), a.to_string()) }
}

/// Returns the set of all currently-overlapping ID pairs.
/// `collidables` must be sorted by `left` ascending (callers sort before passing).
pub fn overlapping_pairs(collidables: &[Collidable]) -> HashSet<(String, String)> {
    let mut result = HashSet::new();
    for i in 0..collidables.len() {
        for j in (i + 1)..collidables.len() {
            let a = &collidables[i];
            let b = &collidables[j];
            if b.left >= a.right { break; }
            if a.bottom <= b.top || b.bottom <= a.top { continue; }
            if a.left == a.right || b.left == b.right { continue; }
            result.insert(canonical_pair(&a.id, &b.id));
        }
    }
    result
}

/// Returns collision events for pairs that are overlapping *now* but were
/// NOT overlapping in the previous frame. Fires at most once per collision.
/// `collidables` must be sorted by `left` ascending.
pub fn detect_new_collisions(
    collidables: &[Collidable],
    previously_overlapping: &HashSet<(String, String)>,
) -> Vec<CollisionPair> {
    let mut result = Vec::new();
    for i in 0..collidables.len() {
        for j in (i + 1)..collidables.len() {
            let a = &collidables[i];
            let b = &collidables[j];
            if b.left >= a.right { break; }
            if a.bottom <= b.top || b.bottom <= a.top { continue; }
            if a.left == a.right || b.left == b.right { continue; }
            let key = canonical_pair(&a.id, &b.id);
            if previously_overlapping.contains(&key) { continue; }
            let (type_a, type_b) = classify(a, b);
            result.push(CollisionPair {
                id_a: a.id.clone(),
                data_a: make_data(a, b, type_a),
                id_b: b.id.clone(),
                data_b: make_data(b, a, type_b),
            });
        }
    }
    result
}

fn classify(a: &Collidable, b: &Collidable) -> (String, String) {
    let rel_vx = a.vx - b.vx;
    let rel_vy = a.vy - b.vy;
    if rel_vx.abs() >= rel_vy.abs() {
        let a_cx = (a.left + a.right) / 2;
        let b_cx = (b.left + b.right) / 2;
        let approaching = (a_cx < b_cx && a.vx > b.vx) || (a_cx > b_cx && a.vx < b.vx);
        let t = if approaching { "head_on" } else { "same_dir" };
        (t.to_string(), t.to_string())
    } else {
        let a_above = a.center_y < b.center_y;
        let a_moving_down = a.vy > b.vy;
        match (a_above, a_moving_down) {
            (true,  true)  => ("fell_on".to_string(),        "landed_on".to_string()),
            (true,  false) => ("landed_on".to_string(),      "fell_on".to_string()),
            (false, true)  => ("hit_from_below".to_string(), "hit_into_above".to_string()),
            (false, false) => ("hit_into_above".to_string(), "hit_from_below".to_string()),
        }
    }
}

fn make_data(me: &Collidable, other: &Collidable, collide_type: String) -> CollideData {
    let vx = me.vx - other.vx;
    let vy = me.vy - other.vy;
    CollideData { collide_type, vx, vy, v: (vx * vx + vy * vy).sqrt() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(id: &str, left: i32, right: i32, top: i32, bottom: i32, vx: f32, vy: f32) -> Collidable {
        Collidable {
            id: id.to_string(),
            left, right, top, bottom,
            center_y: (top + bottom) / 2,
            vx, vy,
        }
    }

    #[test]
    fn head_on_collision_detected() {
        // a moving right, b moving left, overlapping
        let a = col("a", 0,  50, 0, 50, 100.0, 0.0);
        let b = col("b", 25, 75, 0, 50, -100.0, 0.0);
        let pairs = detect_new_collisions(&[a, b], &HashSet::new());
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].data_a.collide_type, "head_on");
        assert_eq!(pairs[0].data_b.collide_type, "head_on");
    }

    #[test]
    fn continued_overlap_not_re_fired() {
        let a = col("a", 0, 50, 0, 50, 0.0, 0.0);
        let b = col("b", 25, 75, 0, 50, 0.0, 0.0);
        let prev: HashSet<_> = [("a".to_string(), "b".to_string())].into_iter().collect();
        let pairs = detect_new_collisions(&[a, b], &prev);
        assert_eq!(pairs.len(), 0);
    }

    #[test]
    fn non_overlapping_produces_no_pair() {
        let a = col("a", 0,  50, 0, 50, 0.0, 0.0);
        let b = col("b", 60, 110, 0, 50, 0.0, 0.0);
        let pairs = detect_new_collisions(&[a, b], &HashSet::new());
        assert_eq!(pairs.len(), 0);
    }

    #[test]
    fn fell_on_vertical_collision() {
        // a above b, a moving down faster
        let a = col("a", 0, 50, 0,  50, 0.0, 200.0);
        let b = col("b", 0, 50, 40, 90, 0.0,   0.0);
        let pairs = detect_new_collisions(&[a, b], &HashSet::new());
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].data_a.collide_type, "fell_on");
        assert_eq!(pairs[0].data_b.collide_type, "landed_on");
    }

    #[test]
    fn overlapping_pairs_tracks_all_current_overlaps() {
        let a = col("a", 0,  50, 0, 50, 0.0, 0.0);
        let b = col("b", 25, 75, 0, 50, 0.0, 0.0);
        let c = col("c", 80, 130, 0, 50, 0.0, 0.0);
        let pairs = overlapping_pairs(&[a, b, c]);
        assert!(pairs.contains(&("a".to_string(), "b".to_string())));
        assert!(!pairs.contains(&("a".to_string(), "c".to_string())));
    }
}
```

- [ ] **Step 2: Export from `crates/ferrite-core/src/sprite/mod.rs`**

Add:

```rust
pub mod collision;
```

- [ ] **Step 3: Run the new tests**

```
cargo test -p ferrite-core collision
```

Expected: all 5 tests pass.

- [ ] **Step 4: Update `src/app.rs` — use the core collision API**

Add import:

```rust
use ferrite_core::sprite::collision::{Collidable, canonical_pair, detect_new_collisions, overlapping_pairs};
```

Replace the private `PetBox` struct and all five helper functions (`collect_boxes`, `canonical_key`, `classify_collision`, `make_collide_data`) with one function that builds `Collidable` values:

```rust
fn collect_collidables(pets: &std::collections::HashMap<String, PetInstance>) -> Vec<Collidable> {
    let mut items: Vec<Collidable> = pets.iter().map(|(id, pet)| {
        let frame_idx = pet.anim.absolute_frame(&pet.sheet);
        let flip = pet.compute_flip();
        let scale = pet.cfg.scale.round() as u32;
        let (dx, dy, w, h) = pet.sheet.tight_bbox(frame_idx, scale, flip);
        let left  = pet.x + dx;
        let top   = pet.y + dy;
        let (vx, vy) = pet.runner.speed();
        Collidable {
            id: id.clone(),
            left,
            right:    left + w as i32,
            top,
            bottom:   top  + h as i32,
            center_y: top  + h as i32 / 2,
            vx,
            vy,
        }
    }).collect();
    items.sort_by_key(|c| c.left);
    items
}
```

Replace the collision detection block in `App::update()`:

```rust
        // ── Collision detection ──────────────────────────────────────────────────
        if self.pets.len() >= 2 {
            let collidables = collect_collidables(&self.pets);
            let new_overlapping = overlapping_pairs(&collidables);
            for pair in detect_new_collisions(&collidables, &self.overlapping) {
                if let Some(pet) = self.pets.get_mut(&pair.id_a) {
                    pet.runner.on_collide(pair.data_a);
                }
                if let Some(pet) = self.pets.get_mut(&pair.id_b) {
                    pet.runner.on_collide(pair.data_b);
                }
            }
            self.overlapping = new_overlapping;
        }
```

Also update `self.overlapping` field type. It was `HashSet<(String, String)>` using inline string manipulation. Verify the type is unchanged — `overlapping_pairs` returns `HashSet<(String, String)>` with the same canonicalisation.

- [ ] **Step 5: Build and test**

```
cargo build
cargo test
```

- [ ] **Step 6: Commit**

```
git add crates/ferrite-core/src/sprite/collision.rs \
        crates/ferrite-core/src/sprite/mod.rs \
        src/app.rs
git commit -m "refactor(core): move collision classification API to ferrite-core"
```

---

## Task 5: `Airborne` — unify `Fall` and `Thrown` physics states

**What this fixes:** `ActiveState::Fall { vy }` and `ActiveState::Thrown { vx, vy }` duplicate gravity application. Screen-boundary bounce is only in `Thrown`; `Fall` has no horizontal movement at all (not just zero bounce — zero movement). A new physics state (e.g., wind-pushed, bouncing ball) would need to manually add gravity and bounce. `Airborne { vx, vy }` unifies them: `vx=0` is a fall, `vx≠0` is a throw, and both go through the same gravity + bounce code.

**Behaviour change:** When a thrown pet lands, it now transitions to the idle state in one tick instead of two. (Previously: Thrown → Fall{vy:0} on landing tick → idle on the next tick. Now: Airborne{vx, vy} → idle on landing tick.)

**Files:**
- Modify: `crates/ferrite-core/src/sprite/sm_runner.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Write failing tests for the new `Airborne` semantics**

Add to the `#[cfg(test)]` block in `sm_runner.rs` (from Task 3):

```rust
    #[test]
    fn current_state_name_airborne_zero_velocity_is_fall() {
        let mut runner = SMRunner::new(load_default_sm(), 100.0);
        runner.active = ActiveState::Airborne { vx: 0.0, vy: 0.0 };
        assert_eq!(runner.current_state_name(), "fall");
    }

    #[test]
    fn current_state_name_airborne_large_vx_is_thrown() {
        let mut runner = SMRunner::new(load_default_sm(), 100.0);
        runner.active = ActiveState::Airborne { vx: 500.0, vy: 100.0 };
        assert_eq!(runner.current_state_name(), "thrown");
    }

    #[test]
    fn release_large_velocity_produces_thrown() {
        let mut runner = SMRunner::new(load_default_sm(), 100.0);
        runner.release((500.0, -200.0));
        assert_eq!(runner.current_state_name(), "thrown");
    }

    #[test]
    fn release_small_velocity_produces_fall() {
        let mut runner = SMRunner::new(load_default_sm(), 100.0);
        runner.release((1.0, 1.0));
        assert_eq!(runner.current_state_name(), "fall");
    }

    #[test]
    fn start_fall_produces_fall() {
        let mut runner = SMRunner::new(load_default_sm(), 100.0);
        runner.start_fall();
        assert_eq!(runner.current_state_name(), "fall");
    }
```

Run — expect failures because `Airborne` variant doesn't exist yet:

```
cargo test -p ferrite-core airborne current_state_name release_large release_small start_fall
```

- [ ] **Step 2: Replace `Fall` and `Thrown` variants with `Airborne` in `sm_runner.rs`**

Change the `ActiveState` enum:

```rust
#[derive(Debug, Clone)]
pub enum ActiveState {
    Named(String),
    /// Airborne physics (falling or thrown). `vx == 0` is a pure fall;
    /// `vx.abs() > 10` is a throw with horizontal bounce.
    Airborne { vx: f32, vy: f32 },
    Grabbed { cursor_offset: (i32, i32) },
}
```

Update `current_state_name`:

```rust
    pub fn current_state_name(&self) -> &str {
        match &self.active {
            ActiveState::Named(name) => name.as_str(),
            ActiveState::Airborne { vx, vy } => {
                if vx.abs() > 10.0 || vy.abs() > 10.0 { "thrown" } else { "fall" }
            }
            ActiveState::Grabbed { .. } => "grabbed",
        }
    }
```

Update `speed`:

```rust
    pub fn speed(&self) -> (f32, f32) {
        match &self.active {
            ActiveState::Airborne { vx, vy } => (*vx, *vy),
            _ => (0.0, 0.0),
        }
    }
```

Update `release`:

```rust
    pub fn release(&mut self, velocity: (f32, f32)) {
        let (vx, vy) = velocity;
        let (vx, vy) = if vx.abs() > 10.0 || vy.abs() > 10.0 { (vx, vy) } else { (0.0, 0.0) };
        self.active = ActiveState::Airborne { vx, vy };
        self.state_time_ms = 0;
    }
```

Update `start_fall`:

```rust
    pub fn start_fall(&mut self) {
        self.set_previous_from_current();
        self.active = ActiveState::Airborne { vx: 0.0, vy: 0.0 };
        self.state_time_ms = 0;
    }
```

Update `execute_action` — replace both `ActiveState::Fall { vy }` and `ActiveState::Thrown { vx, vy }` arms with one unified `Airborne` arm:

```rust
            ActiveState::Airborne { vx, vy } => {
                let mut vx = vx;
                let mut vy = vy;
                vy += GRAVITY * dt;
                let new_x = *x + (vx * dt) as i32;
                let new_y = *y + (vy * dt) as i32;

                // Horizontal boundary bounce (no-op when vx == 0).
                let (clamped_x, new_vx) = if new_x <= 0 {
                    (0, vx.abs())
                } else if new_x + pet_w >= bounds.screen_w {
                    (bounds.screen_w - pet_w, -vx.abs())
                } else {
                    (new_x, vx)
                };
                vx = new_vx;

                if new_y >= floor_y {
                    *x = clamped_x;
                    *y = floor_y;
                    self.last_vars.on_surface = true;
                    let fallback = self.sm.default_fallback.clone();
                    self.transition_to(&fallback, "landed");
                } else {
                    *x = clamped_x;
                    *y = new_y;
                    self.active = ActiveState::Airborne { vx, vy };
                    self.last_vars.on_surface = false;
                }
            }
```

Also update `try_transitions` — it currently returns early for `Fall` and `Thrown` variants:

```rust
        let state_name = match &self.active {
            ActiveState::Named(n) => n.clone(),
            _ => return, // Airborne and Grabbed transition via execute_action
        };
```

- [ ] **Step 3: Update `src/app.rs`**

Replace the `is_airborne` match expression:

```rust
        let is_airborne = matches!(
            self.runner.active,
            crate::sprite::sm_runner::ActiveState::Airborne { .. }
            | crate::sprite::sm_runner::ActiveState::Grabbed { .. }
        );
```

Replace both places where `ActiveState::Fall { vy: 0.0 }` is assigned directly (edge-fall and elevated-drop):

```rust
                self.runner.active = crate::sprite::sm_runner::ActiveState::Airborne { vx: 0.0, vy: 0.0 };
```

- [ ] **Step 4: Run the tests**

```
cargo test -p ferrite-core airborne current_state_name_airborne release_large release_small start_fall
cargo test
```

Expected: all pass. Verify the integration tests still pass — the `"fall"` and `"thrown"` tag names are returned by `current_state_name()` identically to before.

- [ ] **Step 5: Commit**

```
git add crates/ferrite-core/src/sprite/sm_runner.rs src/app.rs
git commit -m "refactor(core): unify Fall/Thrown into Airborne; gravity and bounce in one place"
```

---

## Task 6: `AnimationState::current_tag` encapsulation

**What this fixes:** `AnimationState::current_tag` is a `pub` field, so external code can write `anim.current_tag = "..."` without resetting `frame_index`, `elapsed_ms`, and `ping_pong_dir`. This would produce jittery playback. `set_tag()` already handles the atomic reset correctly; making `current_tag` private enforces that all tag changes go through `set_tag`.

**Files:**
- Modify: `crates/ferrite-core/src/sprite/animation.rs`
- Modify: `crates/ferrite-webapp/src/simulation.rs`
- Check and update any other external access sites.

- [ ] **Step 1: Make `current_tag` private and add getter in `animation.rs`**

Change:

```rust
pub struct AnimationState {
    pub current_tag: String,
    pub frame_index: usize,
    pub elapsed_ms: u32,
    pub ping_pong_dir: PlayDirection,
}
```

to:

```rust
pub struct AnimationState {
    current_tag: String,        // private — use set_tag() to change, current_tag() to read
    pub frame_index: usize,
    pub elapsed_ms: u32,
    pub ping_pong_dir: PlayDirection,
}
```

Add a getter after `new`:

```rust
    /// Returns the name of the currently-active animation tag.
    pub fn current_tag(&self) -> &str {
        &self.current_tag
    }
```

- [ ] **Step 2: Attempt to build — find all external access sites**

```
cargo build 2>&1 | grep "current_tag"
```

This will list every file that directly accesses the now-private field.

- [ ] **Step 3: Fix all external access sites**

The known site is `crates/ferrite-webapp/src/simulation.rs` line `animation_tag: pet.anim.current_tag.clone()`. Change to:

```rust
            animation_tag: pet.anim.current_tag().to_owned(),
```

For any other sites found in Step 2, apply the same pattern: `anim.current_tag` → `anim.current_tag()` for reads.

- [ ] **Step 4: Build and test**

```
cargo build
cargo test
```

- [ ] **Step 5: Commit**

```
git add crates/ferrite-core/src/sprite/animation.rs crates/ferrite-webapp/src/simulation.rs
git commit -m "refactor(core): make AnimationState::current_tag private; enforce set_tag() as sole mutation path"
```

---

## Backlog (non-computation cleanup — separate PRs)

Create `docs/superpowers/plans/backlog-engine-cleanup.md` with the following content and commit it:

```markdown
# Engine Cleanup Backlog

Items identified during the core-computation-centralization refactor that are
out-of-place but are NOT computation (so deferred from that PR per policy).

## B1: DragState encapsulation in wndproc.rs

`HwndData` has 8 loose drag-related fields. Organise them into an inner
`DragState` struct for readability. Pure organisation within Win32 platform code.

File: `src/window/wndproc.rs`

## B2: SpatialContext — deduplicate distance formulas in app.rs

The cursor-to-pet and pet-to-pet distance formulas are written out twice inline
in `App::update()`. A small helper or closure would deduplicate them. Stays in
desktop platform code.

File: `src/app.rs` (~lines 738–751)

## B3: ScaledDimensions — centralise scale rounding

`(value as f32 * cfg.scale).round() as i32` appears in multiple places in
`PetInstance::tick` and `collect_collidables`. A helper type or method would
ensure consistent rounding.

File: `src/app.rs`

## B4: InteractionEvent coordinate frame documentation

`PetDragStart` carries screen-space cursor coordinates; the pet-relative offset
is computed later in `App::handle_event`. A doc comment clarifying the coordinate
frame would prevent confusion when extending drag handling.

File: `src/event.rs`
```

Commit:
```
git add docs/superpowers/plans/backlog-engine-cleanup.md
git commit -m "docs: add engine cleanup backlog (non-computation items)"
```

---

## Verification

```
cargo build                       # clean compile
cargo test                        # all unit + integration tests
cargo clippy -- -D warnings -A dead-code   # no lint errors
cargo test --test e2e             # all E2E tests
```
