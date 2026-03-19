# Performance Optimization Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate debug build lag and reduce per-tick Win32 overhead by optimizing the Cargo profile, caching surface detection results, and reusing GDI objects across frames.

**Architecture:** Three independent improvements applied in sequence: (1) compile deps at opt-level 2 in debug builds, (2) cache `EnumWindows` results in `SurfaceCache` with 250 ms TTL, (3) keep GDI DC + DIB alive on `PetWindow` instead of recreating each frame. Criterion benchmarks and a stress integration test complete the picture.

**Tech Stack:** Rust, windows-sys, criterion 0.5, existing eframe/wgpu stack.

---

## Chunk 1: Cargo Profile + Surface Cache

### Task 1: Cargo Profile Changes

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add dev profile override**

In `Cargo.toml`, add below the existing `[profile.release]` block:

```toml
[profile.dev.package."*"]
opt-level = 2
```

Also change `[profile.release]`:

```toml
[profile.release]
opt-level = 3   # was "s" — speed over size
lto = true
strip = true
```

- [ ] **Step 2: Verify debug build still compiles**

```bash
cargo build 2>&1 | grep -E "^error"
```

Expected: no output (clean build).

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "perf: compile deps at opt-level 2 in debug, switch release to opt-level 3"
```

---

### Task 2: Surface Cache (`src/window/surfaces.rs`)

**Files:**
- Modify: `src/window/surfaces.rs`
- Modify: `src/app.rs` (App struct + PetInstance::tick)

#### Step 2a: Add SurfaceCache type and update find_floor

- [ ] **Step 1: Write a failing test for cache expiry**

Add to `src/window/surfaces.rs` inside `#[cfg(test)] mod tests`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_cache_default_is_expired() {
        let cache = SurfaceCache::default();
        assert!(cache.is_expired(), "default cache must be expired so first call always re-fetches");
    }

    #[test]
    fn surface_cache_find_floor_returns_plausible_value() {
        let mut cache = SurfaceCache::default();
        let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) }; // SM_CXSCREEN
        let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) }; // SM_CYSCREEN
        // Pet at top of screen, 32x32
        let floor = find_floor(0, 0, 32, 32, screen_w, screen_h, &mut cache);
        // Floor must be above the screen bottom and >= 0
        assert!(floor >= 0, "floor y must be non-negative, got {floor}");
        assert!(floor < screen_h, "floor y must be above screen bottom, got {floor}");
    }

    #[test]
    fn surface_cache_warm_returns_same_result() {
        let mut cache = SurfaceCache::default();
        let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) };
        let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) };
        // First call: cold (fills cache)
        let floor1 = find_floor(100, 0, 32, 32, screen_w, screen_h, &mut cache);
        assert!(!cache.is_expired(), "cache must be warm after first call");
        // Second call: warm (must return same value as long as pet position unchanged)
        let floor2 = find_floor(100, 0, 32, 32, screen_w, screen_h, &mut cache);
        assert_eq!(floor1, floor2, "warm cache must return same floor as cold call");
    }
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test --lib window::surfaces 2>&1 | tail -20
```

Expected: compile error — `SurfaceCache`, `is_expired`, updated `find_floor` signature don't exist yet.

- [ ] **Step 3: Add SurfaceCache and SurfaceRect types**

At the top of `src/window/surfaces.rs`, after the imports, add:

```rust
use std::time::{Duration, Instant};

/// One entry in the surface cache. Stores the raw rect of a qualifying window
/// plus the HWND so the occlusion check can be performed at fill time.
/// `hwnd` is not public — it's an implementation detail of the fill pass.
#[derive(Clone)]
pub struct SurfaceRect {
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
    hwnd: HWND,
}

/// 250 ms TTL cache for walkable surface rects.
/// Rects are filtered for visibility and occlusion at fill time (`EnumWindows`).
/// Cache-hit path re-applies per-call overlap and `min_surface` checks only —
/// occlusion is intentionally skipped on hits (acceptable TTL trade-off).
///
/// `Default` produces an already-expired cache so the first `find_floor` call
/// always triggers a fresh `EnumWindows`.
pub struct SurfaceCache {
    entries: Vec<SurfaceRect>,
    expires_at: Instant,
}

impl Default for SurfaceCache {
    fn default() -> Self {
        SurfaceCache {
            entries: Vec::new(),
            expires_at: Instant::now() - Duration::from_secs(1), // already expired
        }
    }
}

impl SurfaceCache {
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}
```

- [ ] **Step 4: Add a fill-state struct for the EnumWindows callback**

Replace the existing `FindState` struct and `enum_cb` function with `FillState` and `fill_cb`.
The key difference: occlusion (`WindowFromPoint`) is checked inside `fill_cb` at fill time using
the horizontal midpoint of the window rect. Only rects that pass are stored.

```rust
struct FillState {
    screen_w: i32,
    entries: Vec<SurfaceRect>,
}

unsafe extern "system" fn fill_cb(hwnd: HWND, lparam: LPARAM) -> i32 {
    if wndproc::is_pet_hwnd(hwnd) { return 1; }
    if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 { return 1; }
    let s = &mut *(lparam as *mut FillState);
    let mut rc: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut rc);
    // Skip full-screen / maximised windows.
    if rc.right - rc.left >= s.screen_w - 10 && rc.top <= 0 { return 1; }
    // Occlusion check at fill time: sample the window at the horizontal midpoint
    // of this rect's top edge. If something else is in front, skip this rect.
    let check_x = (rc.left + rc.right) / 2;
    let top_at_pt = WindowFromPoint(POINT { x: check_x, y: rc.top });
    let visible = top_at_pt.is_null()
        || top_at_pt == hwnd
        || IsChild(hwnd, top_at_pt) != 0
        || wndproc::is_pet_hwnd(top_at_pt);
    if !visible { return 1; }
    s.entries.push(SurfaceRect {
        left: rc.left,
        right: rc.right,
        top: rc.top,
        bottom: rc.bottom,
        hwnd,
    });
    1
}
```

- [ ] **Step 5: Rewrite find_floor with cache support**

Replace the existing `find_floor` function. The scan loop re-applies only per-call
overlap and `min_surface` filters — no `WindowFromPoint` on cache hits:

```rust
/// Returns the y-coordinate the pet top should be at when it lands on the
/// nearest surface below it. Falls back to the virtual screen ground.
///
/// `cache` is filled via `EnumWindows` on the first call (or after TTL expiry)
/// including full occlusion checks. Cache hits re-apply per-call overlap and
/// min_surface filters only; occlusion is skipped (acceptable 250 ms TTL trade-off).
pub fn find_floor(
    pet_x: i32,
    pet_y: i32,
    pet_w: i32,
    pet_h: i32,
    screen_w: i32,
    screen_h: i32,
    cache: &mut SurfaceCache,
) -> i32 {
    // Refresh cache if expired.
    if cache.is_expired() {
        let mut fill = FillState { screen_w, entries: Vec::new() };
        unsafe {
            EnumWindows(Some(fill_cb), &mut fill as *mut _ as LPARAM);
        }
        cache.entries = fill.entries;
        cache.expires_at = Instant::now() + Duration::from_millis(250);
    }

    let pet_left = pet_x;
    let pet_right = pet_x + pet_w;
    let pet_bottom = pet_y + pet_h;
    let min_surface = pet_bottom.max(pet_h);
    let virtual_ground_top = screen_h - 4;
    let mut best = virtual_ground_top;

    for rect in &cache.entries {
        // Re-apply per-call horizontal overlap filter.
        if pet_right <= rect.left || pet_left >= rect.right { continue; }
        // Re-apply min_surface filter.
        if rect.top < min_surface || rect.top >= best { continue; }
        // Occlusion already verified at fill time — skip WindowFromPoint here.
        best = rect.top;
    }

    best - pet_h
}
```

- [ ] **Step 6: Run the tests**

```bash
cargo test --lib window::surfaces 2>&1 | tail -20
```

Expected: all 3 new tests pass.

- [ ] **Step 7: Update all call sites in app.rs**

In `src/app.rs`:

1. Add `surface_cache` field to `App`:

```rust
pub struct App {
    tx: Sender<AppEvent>,
    rx: Receiver<AppEvent>,
    pets: HashMap<String, PetInstance>,
    _tray: SystemTray,
    _watcher: notify::RecommendedWatcher,
    last_tick_ms: std::time::Instant,
    config_window_state: Option<Arc<Mutex<ConfigWindowState>>>,
    sprite_editor_state: Option<Arc<Mutex<SpriteEditorViewport>>>,
    should_quit: bool,
    surface_cache: crate::window::surfaces::SurfaceCache,
}
```

2. Initialize it in `App::new()`:

```rust
Ok(App {
    // ... existing fields ...
    surface_cache: crate::window::surfaces::SurfaceCache::default(),
})
```

3. Update `PetInstance::tick` signature:

```rust
pub fn tick(&mut self, delta_ms: u32, cache: &mut crate::window::surfaces::SurfaceCache) -> Result<()> {
```

4. Update both `find_floor` calls inside `tick`:

```rust
let floor_y = crate::window::surfaces::find_floor(
    self.x, self.y, pet_w, pet_h, screen_w, screen_h, cache,
);
// ...
let new_floor = crate::window::surfaces::find_floor(
    self.x, self.y, pet_w, pet_h, screen_w, screen_h, cache,
);
```

5. Update the tick call site in `App::update()`:

```rust
for pet in self.pets.values_mut() {
    if let Err(e) = pet.tick(delta_ms, &mut self.surface_cache) {
        log::warn!("pet tick error: {e}");
    }
}
```

- [ ] **Step 8: Run full test suite**

```bash
cargo test 2>&1 | grep -E "test result|^error"
```

Expected: all tests pass, 0 failures.

- [ ] **Step 9: Commit**

```bash
git add src/window/surfaces.rs src/app.rs
git commit -m "perf: cache EnumWindows results in SurfaceCache with 250ms TTL"
```

---

## Chunk 2: GDI Cache + Benchmarks + Stress Test

> **Prerequisite:** Chunk 1 (Tasks 1 and 2) must be complete before starting this chunk.
> Verify with `cargo test 2>&1 | grep "test result"` — all tests must pass.

### Task 3: GDI Object Cache (`src/window/pet_window.rs`)

**Files:**
- Modify: `src/window/pet_window.rs`

- [ ] **Step 1: Write a failing test**

Add to the `#[cfg(test)] mod tests` block in `src/window/pet_window.rs`:

```rust
#[test]
fn render_frame_twice_same_result() {
    let mut win = PetWindow::create(0, 0, 64, 64).expect("create");
    let sheet = crate::sprite::sheet::load_embedded(
        include_bytes!("../../assets/test_pet.json"),
        include_bytes!("../../assets/test_pet.png"),
    )
    .unwrap();
    let f = &sheet.frames[0];
    win.render_frame(&sheet.image, f.x, f.y, f.w, f.h, 2, false)
        .expect("first render");
    let buf1 = win.frame_buf.clone();
    win.render_frame(&sheet.image, f.x, f.y, f.w, f.h, 2, false)
        .expect("second render");
    assert_eq!(buf1, win.frame_buf, "frame buffer must be identical on repeated render");
}
```

- [ ] **Step 2: Run test to verify it passes already (baseline)**

```bash
cargo test --lib window::pet_window::tests::render_frame_twice_same_result 2>&1 | tail -10
```

Expected: PASS — the test establishes a behavioral baseline. It must continue to pass after the GDI cache is added.

- [ ] **Step 3: Add GDI cache fields to PetWindow**

Replace the `PetWindow` struct definition:

```rust
pub struct PetWindow {
    pub hwnd: HWND,
    pub width: u32,
    pub height: u32,
    /// Last rendered frame buffer (premultiplied BGRA). Exposed for tests.
    pub frame_buf: Vec<u8>,
    // ── GDI cache ──────────────────────────────────────────────────────────
    // These are created once in `create()` and reused every `render_frame()`.
    // Adding *mut u8 makes PetWindow automatically !Send + !Sync — correct for
    // Win32 GDI objects which must stay on their creation thread.
    mem_dc: HDC,
    dib: HBITMAP,
    /// Direct pointer into the DIB's pixel memory. Valid while `dib` is alive.
    dib_bits: *mut u8,
    /// Dimensions the GDI cache was allocated for. Used to detect size changes.
    cached_w: u32,
    cached_h: u32,
}
```

- [ ] **Step 4: Add a helper to create the GDI cache**

Add a private method to `PetWindow`:

```rust
/// Allocate (or reallocate) the mem_dc + DIB for the given dimensions.
/// Destroys the previous objects if they exist (non-null).
/// Destruction order: deselect bitmap → delete DC → delete bitmap.
/// (A bitmap selected into a DC must be deselected before the DC is deleted,
/// and the DC must be deleted before the bitmap to avoid GDI handle leaks.)
unsafe fn alloc_gdi_cache(&mut self, w: u32, h: u32) {
    // Destroy previous objects in the correct order.
    if !self.mem_dc.is_null() {
        // Deselect the bitmap by selecting a stock object, then delete the DC.
        SelectObject(self.mem_dc, GetStockObject(BLACK_BRUSH as i32));
        DeleteDC(self.mem_dc);
        self.mem_dc = std::ptr::null_mut();
    }
    if !self.dib.is_null() {
        DeleteObject(self.dib);
        self.dib = std::ptr::null_mut();
        self.dib_bits = std::ptr::null_mut();
    }

    let hdc_screen = GetDC(std::ptr::null_mut());
    self.mem_dc = CreateCompatibleDC(hdc_screen);
    ReleaseDC(std::ptr::null_mut(), hdc_screen);

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32), // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbReserved: 0 }],
    };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    self.dib = CreateDIBSection(
        self.mem_dc,
        &bmi,
        DIB_RGB_COLORS,
        &mut bits,
        std::ptr::null_mut(),
        0,
    );
    self.dib_bits = bits as *mut u8;
    SelectObject(self.mem_dc, self.dib);
    self.cached_w = w;
    self.cached_h = h;
}
```

- [ ] **Step 5: Update PetWindow::create to initialize GDI cache**

In `PetWindow::create`, after `ShowWindow`, replace:

```rust
Ok(PetWindow { hwnd, width, height, frame_buf: Vec::new() })
```

with:

```rust
let mut win = PetWindow {
    hwnd,
    width,
    height,
    frame_buf: Vec::new(),
    mem_dc: std::ptr::null_mut(),
    dib: std::ptr::null_mut(),
    dib_bits: std::ptr::null_mut(),
    cached_w: 0,
    cached_h: 0,
};
win.alloc_gdi_cache(width, height);
Ok(win)
```

- [ ] **Step 6: Update render_frame to use cached GDI objects**

Replace the body of `render_frame` after the `blit_frame` call:

```rust
pub fn render_frame(
    &mut self,
    src: &image::RgbaImage,
    src_x: u32,
    src_y: u32,
    src_w: u32,
    src_h: u32,
    scale: u32,
    flip_h: bool,
) -> Result<()> {
    blit_frame(src, src_x, src_y, src_w, src_h, &mut self.frame_buf, scale, flip_h);

    let dw = src_w * scale;
    let dh = src_h * scale;

    // Reallocate GDI cache if dimensions changed (e.g. scale change).
    if dw != self.cached_w || dh != self.cached_h {
        unsafe { self.alloc_gdi_cache(dw, dh); }
        self.width = dw;
        self.height = dh;
    }

    anyhow::ensure!(!self.dib_bits.is_null(), "GDI cache not initialized");

    unsafe {
        // Copy premultiplied BGRA pixels directly into the DIB's memory.
        std::ptr::copy_nonoverlapping(
            self.frame_buf.as_ptr(),
            self.dib_bits,
            self.frame_buf.len(),
        );

        let mut rc: RECT = std::mem::zeroed();
        GetWindowRect(self.hwnd, &mut rc);
        let pt_dst = POINT { x: rc.left, y: rc.top };
        let pt_src = POINT { x: 0, y: 0 };
        let sz = SIZE { cx: dw as i32, cy: dh as i32 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        let hdc_screen = GetDC(std::ptr::null_mut());
        let ok = UpdateLayeredWindow(
            self.hwnd, hdc_screen,
            &pt_dst, &sz,
            self.mem_dc, &pt_src,
            0, &blend, ULW_ALPHA,
        );
        ReleaseDC(std::ptr::null_mut(), hdc_screen);
        if ok == 0 {
            // Can fail on headless/RDP sessions without desktop composition — log and continue.
            log::warn!("UpdateLayeredWindow failed (err={})", windows_sys::Win32::Foundation::GetLastError());
        }
    }

    crate::window::wndproc::update_alpha_buf(self.hwnd, &self.frame_buf, dw);
    Ok(())
}
```

- [ ] **Step 7: Update Drop to free GDI cache**

Replace the `Drop` impl:

```rust
impl Drop for PetWindow {
    fn drop(&mut self) {
        unsafe {
            // Deselect bitmap → delete DC → delete bitmap (GDI required order).
            if !self.mem_dc.is_null() {
                SelectObject(self.mem_dc, GetStockObject(BLACK_BRUSH as i32));
                DeleteDC(self.mem_dc);
            }
            if !self.dib.is_null() { DeleteObject(self.dib); }
            if !self.hwnd.is_null() { DestroyWindow(self.hwnd); }
        }
    }
}
```

- [ ] **Step 8: Run all tests**

```bash
cargo test 2>&1 | grep -E "test result|^error"
```

Expected: all tests pass. `render_frame_twice_same_result` must still pass.

- [ ] **Step 9: Commit**

```bash
git add src/window/pet_window.rs
git commit -m "perf: cache GDI DC+DIB in PetWindow, reuse across render_frame calls"
```

---

### Task 4: Criterion Benchmarks

**Files:**
- Modify: `Cargo.toml` (add criterion dev-dep and bench targets)
- Create: `benches/surfaces.rs`
- Create: `benches/render.rs`
- Create: `benches/animation.rs`

- [ ] **Step 1: Add criterion to Cargo.toml**

```toml
[dev-dependencies]
tempfile = "3"
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "surfaces"
harness = false

[[bench]]
name = "render"
harness = false

[[bench]]
name = "animation"
harness = false
```

- [ ] **Step 2: Create benches/surfaces.rs**

```rust
use criterion::{criterion_group, criterion_main, Criterion};
use my_pet::window::surfaces::{find_floor, SurfaceCache};

fn bench_find_floor_cold(c: &mut Criterion) {
    let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) };
    let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) };
    // Reduced sample size: EnumWindows is a blocking syscall with OS scheduling jitter.
    c.bench_function("find_floor_cold", |b| {
        b.iter(|| {
            // Re-expire cache on every iteration to force EnumWindows each time.
            let mut cache = SurfaceCache::default();
            find_floor(100, 0, 32, 32, screen_w, screen_h, &mut cache)
        })
    });
}

fn bench_find_floor_cached(c: &mut Criterion) {
    let screen_w = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0) };
    let screen_h = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1) };
    let mut cache = SurfaceCache::default();
    // Warm the cache once before benchmarking.
    find_floor(100, 0, 32, 32, screen_w, screen_h, &mut cache);
    c.bench_function("find_floor_cached", |b| {
        b.iter(|| find_floor(100, 0, 32, 32, screen_w, screen_h, &mut cache))
    });
}

criterion_group! {
    name = surfaces_benches;
    config = Criterion::default().sample_size(20);
    targets = bench_find_floor_cold, bench_find_floor_cached
}
criterion_main!(surfaces_benches);
```

- [ ] **Step 3: Create benches/render.rs**

```rust
use criterion::{criterion_group, criterion_main, Criterion};
use my_pet::sprite::sheet::load_embedded;

fn bench_blit_frame(c: &mut Criterion, scale: u32, label: &str) {
    let sheet = load_embedded(
        include_bytes!("../assets/test_pet.json"),
        include_bytes!("../assets/test_pet.png"),
    )
    .unwrap();
    let f = &sheet.frames[0];
    let mut buf = Vec::new();
    c.bench_function(label, |b| {
        b.iter(|| {
            my_pet::window::blender::blit_frame(
                &sheet.image, f.x, f.y, f.w, f.h, &mut buf, scale, false,
            )
        })
    });
}

fn bench_blit_1x(c: &mut Criterion) { bench_blit_frame(c, 1, "blit_frame_1x"); }
fn bench_blit_2x(c: &mut Criterion) { bench_blit_frame(c, 2, "blit_frame_2x"); }
fn bench_blit_4x(c: &mut Criterion) { bench_blit_frame(c, 4, "blit_frame_4x"); }

criterion_group!(render_benches, bench_blit_1x, bench_blit_2x, bench_blit_4x);
criterion_main!(render_benches);
```

**Note:** `blit_frame` and `blender` must be `pub` in `src/window/blender.rs` and re-exported from `src/window/mod.rs` for benchmarks to access them. Check current visibility — if private, add `pub` to `blit_frame` and `pub mod blender` in `src/window/mod.rs`.

- [ ] **Step 4: Create benches/animation.rs**

```rust
use criterion::{criterion_group, criterion_main, Criterion};
use my_pet::sprite::{
    animation::AnimationState,
    behavior::{AnimTagMap, BehaviorAi, BehaviorState},
    sheet::load_embedded,
};

fn bench_animation_tick(c: &mut Criterion) {
    let sheet = load_embedded(
        include_bytes!("../assets/test_pet.json"),
        include_bytes!("../assets/test_pet.png"),
    )
    .unwrap();
    let tag = sheet.tags.first().map(|t| t.name.clone()).unwrap_or_default();
    let mut anim = AnimationState::new(tag);
    c.bench_function("animation_tick", |b| {
        b.iter(|| anim.tick(&sheet, 16))
    });
}

fn bench_behavior_tick(c: &mut Criterion) {
    let tag_map = AnimTagMap {
        idle: "idle".into(),
        walk: "walk".into(),
        run: None, sit: None, sleep: None, wake: None,
        grabbed: None, petted: None, react: None, fall: None, thrown: None,
    };
    let mut ai = BehaviorAi::new();
    ai.state = BehaviorState::Walk { facing: my_pet::sprite::behavior::Facing::Right };
    c.bench_function("behavior_tick", |b| {
        b.iter(|| {
            let mut x = 0i32;
            let mut y = 0i32;
            ai.tick(16, &mut x, &mut y, 1920, 32, 32, 100.0, 1000, &tag_map)
        })
    });
}

criterion_group!(animation_benches, bench_animation_tick, bench_behavior_tick);
criterion_main!(animation_benches);
```

**Note:** `AnimationState`, `BehaviorAi`, `BehaviorState`, `Facing`, `AnimTagMap` must all be `pub`. Check `src/sprite/behavior.rs` — `Facing` may need `pub`. Adjust imports to match actual module paths.

- [ ] **Step 5: Verify benchmarks compile**

```bash
cargo bench --no-run 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 6: Run benchmarks once to confirm they execute**

```bash
cargo bench -- --test 2>&1 | grep -E "bench|FAILED|error"
```

Expected: benchmark functions execute without panicking.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml benches/
git commit -m "perf: add criterion benchmarks for surfaces, render, and animation"
```

---

### Task 5: Stress Integration Test

**Files:**
- Create: `tests/integration/test_stress.rs`
- Modify: `tests/integration.rs`

- [ ] **Step 1: Create tests/integration/test_stress.rs**

```rust
//! Stress and timing tests for pet tick and render performance.
//! Uses real Win32 windows — no mocking needed for UpdateLayeredWindow/SetWindowPos/GetWindowRect.

use my_pet::{
    app::PetInstance,
    config::schema::PetConfig,
    sprite::{behavior::AnimTagMap, sheet::load_embedded},
    window::{pet_window::PetWindow, surfaces::SurfaceCache},
};
use std::time::Instant;

fn make_pet() -> PetInstance {
    let sheet = load_embedded(
        include_bytes!("../assets/test_pet.json"),
        include_bytes!("../assets/test_pet.png"),
    )
    .unwrap();
    let cfg = PetConfig {
        id: "stress_pet".into(),
        sheet_path: "embedded://test_pet".into(),
        scale: 2,
        x: 100,
        y: 100,
        walk_speed: 100.0,
        flip_walk_left: false,
        tag_map: AnimTagMap {
            idle: "idle".into(),
            walk: "walk".into(),
            run: None, sit: None, sleep: None, wake: None,
            grabbed: None, petted: None, react: None, fall: None, thrown: None,
        },
    };
    PetInstance::new(cfg, sheet).expect("create PetInstance")
}

#[test]
fn tick_1000_frames_10_pets() {
    let mut pets: Vec<PetInstance> = (0..10).map(|_| make_pet()).collect();
    let mut cache = SurfaceCache::default();

    let start = Instant::now();
    for _ in 0..1000 {
        for pet in &mut pets {
            pet.tick(16, &mut cache).expect("tick must not error");
        }
    }
    let elapsed = start.elapsed();

    let budget_ms = 500;
    assert!(
        elapsed.as_millis() < budget_ms,
        "10 pets × 1000 ticks took {}ms — must be under {}ms",
        elapsed.as_millis(),
        budget_ms,
    );
    println!("tick_1000_frames_10_pets: {}ms", elapsed.as_millis());
}

#[test]
fn render_frame_100_times() {
    let mut win = PetWindow::create(0, 0, 64, 64).expect("create window");
    let sheet = load_embedded(
        include_bytes!("../assets/test_pet.json"),
        include_bytes!("../assets/test_pet.png"),
    )
    .unwrap();
    let f = &sheet.frames[0];

    let start = Instant::now();
    for _ in 0..100 {
        win.render_frame(&sheet.image, f.x, f.y, f.w, f.h, 2, false)
            .expect("render must not error");
    }
    let elapsed = start.elapsed();

    let budget_ms = 50;
    assert!(
        elapsed.as_millis() < budget_ms,
        "100 render_frame calls took {}ms — must be under {}ms",
        elapsed.as_millis(),
        budget_ms,
    );
    println!("render_frame_100_times: {}ms", elapsed.as_millis());
}
```

- [ ] **Step 2: Register stress module in tests/integration.rs**

Add at the end of `tests/integration.rs`:

```rust
mod stress {
    include!("integration/test_stress.rs");
}
```

- [ ] **Step 3: Run the stress tests**

```bash
cargo test --test integration stress 2>&1 | tail -20
```

Expected: both tests pass. Timing lines printed to stdout.

- [ ] **Step 4: Run full test suite**

```bash
cargo test 2>&1 | grep -E "test result|^error"
```

Expected: all tests pass, 0 failures.

- [ ] **Step 5: Commit**

```bash
git add tests/integration/test_stress.rs tests/integration.rs
git commit -m "test: add stress integration tests for pet tick and render_frame timing"
```
