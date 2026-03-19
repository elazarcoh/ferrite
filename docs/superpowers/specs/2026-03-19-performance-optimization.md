# Performance Optimization Spec
**Date:** 2026-03-19

## Problem

Debug builds lag noticeably with a single pet. Root causes identified:

1. **Debug build overhead** — wgpu, egui, and image compile at opt-level 0, making the render loop 5–10× slower than release without providing any extra debuggability for those crates.
2. **`find_floor()` calls `EnumWindows()` twice per pet per tick** — enumerates all visible system windows with no caching (O(N×M) per tick where N = pets, M = system windows).
3. **New GDI DC + DIB section created on every `render_frame()` call** — even though GDI objects can be reused across frames.

## Goals

- Debug builds feel smooth with ≥1 pet at 60fps.
- `find_floor()` cost is amortized via a short-lived cache.
- GDI objects are reused across frames.
- Criterion benchmarks cover the key hotspots so regressions are caught early.
- A stress integration test asserts tick performance under load.

## Non-Goals

- Direct2D / DXGI rendering rewrite.
- SIMD in `blit_frame`.
- WinEvent-based cache invalidation (TTL is simpler and sufficient).
- Changing the animation or behavior logic.

---

## Design

### 1. Cargo Profile Changes (`Cargo.toml`)

```toml
[profile.dev.package."*"]
opt-level = 2

[profile.release]
opt-level = 3   # was "s"
lto = true
strip = true
```

`[profile.dev.package."*"]` compiles all dependencies (wgpu, egui, image, windows-sys, etc.) at opt-level 2 while the application crate itself stays at opt-level 0 — preserving line numbers, variable inspection, and panic backtraces for application code. This is the standard fix for wgpu/egui debug lag.

`opt-level = 3` in release replaces `opt-level = "s"` (size-optimised). The binary will be slightly larger but meaningfully faster at runtime.

---

### 2. `find_floor()` Surface Cache (`src/window/surfaces.rs`)

**New type:**

```rust
pub struct SurfaceCache {
    entries: Vec<SurfaceRect>,   // cached walkable window rects from last EnumWindows
    expires_at: std::time::Instant,
}

pub struct SurfaceRect {
    pub left: i32,
    pub right: i32,
    pub top: i32,
}
```

**Cache lifetime:** 250 ms TTL. On a cache hit, `find_floor()` scans the cached rects instead of calling `EnumWindows`. On a cache miss it calls `EnumWindows`, rebuilds the cache, and sets `expires_at = Instant::now() + Duration::from_millis(250)`.

**API change:**

```rust
// Before
pub fn find_floor(pet_x: i32, pet_y: i32, pet_w: i32, pet_h: i32,
                  screen_w: i32, screen_h: i32) -> i32

// After
pub fn find_floor(pet_x: i32, pet_y: i32, pet_w: i32, pet_h: i32,
                  screen_w: i32, screen_h: i32,
                  cache: &mut SurfaceCache) -> i32
```

`SurfaceCache` implements `Default` (empty cache, already expired). `App` holds one `SurfaceCache` and passes a mutable reference to each `pet.tick()` call. All pets share the same cache per tick so `EnumWindows` is called at most once per 250 ms regardless of pet count.

---

### 3. GDI Object Cache (`src/window/pet_window.rs`)

`PetWindow` gains three additional fields to hold live GDI objects:

```rust
pub struct PetWindow {
    pub hwnd: HWND,
    pub width: u32,
    pub height: u32,
    pub frame_buf: Vec<u8>,
    // --- GDI cache ---
    mem_dc: HDC,
    dib: HBITMAP,
    dib_bits: *mut u8,   // raw pointer into DIB pixel data (owned by dib)
}
```

**Lifecycle:**
- `mem_dc` and `dib` are created once in `PetWindow::create()` for the initial `width × height`.
- `render_frame()` writes premultiplied BGRA pixels directly into `dib_bits` (the memory-mapped DIB buffer) instead of creating a new DIB each call.
- If `width` or `height` change (pet scale change), the old `dib` and `mem_dc` are destroyed and recreated at the new size.
- `Drop` destroys `dib` and `mem_dc`.

`dib_bits` is a raw pointer into memory owned by `dib`. It is valid for the lifetime of `dib`. `PetWindow` is `!Send` (it holds Win32 handles), so there is no threading concern.

---

### 4. Criterion Benchmarks (`benches/`)

Add `criterion = "0.5"` to `[dev-dependencies]`.

**`benches/surfaces.rs`**
- `find_floor_cold` — calls `find_floor()` with a fresh (expired) `SurfaceCache`, forcing `EnumWindows`. Measures real-world cost of a cache miss.
- `find_floor_cached` — calls `find_floor()` with a warm cache. Measures cache-hit fast path.

**`benches/render.rs`**
- `blit_frame_1x`, `blit_frame_2x`, `blit_frame_4x` — calls `blit_frame()` with a 32×32 source frame at scales 1, 2, 4. Measures pixel-copy/premultiply cost.

**`benches/animation.rs`**
- `animation_tick` — calls `AnimationState::tick()` 1 000 times over a full tag loop.
- `behavior_tick` — calls `BehaviorAi::tick()` 1 000 times across all state transitions.

Run with `cargo bench`. HTML reports written to `target/criterion/`.

---

### 5. Stress Integration Test (`tests/integration/test_stress.rs`)

Two sub-tests in a single file, both using the embedded `esheep` sprite:

**`tick_1000_frames_10_pets`**
- Creates 10 `PetInstance` structs with real `PetWindow` handles.
- Runs 1 000 ticks of `pet.tick(delta_ms=16, cache)` for each pet.
- Asserts total wall time ≤ 500 ms (50 µs per tick per pet — 320× headroom vs the 16 ms budget).
- Cleans up windows after the test.

**`render_frame_100_times`**
- Creates 1 `PetWindow`.
- Calls `render_frame()` 100 times with the same frame data.
- Asserts total wall time ≤ 50 ms (500 µs per render — well within budget).

Both tests run on the test thread, which has a Win32 message queue by default on Windows. No mocking required.

---

## File Changes Summary

| File | Change |
|------|--------|
| `Cargo.toml` | Add `[profile.dev.package."*"]`, change release `opt-level` |
| `src/window/surfaces.rs` | Add `SurfaceCache`, `SurfaceRect`; update `find_floor` signature |
| `src/window/pet_window.rs` | Add `mem_dc`, `dib`, `dib_bits` fields; update `create`, `render_frame`, `Drop` |
| `src/app.rs` | Add `surface_cache: SurfaceCache` to `App`; pass to `pet.tick()` |
| `Cargo.toml` (dev-deps) | Add `criterion = "0.5"` |
| `benches/surfaces.rs` | New — surface cache benchmarks |
| `benches/render.rs` | New — blit_frame benchmarks |
| `benches/animation.rs` | New — animation + behavior tick benchmarks |
| `tests/integration/test_stress.rs` | New — stress + render timing tests |
| `tests/integration.rs` | Add `mod test_stress` |
