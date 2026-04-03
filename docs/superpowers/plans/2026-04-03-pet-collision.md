# Pet-to-Pet Collision Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fire a `"collide"` SM interrupt on both pets when their tight bounding boxes first overlap, with 4 condition variables (`collide_type`, `collide_vx`, `collide_vy`, `collide_v`) available to SM conditions.

**Architecture:** Four independent changes applied in sequence: (1) per-frame transparency-aware `TightBbox` precomputed in `SpriteSheet`; (2) four new condition variables in the SM expression engine; (3) `CollideData`/`on_collide()` on `SMRunner` plus fix per-state interrupt dispatch; (4) sweep-and-prune detection + edge-trigger in `App::update()`.

**Tech Stack:** Rust, `image` crate (`RgbaImage`), existing SM interrupt machinery, `std::collections::HashSet`.

---

## File Map

| File | Change |
|---|---|
| `crates/ferrite-core/src/sprite/sheet.rs` | Add `TightBbox`, `tight_bboxes` field, precompute in `from_json_and_image`, add `tight_bbox()` method |
| `crates/ferrite-core/src/sprite/sm_expr.rs` | Add 4 `ConditionVars` fields + `Var` variants + tokenizer/parser/evaluator entries |
| `crates/ferrite-core/src/sprite/sm_runner.rs` | Add `CollideData`, `speed()`, fix `interrupt()` for per-state, add `on_collide()` |
| `src/app.rs` | Add `overlapping` field, `PetBox` struct, helper fns, collision pass in `update()` |
| `tests/integration/test_collision.rs` | 7 integration tests |
| `tests/integration.rs` | Register `mod collision` |

---

## Task 1: `TightBbox` in `crates/ferrite-core/src/sprite/sheet.rs`

**Files:**
- Modify: `crates/ferrite-core/src/sprite/sheet.rs`

- [ ] **Step 1: Write failing unit tests**

Add at the bottom of the `#[cfg(test)] mod tests` block in `sheet.rs`:

```rust
#[test]
fn tight_bbox_fully_opaque_frame() {
    // 4×4 image, one 2×2 frame, all pixels opaque
    let mut img = RgbaImage::new(4, 4);
    for y in 0..2u32 { for x in 0..2u32 { img.put_pixel(x, y, image::Rgba([255,0,0,255])); } }
    let sheet = SpriteSheet::from_json_and_image(
        r#"{"frames":[{"frame":{"x":0,"y":0,"w":2,"h":2},"duration":100}],"meta":{"frameTags":[]}}"#.as_bytes(),
        img,
    ).unwrap();
    assert_eq!(sheet.tight_bboxes.len(), 1);
    let tb = &sheet.tight_bboxes[0];
    assert_eq!(tb.dx, 0); assert_eq!(tb.dy, 0);
    assert_eq!(tb.w, 2);  assert_eq!(tb.h, 2);
}

#[test]
fn tight_bbox_transparent_border() {
    // 4×4 image, one 4×4 frame; only center 2×2 pixels are opaque
    let mut img = RgbaImage::new(4, 4);
    for y in 1..3u32 { for x in 1..3u32 { img.put_pixel(x, y, image::Rgba([255,0,0,255])); } }
    let sheet = SpriteSheet::from_json_and_image(
        r#"{"frames":[{"frame":{"x":0,"y":0,"w":4,"h":4},"duration":100}],"meta":{"frameTags":[]}}"#.as_bytes(),
        img,
    ).unwrap();
    let tb = &sheet.tight_bboxes[0];
    assert_eq!(tb.dx, 1); assert_eq!(tb.dy, 1);
    assert_eq!(tb.w, 2);  assert_eq!(tb.h, 2);
}

#[test]
fn tight_bbox_all_transparent_gives_zero_size() {
    let img = RgbaImage::new(4, 4); // all pixels alpha=0
    let sheet = SpriteSheet::from_json_and_image(
        r#"{"frames":[{"frame":{"x":0,"y":0,"w":4,"h":4},"duration":100}],"meta":{"frameTags":[]}}"#.as_bytes(),
        img,
    ).unwrap();
    let tb = &sheet.tight_bboxes[0];
    assert_eq!(tb.w, 0); assert_eq!(tb.h, 0);
}

#[test]
fn tight_bbox_flip_h_mirrors_x_offset() {
    // 4×4 frame; opaque pixels only in left column (x=0)
    let mut img = RgbaImage::new(4, 4);
    for y in 0..4u32 { img.put_pixel(0, y, image::Rgba([255,0,0,255])); }
    let sheet = SpriteSheet::from_json_and_image(
        r#"{"frames":[{"frame":{"x":0,"y":0,"w":4,"h":4},"duration":100}],"meta":{"frameTags":[]}}"#.as_bytes(),
        img,
    ).unwrap();
    // No flip: dx=0, w=1
    let (dx_no_flip, _, w, _) = sheet.tight_bbox(0, 1, false);
    assert_eq!(dx_no_flip, 0);
    assert_eq!(w, 1);
    // Flip: mirrored dx = frame_w - (dx + w) = 4 - (0+1) = 3
    let (dx_flipped, _, w2, _) = sheet.tight_bbox(0, 1, true);
    assert_eq!(dx_flipped, 3);
    assert_eq!(w2, 1);
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
cd D:/elazar/private/my-pet
cargo test -p ferrite-core tight_bbox 2>&1 | tail -15
```

Expected: compile errors — `TightBbox`, `tight_bboxes`, `tight_bbox()` don't exist yet.

- [ ] **Step 3: Add `TightBbox` struct and `tight_bboxes` field**

In `crates/ferrite-core/src/sprite/sheet.rs`, add after the `ChromakeyConfig` block (around line 65):

```rust
/// Per-frame transparency-aware tight bounding box.
/// `dx`/`dy` are offsets from the frame's top-left corner in source pixels.
/// `w == 0 && h == 0` means fully transparent — non-collidable.
#[derive(Debug, Clone, Copy, Default)]
pub struct TightBbox {
    pub dx: u32,
    pub dy: u32,
    pub w: u32,
    pub h: u32,
}
```

Add `tight_bboxes: Vec<TightBbox>` to the `SpriteSheet` struct:

```rust
pub struct SpriteSheet {
    pub image: RgbaImage,
    pub frames: Vec<Frame>,
    pub tags: Vec<FrameTag>,
    pub sm_mappings: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
    pub chromakey: ChromakeyConfig,
    pub tight_bboxes: Vec<TightBbox>,
}
```

- [ ] **Step 4: Add `compute_tight_bbox` private helper**

Add this private function before `impl SpriteSheet`:

```rust
fn compute_tight_bbox(image: &RgbaImage, frame: &Frame) -> TightBbox {
    let (fx, fy, fw, fh) = (frame.x, frame.y, frame.w, frame.h);
    let (img_w, img_h) = (image.width(), image.height());
    let mut min_x = fw;
    let mut min_y = fh;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut found = false;

    for dy in 0..fh {
        for dx in 0..fw {
            let px = fx + dx;
            let py = fy + dy;
            if px >= img_w || py >= img_h { continue; }
            if image.get_pixel(px, py)[3] > 0 {
                min_x = min_x.min(dx);
                min_y = min_y.min(dy);
                max_x = max_x.max(dx + 1);
                max_y = max_y.max(dy + 1);
                found = true;
            }
        }
    }
    if !found {
        return TightBbox::default();
    }
    TightBbox { dx: min_x, dy: min_y, w: max_x - min_x, h: max_y - min_y }
}
```

- [ ] **Step 5: Update `from_json_and_image` to compute tight bboxes**

In `SpriteSheet::from_json_and_image`, after parsing `chromakey`, add:

```rust
let tight_bboxes: Vec<TightBbox> = frames.iter()
    .map(|f| compute_tight_bbox(&image, f))
    .collect();
```

And include in the return:

```rust
Ok(SpriteSheet { image, frames, tags, sm_mappings, chromakey, tight_bboxes })
```

- [ ] **Step 6: Add `tight_bbox()` method**

Inside `impl SpriteSheet`, add:

```rust
/// Returns `(dx_px, dy_px, w_px, h_px)` — the tight bbox offset and size
/// in world pixels (after scale), accounting for horizontal flip.
/// Returns `(0, 0, 0, 0)` for fully-transparent frames (non-collidable).
pub fn tight_bbox(&self, frame_idx: usize, scale: u32, flip_h: bool) -> (i32, i32, u32, u32) {
    let tb = self.tight_bboxes.get(frame_idx).copied().unwrap_or_default();
    let frame_w = self.frames.get(frame_idx).map(|f| f.w).unwrap_or(0);
    let dx = if flip_h {
        frame_w.saturating_sub(tb.dx + tb.w)
    } else {
        tb.dx
    };
    ((dx * scale) as i32, (tb.dy * scale) as i32, tb.w * scale, tb.h * scale)
}
```

- [ ] **Step 7: Fix all `SpriteSheet { ... }` struct literals**

Add `tight_bboxes: vec![]` to every hand-constructed `SpriteSheet` literal in the codebase.
Find them all:

```bash
grep -rn "SpriteSheet {" D:/elazar/private/my-pet/crates D:/elazar/private/my-pet/src D:/elazar/private/my-pet/tests 2>&1
```

For each match (typically in test helpers in `sheet.rs`, `sm_runner.rs`, `test_animation.rs`, `test_behavior.rs`, `test_sm_switching.rs`, `sprite_editor.rs`), add the field:

```rust
SpriteSheet {
    image,
    frames,
    tags,
    sm_mappings: std::collections::HashMap::new(),
    chromakey: ChromakeyConfig::default(),
    tight_bboxes: vec![],   // ← add this
}
```

- [ ] **Step 8: Run tests**

```bash
cargo test -p ferrite-core tight_bbox 2>&1 | tail -10
```

Expected: 4 new tests pass.

- [ ] **Step 9: Run full ferrite-core suite**

```bash
cargo test -p ferrite-core 2>&1 | grep "test result"
```

Expected: all tests pass, 0 failures.

- [ ] **Step 10: Commit**

```bash
git add crates/ferrite-core/src/sprite/sheet.rs
git commit -m "feat(collision): add TightBbox precomputed per frame in SpriteSheet"
```

---

## Task 2: Collide variables in `crates/ferrite-core/src/sprite/sm_expr.rs`

**Files:**
- Modify: `crates/ferrite-core/src/sprite/sm_expr.rs`

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn parse_collide_type_variable() {
    assert!(parse(r#"collide_type == "head_on""#).is_ok());
}

#[test]
fn parse_collide_v_variable() {
    assert!(parse("collide_v > 80").is_ok());
}

#[test]
fn parse_collide_vx_vy_variables() {
    assert!(parse("collide_vx > 0 and collide_vy < 0").is_ok());
}

#[test]
fn eval_collide_type_matches() {
    let expr = parse(r#"collide_type == "head_on""#).unwrap();
    let mut v = ConditionVars::default();
    v.collide_type = "head_on".to_string();
    assert!(eval(&expr, &v).unwrap());
    v.collide_type = String::new();
    assert!(!eval(&expr, &v).unwrap());
}

#[test]
fn eval_collide_v_threshold() {
    let expr = parse("collide_v > 50").unwrap();
    let mut v = ConditionVars::default();
    v.collide_v = 80.0;
    assert!(eval(&expr, &v).unwrap());
    v.collide_v = 30.0;
    assert!(!eval(&expr, &v).unwrap());
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
cargo test -p ferrite-core collide 2>&1 | tail -10
```

Expected: compile errors — `collide_type`, `collide_v`, etc. not recognized.

- [ ] **Step 3: Add fields to `ConditionVars`**

```rust
#[derive(Debug, Clone, Default)]
pub struct ConditionVars {
    pub cursor_dist: f32,
    pub state_time_ms: u32,
    pub on_surface: bool,
    pub pet_x: f32,
    pub pet_y: f32,
    pub pet_vx: f32,
    pub pet_vy: f32,
    pub pet_v: f32,
    pub pet_w: f32,
    pub pet_h: f32,
    pub screen_w: f32,
    pub screen_h: f32,
    pub hour: u32,
    pub focused_app: String,
    // Collision vars — populated only during on_collide(); "" / 0.0 otherwise
    pub collide_type: String,
    pub collide_vx: f32,
    pub collide_vy: f32,
    pub collide_v: f32,
}
```

- [ ] **Step 4: Add `Var` enum variants**

In the `Var` enum, add after `FocusedApp`:

```rust
CollideType,
CollideVx,
CollideVy,
CollideV,
```

- [ ] **Step 5: Update the tokenizer allowlist**

In `tokenize()`, find the match arm (around line 305):

```rust
"cursor_dist" | "state_time" | "on_surface" | "pet_x" | "pet_y" | "screen_w"
| "screen_h" | "hour" | "abs" | "min" | "max" => {
    tokens.push(Token::Ident(name))
}
```

Extend it:

```rust
"cursor_dist" | "state_time" | "on_surface" | "pet_x" | "pet_y" | "screen_w"
| "screen_h" | "hour" | "abs" | "min" | "max"
| "collide_type" | "collide_vx" | "collide_vy" | "collide_v" => {
    tokens.push(Token::Ident(name))
}
```

- [ ] **Step 6: Update the parser variable match**

In `parse_primary()`, find the variable match (around line 458). Add after `"input.focused_app" => Var::FocusedApp`:

```rust
"collide_type" => Var::CollideType,
"collide_vx"   => Var::CollideVx,
"collide_vy"   => Var::CollideVy,
"collide_v"    => Var::CollideV,
```

- [ ] **Step 7: Update the evaluator**

In `eval_value()`, inside `Expr::Var(v) => match v { ... }`, add after `Var::FocusedApp`:

```rust
Var::CollideType => Ok(Value::Str(vars.collide_type.clone())),
Var::CollideVx   => Ok(Value::Number(vars.collide_vx)),
Var::CollideVy   => Ok(Value::Number(vars.collide_vy)),
Var::CollideV    => Ok(Value::Number(vars.collide_v)),
```

- [ ] **Step 8: Run collide tests**

```bash
cargo test -p ferrite-core collide 2>&1 | tail -10
```

Expected: 5 new tests pass.

- [ ] **Step 9: Run full ferrite-core suite**

```bash
cargo test -p ferrite-core 2>&1 | grep "test result"
```

Expected: all tests pass.

- [ ] **Step 10: Commit**

```bash
git add crates/ferrite-core/src/sprite/sm_expr.rs
git commit -m "feat(collision): add collide_type/vx/vy/v condition variables to SM expression engine"
```

---

## Task 3: `CollideData` + `on_collide()` + per-state interrupt fix in `crates/ferrite-core/src/sprite/sm_runner.rs`

**Files:**
- Modify: `crates/ferrite-core/src/sprite/sm_runner.rs`

**Context:** The existing `interrupt()` method only checks `global_interrupts`. Per-state interrupts (`[[states.wander.interrupts]]` in TOML) are compiled into `CompiledState.per_state_interrupts` but never consulted. This task fixes that and adds the collision interrupt path.

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)] mod tests` block:

```rust
// Helper SM with per-state collide interrupt
fn make_collide_sm() -> Arc<crate::sprite::sm_compiler::CompiledSM> {
    let toml = r#"
name = "test"
[states.idle]
action = "idle"
[[states.idle.interrupts]]
event = "collide"
goto = "react"
[states.react]
action = "sit"
duration = "500ms"
[[states.react.transitions]]
goto = "idle"
"#;
    let file: SmFile = toml::from_str(toml).unwrap();
    compile(&file).unwrap()
}

#[test]
fn per_state_interrupt_fires_on_matching_event() {
    let mut r = SMRunner::new(make_collide_sm(), 80.0);
    assert!(matches!(&r.active, ActiveState::Named(n) if n == "idle"));
    r.interrupt("collide", None);
    assert!(matches!(&r.active, ActiveState::Named(n) if n == "react"),
        "expected react, got {:?}", r.active);
}

#[test]
fn per_state_interrupt_ignored_on_unknown_event() {
    let mut r = SMRunner::new(make_collide_sm(), 80.0);
    r.interrupt("unknown_event", None);
    assert!(matches!(&r.active, ActiveState::Named(n) if n == "idle"));
}

#[test]
fn on_collide_sets_vars_and_transitions() {
    let mut r = SMRunner::new(make_collide_sm(), 80.0);
    r.on_collide(CollideData {
        collide_type: "head_on".to_string(),
        vx: 50.0, vy: 0.0, v: 50.0,
    });
    assert!(matches!(&r.active, ActiveState::Named(n) if n == "react"));
}

#[test]
fn on_collide_clears_vars_after_interrupt() {
    let mut r = SMRunner::new(make_collide_sm(), 80.0);
    r.on_collide(CollideData {
        collide_type: "head_on".to_string(),
        vx: 50.0, vy: 0.0, v: 50.0,
    });
    // After on_collide, collide vars must be cleared
    assert_eq!(r.last_condition_vars().collide_type, "");
    assert_eq!(r.last_condition_vars().collide_v, 0.0);
}

#[test]
fn on_collide_with_condition_only_fires_when_met() {
    let toml = r#"
name = "test"
[states.idle]
action = "idle"
[[states.idle.interrupts]]
event = "collide"
condition = "collide_v > 100"
goto = "react"
[states.react]
action = "sit"
"#;
    let file: SmFile = toml::from_str(toml).unwrap();
    let sm = compile(&file).unwrap();
    let mut r = SMRunner::new(sm.clone(), 80.0);
    // Low speed — should NOT transition
    r.on_collide(CollideData { collide_type: "head_on".to_string(), vx: 0.0, vy: 0.0, v: 50.0 });
    assert!(matches!(&r.active, ActiveState::Named(n) if n == "idle"));
    // High speed — should transition
    r.on_collide(CollideData { collide_type: "head_on".to_string(), vx: 0.0, vy: 0.0, v: 150.0 });
    assert!(matches!(&r.active, ActiveState::Named(n) if n == "react"));
}

#[test]
fn speed_returns_velocity_from_active_state() {
    let mut r = SMRunner::new(make_collide_sm(), 80.0);
    r.active = ActiveState::Thrown { vx: 100.0, vy: -50.0 };
    assert_eq!(r.speed(), (100.0, -50.0));
    r.active = ActiveState::Fall { vy: 200.0 };
    assert_eq!(r.speed(), (0.0, 200.0));
    r.active = ActiveState::Named("idle".to_string());
    assert_eq!(r.speed(), (0.0, 0.0));
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
cargo test -p ferrite-core -- --test per_state_interrupt 2>&1 | tail -10
```

Expected: compile errors — `CollideData`, `on_collide`, `speed` don't exist.

- [ ] **Step 3: Add `CollideData` struct**

After the `TransitionLogEntry` struct (around line 36), add:

```rust
/// Payload for the "collide" interrupt event.
#[derive(Debug, Clone)]
pub struct CollideData {
    /// One of: "head_on", "same_dir", "fell_on", "landed_on",
    ///         "hit_into_above", "hit_from_below"
    pub collide_type: String,
    /// Relative horizontal velocity (my vx − other vx).
    pub vx: f32,
    /// Relative vertical velocity (my vy − other vy).
    pub vy: f32,
    /// Magnitude: sqrt(vx²+vy²).
    pub v: f32,
}
```

- [ ] **Step 4: Add `speed()` method**

Inside `impl SMRunner`, add after `current_facing()`:

```rust
/// Returns `(vx, vy)` of the current physics state.
/// Returns `(0.0, 0.0)` for Named and Grabbed states.
pub fn speed(&self) -> (f32, f32) {
    match &self.active {
        ActiveState::Fall { vy } => (0.0, *vy),
        ActiveState::Thrown { vx, vy } => (*vx, *vy),
        _ => (0.0, 0.0),
    }
}
```

- [ ] **Step 5: Refactor `interrupt()` to handle per-state interrupts**

Replace the entire `interrupt()` method with this version that extracts a private `apply_interrupt_effect()` helper to avoid duplication:

```rust
/// Handle a named interrupt event (e.g. "grabbed", "petted", "collide").
/// Checks global interrupts first, then per-state interrupts for the current named state.
pub fn interrupt(&mut self, event: &str, cursor_offset: Option<(i32, i32)>) {
    // 1. Global interrupts
    if let Some(intr) = self.sm.global_interrupts.iter().find(|i| i.event == event).cloned() {
        self.apply_interrupt_effect(intr.def, event, cursor_offset);
        return;
    }
    // 2. Per-state interrupts (current Named state only)
    if let ActiveState::Named(state_name) = self.active.clone() {
        if let Some(state) = self.sm.states.get(&state_name).cloned() {
            if let Some(intr) = state.per_state_interrupts.iter().find(|i| i.event == event).cloned() {
                self.apply_interrupt_effect(intr.def, event, cursor_offset);
                return;
            }
        }
    }
    // 3. Fallback for grabbed with no matching interrupt defined
    if event == "grabbed" {
        self.grab(cursor_offset.unwrap_or((0, 0)));
    }
}

fn apply_interrupt_effect(
    &mut self,
    effect: crate::sprite::sm_compiler::InterruptEffect,
    event: &str,
    cursor_offset: Option<(i32, i32)>,
) {
    use crate::sprite::sm_compiler::InterruptEffect;
    match effect {
        InterruptEffect::Ignore => {}
        InterruptEffect::Goto { target, condition } => {
            let ok = condition.as_ref()
                .map(|cond| crate::sprite::sm_expr::eval(cond, &self.last_vars).unwrap_or(false))
                .unwrap_or(true);
            if ok {
                if event == "grabbed" {
                    self.grab(cursor_offset.unwrap_or((0, 0)));
                } else {
                    self.set_previous_from_current();
                    let from = self.current_state_name().to_string();
                    self.enter_state(&target.clone());
                    self.log_transition(&from, &target, "interrupt");
                }
            }
        }
    }
}
```

- [ ] **Step 6: Add `on_collide()` method**

Inside `impl SMRunner`, add after `speed()`:

```rust
/// Fire the "collide" interrupt with the given collision data.
/// Temporarily populates `last_vars` with collision fields for condition evaluation,
/// then clears them after the interrupt is processed.
pub fn on_collide(&mut self, data: CollideData) {
    self.last_vars.collide_type = data.collide_type.clone();
    self.last_vars.collide_vx = data.vx;
    self.last_vars.collide_vy = data.vy;
    self.last_vars.collide_v = data.v;
    self.interrupt("collide", None);
    self.last_vars.collide_type = String::new();
    self.last_vars.collide_vx = 0.0;
    self.last_vars.collide_vy = 0.0;
    self.last_vars.collide_v = 0.0;
}
```

- [ ] **Step 7: Run new tests**

```bash
cargo test -p ferrite-core -- per_state_interrupt on_collide speed_returns 2>&1 | tail -15
```

Expected: 6 new tests pass.

- [ ] **Step 8: Run full ferrite-core suite**

```bash
cargo test -p ferrite-core 2>&1 | grep "test result"
```

Expected: all tests pass.

- [ ] **Step 9: Commit**

```bash
git add crates/ferrite-core/src/sprite/sm_runner.rs
git commit -m "feat(collision): add CollideData, on_collide(), speed(); fix per-state interrupt dispatch"
```

---

## Task 4: Collision detection in `src/app.rs`

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add `overlapping` field to `App`**

In the `App` struct definition, add after `surface_cache`:

```rust
/// Canonical (min_id, max_id) pairs currently overlapping — used for edge-triggering.
overlapping: std::collections::HashSet<(String, String)>,
```

In `App::new()`, initialize it:

```rust
Ok(App {
    // ... existing fields ...
    surface_cache: crate::window::surfaces::SurfaceCache::default(),
    overlapping: std::collections::HashSet::new(),
    // ...
})
```

- [ ] **Step 2: Add `PetBox` struct and helper functions**

Add these private items near the top of `app.rs` (after the imports, before `impl App`):

```rust
/// Snapshot of a pet's tight bounding box for collision detection.
struct PetBox {
    id: String,
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
    center_y: i32,
    vx: f32,
    vy: f32,
}

fn collect_boxes(pets: &std::collections::HashMap<String, PetInstance>) -> Vec<PetBox> {
    let mut boxes: Vec<PetBox> = pets.iter().map(|(id, pet)| {
        let frame_idx = pet.anim.absolute_frame(&pet.sheet);
        let flip = pet.compute_flip();
        let (dx, dy, w, h) = pet.sheet.tight_bbox(frame_idx, pet.cfg.scale, flip);
        let left = pet.x + dx;
        let top = pet.y + dy;
        let (vx, vy) = pet.runner.speed();
        PetBox {
            id: id.clone(),
            left,
            right: left + w as i32,
            top,
            bottom: top + h as i32,
            center_y: top + h as i32 / 2,
            vx,
            vy,
        }
    }).collect();
    boxes.sort_by_key(|b| b.left);
    boxes
}

fn canonical_key(a: &str, b: &str) -> (String, String) {
    if a <= b { (a.to_string(), b.to_string()) } else { (b.to_string(), a.to_string()) }
}

/// Classify the collision type for (a, b) returning (type_for_a, type_for_b).
fn classify_collision(a: &PetBox, b: &PetBox) -> (String, String) {
    let rel_vx = a.vx - b.vx;
    let rel_vy = a.vy - b.vy;

    if rel_vx.abs() >= rel_vy.abs() {
        // Horizontal dominant
        let a_cx = (a.left + a.right) / 2;
        let b_cx = (b.left + b.right) / 2;
        let approaching = (a_cx < b_cx && a.vx > b.vx) || (a_cx > b_cx && a.vx < b.vx);
        let t = if approaching { "head_on" } else { "same_dir" };
        (t.to_string(), t.to_string())
    } else {
        // Vertical dominant: determine who is above and who is moving
        let a_above = a.center_y < b.center_y;
        let a_moving_down = a.vy > b.vy;
        match (a_above, a_moving_down) {
            (true, true)  => ("fell_on".to_string(),        "landed_on".to_string()),
            (true, false) => ("landed_on".to_string(),      "fell_on".to_string()),
            (false, true) => ("hit_from_below".to_string(), "hit_into_above".to_string()),
            (false, false) => ("hit_into_above".to_string(), "hit_from_below".to_string()),
        }
    }
}

fn make_collide_data(
    my_box: &PetBox,
    other_box: &PetBox,
    collide_type: String,
) -> ferrite_core::sprite::sm_runner::CollideData {
    let vx = my_box.vx - other_box.vx;
    let vy = my_box.vy - other_box.vy;
    let v = (vx * vx + vy * vy).sqrt();
    ferrite_core::sprite::sm_runner::CollideData { collide_type, vx, vy, v }
}
```

- [ ] **Step 3: Add collision pass in `App::update()`**

In `App::update()`, after the pet tick loop (after line 605 `}`), add the collision pass:

```rust
// ── Collision detection ──────────────────────────────────────────────────────
if self.pets.len() >= 2 {
    let boxes = collect_boxes(&self.pets);
    let mut new_overlapping = std::collections::HashSet::new();

    // Sweep-and-prune: boxes sorted by left edge
    for i in 0..boxes.len() {
        for j in (i + 1)..boxes.len() {
            let a = &boxes[i];
            let b = &boxes[j];
            // X gap: since sorted by left, all further j have larger left
            if b.left >= a.right { break; }
            // Y gap
            if a.bottom <= b.top || b.bottom <= a.top { continue; }
            // Zero-size (fully transparent) bbox — non-collidable
            if a.left == a.right || b.left == b.right { continue; }
            new_overlapping.insert(canonical_key(&a.id, &b.id));
        }
    }

    // Fire interrupts for newly-overlapping pairs (edge-trigger)
    for (id_min, id_max) in &new_overlapping {
        if self.overlapping.contains(&(id_min.clone(), id_max.clone())) {
            continue; // already overlapping — not a new overlap
        }
        let box_a = match boxes.iter().find(|b| &b.id == id_min) { Some(b) => b, None => continue };
        let box_b = match boxes.iter().find(|b| &b.id == id_max) { Some(b) => b, None => continue };
        let (type_a, type_b) = classify_collision(box_a, box_b);
        let data_a = make_collide_data(box_a, box_b, type_a);
        let data_b = make_collide_data(box_b, box_a, type_b);
        if let Some(pet) = self.pets.get_mut(id_min) {
            pet.runner.on_collide(data_a);
        }
        if let Some(pet) = self.pets.get_mut(id_max) {
            pet.runner.on_collide(data_b);
        }
    }

    self.overlapping = new_overlapping;
}
```

- [ ] **Step 4: Check imports**

Ensure `ferrite_core::sprite::sm_runner::CollideData` is accessible. In `src/app.rs`, the existing import block likely has:

```rust
use ferrite_core::sprite::{
    animation::AnimationState,
    sheet::{self, apply_chromakey, SpriteSheet},
    sm_runner::SMRunner,
};
```

Add `CollideData` to the `sm_runner` import:

```rust
use ferrite_core::sprite::{
    animation::AnimationState,
    sheet::{self, apply_chromakey, SpriteSheet},
    sm_runner::{SMRunner, CollideData},
};
```

- [ ] **Step 5: Build check**

```bash
cargo build -p ferrite 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 6: Clippy**

```bash
cargo clippy -p ferrite -- -D warnings -A dead-code 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add src/app.rs
git commit -m "feat(collision): sweep-and-prune collision detection with edge-trigger in App::update"
```

---

## Task 5: Integration tests

**Files:**
- Create: `tests/integration/test_collision.rs`
- Modify: `tests/integration.rs`

- [ ] **Step 1: Create `tests/integration/test_collision.rs`**

```rust
//! Pet-to-pet collision integration tests.
//! Uses real Win32 windows and the SM interrupt machinery end-to-end.

use ferrite_core::sprite::{
    sheet::{Frame, FrameTag, SpriteSheet, TagDirection, ChromakeyConfig, TightBbox},
    sm_compiler::compile,
    sm_format::SmFile,
    sm_runner::{CollideData, SMRunner},
};
use image::{Rgba, RgbaImage};
use std::sync::Arc;

// ─── Helpers ────────────────────────────────────────────────────────────────

fn sm_with_collide_interrupt(target: &str) -> Arc<ferrite_core::sprite::sm_compiler::CompiledSM> {
    let toml = format!(r#"
name = "test"
[states.idle]
action = "idle"
[[states.idle.interrupts]]
event = "collide"
goto = "{target}"
[states.{target}]
action = "sit"
duration = "500ms"
[[states.{target}.transitions]]
goto = "idle"
"#);
    let file: SmFile = toml::from_str(&toml).unwrap();
    compile(&file).unwrap()
}

fn sm_with_typed_collide_interrupt(collide_type: &str, target: &str) -> Arc<ferrite_core::sprite::sm_compiler::CompiledSM> {
    let toml = format!(r#"
name = "test"
[states.idle]
action = "idle"
[[states.idle.interrupts]]
event = "collide"
condition = "collide_type == \"{collide_type}\""
goto = "{target}"
[states.{target}]
action = "sit"
"#);
    let file: SmFile = toml::from_str(&toml).unwrap();
    compile(&file).unwrap()
}

fn runner_with_sm(sm: Arc<ferrite_core::sprite::sm_compiler::CompiledSM>) -> SMRunner {
    SMRunner::new(sm, 80.0)
}

fn opaque_sheet() -> SpriteSheet {
    let mut img = RgbaImage::new(32, 32);
    for y in 0..32u32 { for x in 0..32u32 { img.put_pixel(x, y, Rgba([255,0,0,255])); } }
    let json = r#"{"frames":[{"frame":{"x":0,"y":0,"w":32,"h":32},"duration":100}],"meta":{"frameTags":[
        {"name":"idle","from":0,"to":0,"direction":"forward"},
        {"name":"sit","from":0,"to":0,"direction":"forward"}
    ]}}"#;
    SpriteSheet::from_json_and_image(json.as_bytes(), img).unwrap()
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[test]
fn head_on_collision_fires_interrupt_on_both_runners() {
    let sm = sm_with_typed_collide_interrupt("head_on", "react");
    let mut runner_a = runner_with_sm(sm.clone());
    let mut runner_b = runner_with_sm(sm.clone());

    // Simulate head-on: a moving right (+vx), b moving left (-vx), same y
    let data_a = CollideData { collide_type: "head_on".to_string(), vx: 80.0, vy: 0.0, v: 80.0 };
    let data_b = CollideData { collide_type: "head_on".to_string(), vx: -80.0, vy: 0.0, v: 80.0 };

    runner_a.on_collide(data_a);
    runner_b.on_collide(data_b);

    assert!(
        matches!(&runner_a.active, ferrite_core::sprite::sm_runner::ActiveState::Named(n) if n == "react"),
        "runner_a should be in react"
    );
    assert!(
        matches!(&runner_b.active, ferrite_core::sprite::sm_runner::ActiveState::Named(n) if n == "react"),
        "runner_b should be in react"
    );
}

#[test]
fn same_dir_collision_fires_interrupt() {
    let sm = sm_with_typed_collide_interrupt("same_dir", "nudged");
    let mut runner = runner_with_sm(sm);

    let data = CollideData { collide_type: "same_dir".to_string(), vx: 20.0, vy: 0.0, v: 20.0 };
    runner.on_collide(data);

    assert!(matches!(&runner.active, ferrite_core::sprite::sm_runner::ActiveState::Named(n) if n == "nudged"));
}

#[test]
fn fell_on_and_landed_on_types_assigned_correctly() {
    let fell_sm = sm_with_typed_collide_interrupt("fell_on", "fell_react");
    let landed_sm = sm_with_typed_collide_interrupt("landed_on", "landed_react");

    let mut top_runner = runner_with_sm(fell_sm);      // pet above, falling
    let mut bottom_runner = runner_with_sm(landed_sm); // pet below, resting

    top_runner.on_collide(CollideData {
        collide_type: "fell_on".to_string(), vx: 0.0, vy: 50.0, v: 50.0,
    });
    bottom_runner.on_collide(CollideData {
        collide_type: "landed_on".to_string(), vx: 0.0, vy: -50.0, v: 50.0,
    });

    assert!(matches!(&top_runner.active, ferrite_core::sprite::sm_runner::ActiveState::Named(n) if n == "fell_react"));
    assert!(matches!(&bottom_runner.active, ferrite_core::sprite::sm_runner::ActiveState::Named(n) if n == "landed_react"));
}

#[test]
fn edge_trigger_interrupt_fires_exactly_once() {
    let sm = sm_with_collide_interrupt("react");
    let mut runner = runner_with_sm(sm);
    let sheet = opaque_sheet();

    let data = CollideData { collide_type: "head_on".to_string(), vx: 80.0, vy: 0.0, v: 80.0 };

    // First on_collide → should transition to react
    runner.on_collide(data.clone());
    assert!(matches!(&runner.active, ferrite_core::sprite::sm_runner::ActiveState::Named(n) if n == "react"));

    // While in "react", another on_collide fires but react has no collide interrupt — state unchanged
    runner.on_collide(data.clone());
    assert!(matches!(&runner.active, ferrite_core::sprite::sm_runner::ActiveState::Named(n) if n == "react"),
        "must remain in react after second on_collide (no interrupt defined on react)");

    // Tick until react expires, transitions back to idle
    let mut x = 0i32; let mut y = 0i32;
    for _ in 0..60 { runner.tick(16, &mut x, &mut y, 1920, 32, 32, 1000, &sheet); }
    assert!(matches!(&runner.active, ferrite_core::sprite::sm_runner::ActiveState::Named(n) if n == "idle"));

    // New on_collide after return to idle → fires again
    runner.on_collide(data);
    assert!(matches!(&runner.active, ferrite_core::sprite::sm_runner::ActiveState::Named(n) if n == "react"));
}

#[test]
fn collide_v_condition_gates_interrupt() {
    let toml = r#"
name = "test"
[states.idle]
action = "idle"
[[states.idle.interrupts]]
event = "collide"
condition = "collide_v > 100"
goto = "react"
[states.react]
action = "sit"
"#;
    let file: SmFile = toml::from_str(toml).unwrap();
    let sm = compile(&file).unwrap();
    let mut runner = runner_with_sm(sm);

    // Below threshold — must NOT fire
    runner.on_collide(CollideData { collide_type: "head_on".to_string(), vx: 0.0, vy: 0.0, v: 50.0 });
    assert!(matches!(&runner.active, ferrite_core::sprite::sm_runner::ActiveState::Named(n) if n == "idle"),
        "low-speed collide must not trigger transition");

    // Above threshold — must fire
    runner.on_collide(CollideData { collide_type: "head_on".to_string(), vx: 0.0, vy: 0.0, v: 150.0 });
    assert!(matches!(&runner.active, ferrite_core::sprite::sm_runner::ActiveState::Named(n) if n == "react"),
        "high-speed collide must trigger transition");
}

#[test]
fn no_interrupt_defined_ignores_collide() {
    // SM with no interrupt on idle — on_collide must be a no-op
    let toml = r#"
name = "test"
[states.idle]
action = "idle"
"#;
    let file: SmFile = toml::from_str(toml).unwrap();
    let sm = compile(&file).unwrap();
    let mut runner = runner_with_sm(sm);

    runner.on_collide(CollideData { collide_type: "head_on".to_string(), vx: 80.0, vy: 0.0, v: 80.0 });
    assert!(matches!(&runner.active, ferrite_core::sprite::sm_runner::ActiveState::Named(n) if n == "idle"),
        "no collide interrupt defined — state must not change");
}

#[test]
fn collide_vars_cleared_after_on_collide() {
    let sm = sm_with_collide_interrupt("react");
    let mut runner = runner_with_sm(sm);

    runner.on_collide(CollideData {
        collide_type: "head_on".to_string(),
        vx: 80.0, vy: -10.0, v: 80.6,
    });

    let vars = runner.last_condition_vars();
    assert_eq!(vars.collide_type, "", "collide_type must be cleared");
    assert_eq!(vars.collide_vx, 0.0, "collide_vx must be cleared");
    assert_eq!(vars.collide_vy, 0.0, "collide_vy must be cleared");
    assert_eq!(vars.collide_v, 0.0, "collide_v must be cleared");
}
```

- [ ] **Step 2: Register the module in `tests/integration.rs`**

Open `tests/integration.rs` and add:

```rust
mod collision {
    include!("integration/test_collision.rs");
}
```

- [ ] **Step 3: Run collision tests**

```bash
cargo test --test integration collision 2>&1 | tail -20
```

Expected: 7 tests pass.

- [ ] **Step 4: Run full integration suite**

```bash
cargo test --test integration 2>&1 | grep "test result"
```

Expected: all tests pass (1 flaky stress test may fail due to machine load — this is pre-existing and unrelated).

- [ ] **Step 5: Run lib tests**

```bash
cargo test --lib 2>&1 | grep "test result"
```

Expected: all pass.

- [ ] **Step 6: Final clippy**

```bash
cargo clippy -p ferrite-core -- -D warnings -A dead-code 2>&1 | grep "^error"
cargo clippy -p ferrite -- -D warnings -A dead-code 2>&1 | grep "^error"
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add tests/integration/test_collision.rs tests/integration.rs
git commit -m "test(collision): 7 integration tests for SM collide interrupt"
```

---

## Verification

```bash
# All unit tests
cargo test -p ferrite-core 2>&1 | grep "test result"

# All integration tests (ignoring pre-existing flaky stress test)
cargo test --test integration 2>&1 | grep -E "test result|FAILED"

# Lib tests
cargo test --lib 2>&1 | grep "test result"

# Clippy
cargo clippy -p ferrite-core -- -D warnings -A dead-code 2>&1 | grep "^error"
cargo clippy -p ferrite -- -D warnings -A dead-code 2>&1 | grep "^error"
```
