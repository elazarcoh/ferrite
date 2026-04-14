# Engine Cleanup B1–B4 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Four small, independent cleanup tasks in desktop platform code deferred from the core-computation-centralization PR: extract `DragState`, deduplicate distance formulas, centralise scale rounding, and document the `PetDragStart` coordinate frame.

**Architecture:** All changes stay in `src/` (desktop-only platform code). No `ferrite-core` changes. Each task is independently committable. No new files are created — `DragState` lives as an inner struct in `wndproc.rs`. B3 adds a single private inline function in `app.rs`.

**Tech Stack:** Rust, `windows-sys`, `crossbeam-channel`. No egui, no WASM.

---

## File Map

| File | Change |
|------|--------|
| `src/window/wndproc.rs` | B1 — add `DragState` inner struct; replace 8 loose fields in `HwndData` |
| `src/app.rs` | B2 — extract `dist2d` helper; B3 — extract `scale_round` helper |
| `src/event.rs` | B4 — add doc comment on `PetDragStart` coordinate frame |

---

## Task 1 (B1): Extract `DragState` struct in `wndproc.rs`

**Files:**
- Modify: `src/window/wndproc.rs`

The 8 drag-related fields on `HwndData` (lines 44–57) are replaced with a single `drag: DragState` field. All read/write sites inside the same file are updated. No callers outside the file touch drag fields directly.

Current loose fields:
```
mouse_down, cursor_down_screen, win_down_pos, drag_active,
drag_start_sent, vel_prev, vel_cur
```
(Note: `alpha_buf` and `buf_width` stay on `HwndData` — they are not drag-related.)

- [ ] **Step 1: Add `DragState` struct above `HwndData`**

Insert the following block immediately before the `struct HwndData` definition (around line 38):

```rust
/// All mutable state that exists only while a drag gesture may be in progress.
/// Reset to `DragState::default()` on `WM_LBUTTONDOWN`.
#[derive(Default)]
struct DragState {
    /// Left button is held down.
    mouse_down: bool,
    /// Screen position of the initial mousedown.
    cursor_down_screen: (i32, i32),
    /// Window top-left at the time of mousedown.
    win_down_pos: (i32, i32),
    /// Movement threshold (5 px) exceeded — dragging is active.
    drag_active: bool,
    /// PetDragStart event has been sent.
    drag_start_sent: bool,
    /// Two most-recent cursor screen positions + timestamps for velocity.
    vel_prev: Option<((i32, i32), Instant)>,
    vel_cur: Option<((i32, i32), Instant)>,
}
```

- [ ] **Step 2: Replace the 7 loose fields in `HwndData` with `drag: DragState`**

Change `HwndData` from:
```rust
struct HwndData {
    pet_id: String,
    /// Alpha-only buffer (one byte per pixel, row-major).
    alpha_buf: Vec<u8>,
    buf_width: u32,
    // ── Drag state ────────────────────────────────────────────────────────────
    /// Left button is held down.
    mouse_down: bool,
    /// Screen position of the initial mousedown.
    cursor_down_screen: (i32, i32),
    /// Window top-left at the time of mousedown.
    win_down_pos: (i32, i32),
    /// Movement threshold (5 px) exceeded — dragging is active.
    drag_active: bool,
    /// PetDragStart event has been sent.
    drag_start_sent: bool,
    /// Two most-recent cursor screen positions + timestamps for velocity.
    vel_prev: Option<((i32, i32), Instant)>,
    vel_cur: Option<((i32, i32), Instant)>,
}
```

to:

```rust
struct HwndData {
    pet_id: String,
    /// Alpha-only buffer (one byte per pixel, row-major).
    alpha_buf: Vec<u8>,
    buf_width: u32,
    drag: DragState,
}
```

- [ ] **Step 3: Update `register_hwnd` — remove the 7 loose field initialisers**

Change the `reg.insert(...)` block from:
```rust
reg.insert(
    hwnd as isize,
    HwndData {
        pet_id,
        alpha_buf: Vec::new(),
        buf_width: 0,
        mouse_down: false,
        cursor_down_screen: (0, 0),
        win_down_pos: (0, 0),
        drag_active: false,
        drag_start_sent: false,
        vel_prev: None,
        vel_cur: None,
    },
);
```

to:

```rust
reg.insert(
    hwnd as isize,
    HwndData {
        pet_id,
        alpha_buf: Vec::new(),
        buf_width: 0,
        drag: DragState::default(),
    },
);
```

- [ ] **Step 4: Update `is_mouse_down`**

Change:
```rust
reg.get(&(hwnd as isize)).map(|d| d.mouse_down).unwrap_or(false)
```

to:

```rust
reg.get(&(hwnd as isize)).map(|d| d.drag.mouse_down).unwrap_or(false)
```

- [ ] **Step 5: Update `WM_LBUTTONDOWN` handler**

Change the block inside `WM_LBUTTONDOWN` from:
```rust
if let Some(data) = reg.get_mut(&(hwnd as isize)) {
    data.mouse_down = true;
    data.cursor_down_screen = (cursor_screen.x, cursor_screen.y);
    data.win_down_pos = (rc.left, rc.top);
    data.drag_active = false;
    data.drag_start_sent = false;
    data.vel_prev = None;
    data.vel_cur =
        Some(((cursor_screen.x, cursor_screen.y), Instant::now()));
}
```

to:

```rust
if let Some(data) = reg.get_mut(&(hwnd as isize)) {
    data.drag = DragState {
        mouse_down: true,
        cursor_down_screen: (cursor_screen.x, cursor_screen.y),
        win_down_pos: (rc.left, rc.top),
        vel_cur: Some(((cursor_screen.x, cursor_screen.y), Instant::now())),
        ..DragState::default()
    };
}
```

- [ ] **Step 6: Update `WM_MOUSEMOVE` — read side**

Change the destructuring in the `drag_info` read block from:
```rust
reg.get(&(hwnd as isize)).map(|d| {
    (d.mouse_down, d.cursor_down_screen, d.win_down_pos, d.drag_active, d.drag_start_sent, d.pet_id.clone())
})
```

to:

```rust
reg.get(&(hwnd as isize)).map(|d| {
    (d.drag.mouse_down, d.drag.cursor_down_screen, d.drag.win_down_pos, d.drag.drag_active, d.drag.drag_start_sent, d.pet_id.clone())
})
```

- [ ] **Step 7: Update `WM_MOUSEMOVE` — write side (velocity + drag_active)**

Change:
```rust
if let Some(data) = reg.get_mut(&(hwnd as isize)) {
    data.drag_active = true;
    data.vel_prev = data.vel_cur.take();
    data.vel_cur = Some(((cursor_screen.x, cursor_screen.y), now));
}
```

to:

```rust
if let Some(data) = reg.get_mut(&(hwnd as isize)) {
    data.drag.drag_active = true;
    data.drag.vel_prev = data.drag.vel_cur.take();
    data.drag.vel_cur = Some(((cursor_screen.x, cursor_screen.y), now));
}
```

- [ ] **Step 8: Update `WM_MOUSEMOVE` — write side (drag_start_sent)**

Change:
```rust
if let Some(data) = reg.get_mut(&(hwnd as isize)) {
    data.drag_start_sent = true;
}
```

to:

```rust
if let Some(data) = reg.get_mut(&(hwnd as isize)) {
    data.drag.drag_start_sent = true;
}
```

- [ ] **Step 9: Update `WM_LBUTTONUP` handler**

Change the closure body from:
```rust
reg.get_mut(&(hwnd as isize)).map(|data| {
    let was_drag = data.drag_active;
    let pet_id = data.pet_id.clone();
    let velocity =
        if let (Some((p0, t0)), Some((p1, t1))) = (&data.vel_prev, &data.vel_cur) {
            let dt = t1.duration_since(*t0).as_secs_f32().max(0.001);
            ((p1.0 - p0.0) as f32 / dt, (p1.1 - p0.1) as f32 / dt)
        } else {
            (0.0, 0.0)
        };
    data.mouse_down = false;
    data.drag_active = false;
    data.drag_start_sent = false;
    data.vel_prev = None;
    data.vel_cur = None;
    (was_drag, pet_id, velocity)
})
```

to:

```rust
reg.get_mut(&(hwnd as isize)).map(|data| {
    let was_drag = data.drag.drag_active;
    let pet_id = data.pet_id.clone();
    let velocity =
        if let (Some((p0, t0)), Some((p1, t1))) = (&data.drag.vel_prev, &data.drag.vel_cur) {
            let dt = t1.duration_since(*t0).as_secs_f32().max(0.001);
            ((p1.0 - p0.0) as f32 / dt, (p1.1 - p0.1) as f32 / dt)
        } else {
            (0.0, 0.0)
        };
    data.drag = DragState::default();
    (was_drag, pet_id, velocity)
})
```

- [ ] **Step 10: Build**

```bash
cargo build
```

Expected: compiles with no errors and no new warnings.

- [ ] **Step 11: Commit**

```bash
git add src/window/wndproc.rs
git commit -m "refactor(wndproc): extract DragState inner struct from HwndData

8 loose drag fields consolidated into DragState; reset on WM_LBUTTONDOWN
via DragState::default(). No behavior change."
```

---

## Task 2 (B2): Deduplicate distance formula in `app.rs`

**Files:**
- Modify: `src/app.rs`

The cursor-to-pet and other-pet distance calculations both write out `((dx*dx + dy*dy) as f32).sqrt()`. Extract a private inline helper.

- [ ] **Step 1: Add `dist2d` helper near the top of `app.rs` (after imports, before `struct PetInstance`)**

Find the first `struct` or `impl` block in `app.rs` (around line 30–40) and insert immediately before it:

```rust
/// Euclidean distance between two integer 2-D points.
#[inline]
fn dist2d(ax: i32, ay: i32, bx: i32, by: i32) -> f32 {
    let dx = ax - bx;
    let dy = ay - by;
    ((dx * dx + dy * dy) as f32).sqrt()
}
```

- [ ] **Step 2: Replace the two inline distance blocks in `App::update`**

Find the `cursor_dist` block (around line 691):
```rust
let cursor_dist = {
    let dx = cursor_pt.x - cx;
    let dy = cursor_pt.y - cy;
    ((dx * dx + dy * dy) as f32).sqrt()
};
```

Replace with:
```rust
let cursor_dist = dist2d(cursor_pt.x, cursor_pt.y, cx, cy);
```

Find the `other_pet_dist` block (around line 697–704):
```rust
let other_pet_dist = centers.iter()
    .filter(|(oid, _, _)| oid != id)
    .map(|(_, ox, oy)| {
        let dx = ox - cx;
        let dy = oy - cy;
        ((dx * dx + dy * dy) as f32).sqrt()
    })
    .fold(f32::INFINITY, f32::min);
```

Replace with:
```rust
let other_pet_dist = centers.iter()
    .filter(|(oid, _, _)| oid != id)
    .map(|(_, ox, oy)| dist2d(*ox, *oy, cx, cy))
    .fold(f32::INFINITY, f32::min);
```

- [ ] **Step 3: Build**

```bash
cargo build
```

Expected: compiles cleanly.

- [ ] **Step 4: Commit**

```bash
git add src/app.rs
git commit -m "refactor(app): extract dist2d helper; deduplicate cursor/pet distance formulas"
```

---

## Task 3 (B3): Centralise scale rounding in `app.rs`

**Files:**
- Modify: `src/app.rs`

`(value as f32 * cfg.scale).round() as i32` (or `as u32`) appears in `PetInstance::new` and `PetInstance::tick`. Extract a private inline helper.

- [ ] **Step 1: Add `scale_round` helper next to `dist2d`**

Add immediately after the `dist2d` function you added in Task 2:

```rust
/// Apply a scale factor and round to the nearest integer.
#[inline]
fn scale_round(value: i32, scale: f32) -> i32 {
    (value as f32 * scale).round() as i32
}
```

- [ ] **Step 2: Replace call sites in `PetInstance::new`**

Find (around line 59–60):
```rust
let dw = (frame_w as f32 * cfg.scale).round() as u32;
let dh = (frame_h as f32 * cfg.scale).round() as u32;
```

Replace with:
```rust
let dw = scale_round(frame_w as i32, cfg.scale) as u32;
let dh = scale_round(frame_h as i32, cfg.scale) as u32;
```

- [ ] **Step 3: Replace call site in `PetInstance::tick`**

Find (around line 114):
```rust
let baseline_offset_px = (self.sheet.baseline_offset as f32 * self.cfg.scale).round() as i32;
```

Replace with:
```rust
let baseline_offset_px = scale_round(self.sheet.baseline_offset, self.cfg.scale);
```

- [ ] **Step 4: Build**

```bash
cargo build
```

Expected: compiles cleanly.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "refactor(app): extract scale_round helper; centralise scale rounding"
```

---

## Task 4 (B4): Document `PetDragStart` coordinate frame

**Files:**
- Modify: `src/event.rs`

`PetDragStart` carries screen-space cursor coordinates. The pet-relative offset (`cursor_x - p.x`, `cursor_y - p.y`) is computed later in `App::handle_event`. Without a doc comment this is non-obvious when extending drag handling.

- [ ] **Step 1: Add doc comment to `PetDragStart` in `event.rs`**

Change:
```rust
/// User started dragging a pet.
PetDragStart { pet_id: String, cursor_x: i32, cursor_y: i32 },
```

to:

```rust
/// User started dragging a pet.
///
/// `cursor_x` / `cursor_y` are **screen-space** coordinates (pixels from
/// top-left of the primary monitor, as returned by `GetCursorPos`).
/// The pet-relative grab offset is computed in `App::handle_event` as
/// `(cursor_x - pet.x, cursor_y - pet.y)`.
PetDragStart { pet_id: String, cursor_x: i32, cursor_y: i32 },
```

- [ ] **Step 2: Build**

```bash
cargo build
```

Expected: compiles cleanly.

- [ ] **Step 3: Commit**

```bash
git add src/event.rs
git commit -m "docs(event): document PetDragStart coordinate frame (screen-space)"
```

---

## Task 5: Update BACKLOG.md

- [ ] **Step 1: Mark B1–B4 as done in `BACKLOG.md`**

Change the four `- [ ]` lines in the Engine Cleanup section to `- [x]`.

- [ ] **Step 2: Commit**

```bash
git add BACKLOG.md
git commit -m "chore: mark engine cleanup B1-B4 complete in BACKLOG.md"
```
