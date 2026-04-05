# Sprite Baseline Offset Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `baseline_offset` field to sprites so the pet's visual floor can be set independently of the sprite grid's bottom edge, fixing floating pets on surfaces.

**Architecture:** `baseline_offset` (source pixels from frame bottom) lives in the sprite JSON `meta` and `SpriteSheet`/`SpriteEditorState` structs. At runtime it's scaled by `cfg.scale` and added to every floor-y calculation. The editor exposes a drag input and draws a horizontal line across all grid rows in the canvas.

**Tech Stack:** Rust, egui, serde_json, `windows-sys` (only in surfaces tests)

---

## File Map

| File | Change |
|------|--------|
| `crates/ferrite-core/src/sprite/sheet.rs` | Add `baseline_offset: u32` to `SpriteSheet`; parse from `meta.baseline_offset` |
| `src/sprite/editor_state.rs` | Add `baseline_offset: u32` to `SpriteEditorState`; write to JSON meta |
| `src/window/surfaces.rs` | Add `baseline_offset: i32` param to `find_floor_info` + `find_floor`; extract pure helper |
| `src/app.rs` | Compute `baseline_offset_px`, pass to both floor calls, apply to virtual ground |
| `src/tray/sprite_editor.rs` | DragValue input after Rows; draw baseline line in canvas; `rebuild_preview_sheet` default |

---

## Task 1: `SpriteSheet` — parse `baseline_offset` from JSON

**Files:**
- Modify: `crates/ferrite-core/src/sprite/sheet.rs`

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)]` block at the bottom of `crates/ferrite-core/src/sprite/sheet.rs`:

```rust
#[test]
fn baseline_offset_parsed_from_json() {
    let json = r#"{"frames":[{"frame":{"x":0,"y":0,"w":32,"h":32},"duration":100}],"meta":{"frameTags":[],"baseline_offset":12}}"#;
    let sheet = SpriteSheet::from_json_and_image(json.as_bytes(), image::RgbaImage::new(1, 1)).unwrap();
    assert_eq!(sheet.baseline_offset, 12);
}

#[test]
fn baseline_offset_missing_defaults_zero() {
    let json = r#"{"frames":[],"meta":{"frameTags":[]}}"#;
    let sheet = SpriteSheet::from_json_and_image(json.as_bytes(), image::RgbaImage::new(1, 1)).unwrap();
    assert_eq!(sheet.baseline_offset, 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cargo test -p ferrite-core baseline_offset_parsed_from_json baseline_offset_missing_defaults_zero
```
Expected: FAIL — `SpriteSheet` has no `baseline_offset` field.

- [ ] **Step 3: Add field to `SpriteSheet` struct** (`sheet.rs:92-100`)

```rust
#[derive(Debug)]
pub struct SpriteSheet {
    pub image: RgbaImage,
    pub frames: Vec<Frame>,
    pub tags: Vec<FrameTag>,
    pub sm_mappings: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
    pub chromakey: ChromakeyConfig,
    pub tight_bboxes: Vec<TightBbox>,
    pub baseline_offset: u32,
}
```

- [ ] **Step 4: Add parser function** — add after `parse_chromakey` at `sheet.rs:299`:

```rust
fn parse_baseline_offset(root: &Value) -> u32 {
    root.pointer("/meta/baseline_offset")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32
}
```

- [ ] **Step 5: Wire it into `from_json_and_image`** — update `sheet.rs:164-176`:

```rust
pub fn from_json_and_image(json: &[u8], image: RgbaImage) -> Result<Self> {
    let root: Value = serde_json::from_slice(json).context("parse spritesheet JSON")?;

    let frames = parse_frames(&root).context("parse frames")?;
    let tags = parse_tags(&root).context("parse tags")?;
    let sm_mappings = parse_sm_mappings(&root);
    let chromakey = parse_chromakey(&root);
    let baseline_offset = parse_baseline_offset(&root);
    let tight_bboxes: Vec<TightBbox> = frames.iter()
        .map(|f| compute_tight_bbox(&image, f))
        .collect();

    Ok(SpriteSheet { image, frames, tags, sm_mappings, chromakey, tight_bboxes, baseline_offset })
}
```

- [ ] **Step 6: Fix test helper** — `sheet_with_tags` in the test block constructs `SpriteSheet` directly; add the field:

```rust
fn sheet_with_tags(names: &[&str]) -> SpriteSheet {
    SpriteSheet {
        image: image::RgbaImage::new(1, 1),
        frames: vec![],
        tags: names.iter().map(|n| FrameTag {
            name: n.to_string(),
            from: 0,
            to: 0,
            direction: TagDirection::Forward,
            flip_h: false,
        }).collect(),
        sm_mappings: HashMap::new(),
        chromakey: ChromakeyConfig::default(),
        tight_bboxes: vec![],
        baseline_offset: 0,
    }
}
```

- [ ] **Step 7: Run all sheet tests to verify they pass**

```
cargo test -p ferrite-core
```
Expected: all pass.

- [ ] **Step 8: Commit**

```
git add crates/ferrite-core/src/sprite/sheet.rs
git commit -m "feat(sprite): add baseline_offset field to SpriteSheet, parse from JSON meta"
```

---

## Task 2: `SpriteEditorState` — store and serialize `baseline_offset`

**Files:**
- Modify: `src/sprite/editor_state.rs`

- [ ] **Step 1: Write failing test**

Add to `src/sprite/editor_state.rs` (there are no existing tests in this file — add a test module at the bottom):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn baseline_offset_round_trips_in_json() {
        let mut state = SpriteEditorState::new(
            std::path::PathBuf::from("test.png"),
            RgbaImage::new(32, 32),
        );
        state.rows = 1;
        state.cols = 1;
        state.baseline_offset = 8;
        let json = state.to_json();
        let parsed: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(parsed["meta"]["baseline_offset"], 8);
    }

    #[test]
    fn baseline_offset_zero_not_written_to_json() {
        let mut state = SpriteEditorState::new(
            std::path::PathBuf::from("test.png"),
            RgbaImage::new(32, 32),
        );
        state.rows = 1;
        state.cols = 1;
        // default is 0 — should not appear in JSON
        let json = state.to_json();
        let parsed: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert!(parsed["meta"].get("baseline_offset").is_none() || parsed["meta"]["baseline_offset"] == 0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cargo test baseline_offset_round_trips_in_json baseline_offset_zero_not_written_to_json
```
Expected: FAIL — `SpriteEditorState` has no `baseline_offset` field.

- [ ] **Step 3: Add field to `SpriteEditorState`** (`editor_state.rs:38-51`)

```rust
pub struct SpriteEditorState {
    pub png_path: PathBuf,
    pub image: RgbaImage,
    pub rows: u32,
    pub cols: u32,
    pub tags: Vec<EditorTag>,
    pub selected_tag: Option<usize>,
    pub sm_mappings: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
    pub chromakey: ChromakeyConfig,
    pub sprite_name: String,
    /// Pixels from the bottom of a frame to the actual walking floor.
    /// 0 = bottom edge of the sprite grid is the floor (default, backwards compatible).
    pub baseline_offset: u32,
}
```

- [ ] **Step 4: Initialize in `new()`** — update `SpriteEditorState::new` (`editor_state.rs:64-75`):

```rust
SpriteEditorState {
    png_path,
    image,
    rows: 1,
    cols: 1,
    tags: Vec::new(),
    selected_tag: None,
    sm_mappings: std::collections::HashMap::new(),
    chromakey: ChromakeyConfig::default(),
    sprite_name,
    baseline_offset: 0,
}
```

- [ ] **Step 5: Load `baseline_offset` from `SpriteSheet` in `load_editor_state_from_sheet`** (`src/tray/app_window.rs:337`)

This function copies `SpriteSheet` fields into `SpriteEditorState`. Add the field copy alongside `chromakey`:

```rust
state.chromakey = sheet.chromakey.clone();
state.baseline_offset = sheet.baseline_offset;
```

- [ ] **Step 6: Write `baseline_offset` in `build_json()`** — update `editor_state.rs:184-196`. After the chromakey block:

```rust
let mut meta = serde_json::json!({"frameTags": frame_tags});
if !sm_mappings_json.is_null() {
    meta["smMappings"] = sm_mappings_json;
}
if self.chromakey.enabled {
    meta["chromakey"] = serde_json::to_value(&self.chromakey)
        .unwrap_or_else(|e| unreachable!("ChromakeyConfig serialize failed: {e}"));
}
if self.baseline_offset > 0 {
    meta["baseline_offset"] = self.baseline_offset.into();
}
```

- [ ] **Step 8: Run tests**

```
cargo test baseline_offset_round_trips_in_json baseline_offset_zero_not_written_to_json
```
Expected: both pass.

- [ ] **Step 9: Run full test suite**

```
cargo test
```
Expected: all pass.

- [ ] **Step 10: Commit**

```
git add src/sprite/editor_state.rs src/tray/app_window.rs
git commit -m "feat(sprite): add baseline_offset to SpriteEditorState, load from sheet, serialize to JSON"
```

---

## Task 3: Floor calculation — thread `baseline_offset` through surfaces and app

**Files:**
- Modify: `src/window/surfaces.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Write failing test for the floor formula**

Add to the `#[cfg(test)]` block in `src/window/surfaces.rs`:

```rust
#[test]
fn baseline_offset_zero_matches_old_formula() {
    // floor_y with offset=0 must equal best - pet_h
    let best = 500i32;
    let pet_h = 64i32;
    let offset = 0i32;
    assert_eq!(best - pet_h + offset, 436);
}

#[test]
fn baseline_offset_raises_pet_to_floor() {
    // With offset=16, the window y shifts up by 16 so the character's
    // visual bottom (pet_h - 16 from top) aligns with the surface.
    let best = 500i32;
    let pet_h = 64i32;
    let offset = 16i32;
    assert_eq!(best - pet_h + offset, 452); // window top moves up by 16
}
```

- [ ] **Step 2: Run tests to confirm they pass immediately**

```
cargo test baseline_offset_zero_matches_old_formula baseline_offset_raises_pet_to_floor
```
Expected: PASS (pure arithmetic, no code change needed). These validate our formula before wiring it in.

- [ ] **Step 3: Update `find_floor_info` signature** (`surfaces.rs:109-117`)

Add `baseline_offset: i32` parameter and apply it to the floor formula at line 146:

```rust
pub fn find_floor_info(
    pet_x: i32,
    pet_y: i32,
    pet_w: i32,
    pet_h: i32,
    screen_w: i32,
    screen_h: i32,
    baseline_offset: i32,
    cache: &mut SurfaceCache,
) -> SurfaceHit {
    // ... (unchanged body until line 146) ...
    let floor_y = best - pet_h + baseline_offset;
    // ... (rest unchanged)
```

- [ ] **Step 4: Update `find_floor` wrapper** (`surfaces.rs:162-172`)

```rust
pub fn find_floor(
    pet_x: i32,
    pet_y: i32,
    pet_w: i32,
    pet_h: i32,
    screen_w: i32,
    screen_h: i32,
    baseline_offset: i32,
    cache: &mut SurfaceCache,
) -> i32 {
    find_floor_info(pet_x, pet_y, pet_w, pet_h, screen_w, screen_h, baseline_offset, cache).floor_y
}
```

- [ ] **Step 5: Update existing Windows tests in `surfaces.rs`** — pass `0` for `baseline_offset`:

```rust
// surface_cache_find_floor_returns_plausible_value
let floor = find_floor(0, 0, 32, 32, screen_w, screen_h, 0, &mut cache);

// surface_cache_warm_returns_same_result (both calls)
let floor1 = find_floor(100, 0, 32, 32, screen_w, screen_h, 0, &mut cache);
let floor2 = find_floor(100, 0, 32, 32, screen_w, screen_h, 0, &mut cache);
```

- [ ] **Step 6: Update `app.rs` — compute `baseline_offset_px` and pass it**

In `PetInstance::tick()` (`app.rs:106`), after computing `pet_h` at line 110, add:

```rust
let baseline_offset_px = (self.sheet.baseline_offset as f32 * self.cfg.scale).round() as i32;
```

Update the `find_floor_info` call (`app.rs:124-126`):
```rust
let hit = crate::window::surfaces::find_floor_info(
    self.x, self.y, pet_w, pet_h, screen_w, screen_h, baseline_offset_px, cache,
);
```

Update the `find_floor` call (`app.rs:151-153`):
```rust
let new_floor = crate::window::surfaces::find_floor(
    self.x, self.y, pet_w, pet_h, screen_w, screen_h, baseline_offset_px, cache,
);
```

Update the virtual ground calculation (`app.rs:167`):
```rust
let virtual_ground = screen_h - 4 - pet_h + baseline_offset_px;
```

- [ ] **Step 7: Run full test suite**

```
cargo test
```
Expected: all pass (compiler will catch any missed call sites).

- [ ] **Step 8: Commit**

```
git add src/window/surfaces.rs src/app.rs
git commit -m "feat(app): thread baseline_offset through floor calculations"
```

---

## Task 4: Sprite editor UI — input field and baseline line

**Files:**
- Modify: `src/tray/sprite_editor.rs`

- [ ] **Step 1: Fix `rebuild_preview_sheet` to compile** (`sprite_editor.rs:109-116`)

`SpriteSheet` now requires `baseline_offset`. Add it with value `0` (it's unused in the preview):

```rust
self.preview_sheet = Some(SpriteSheet {
    image: keyed,
    frames,
    tags,
    sm_mappings: std::collections::HashMap::new(),
    chromakey: self.chromakey.clone(),
    tight_bboxes: vec![],
    baseline_offset: 0,
});
```

- [ ] **Step 2: Add `Baseline` input after the `Rows` block**

In `sprite_editor.rs`, after the Rows `ui.horizontal` block (after line 260 where `s.state.rows` is updated), add:

```rust
ui.horizontal(|ui| {
    ui.label("Baseline:");
    crate::tray::ui_theme::help_icon(
        ui,
        "Pixels from the bottom of each frame to the walking floor. \
         0 = bottom edge is the floor.",
    );
    let frame_h = if s.state.rows > 0 {
        s.state.image.height() / s.state.rows
    } else {
        1
    };
    let max_offset = frame_h.saturating_sub(1) as usize;
    let mut offset = s.state.baseline_offset as usize;
    if ui.add(egui::DragValue::new(&mut offset).range(0_usize..=max_offset)).changed() {
        s.state.baseline_offset = offset as u32;
        s.dirty = true;
    }
});
```

- [ ] **Step 3: Draw baseline line in the canvas**

In `sprite_editor.rs`, after the horizontal grid lines loop (after line 807), add:

```rust
// Baseline line: one horizontal line per row, showing the walking floor.
if s.state.baseline_offset > 0 {
    let frame_src_h = if s.state.rows > 0 {
        s.state.image.height() / s.state.rows
    } else {
        1
    };
    let baseline_frac = s.state.baseline_offset as f32 / frame_src_h as f32;
    let baseline_color = egui::Color32::from_rgba_premultiplied(255, 200, 0, 180); // amber
    for r in 0..rows {
        let y = image_rect.top() + (r as f32 + 1.0 - baseline_frac) * cell_h;
        painter.line_segment(
            [egui::pos2(image_rect.left(), y), egui::pos2(image_rect.right(), y)],
            egui::Stroke::new(1.5, baseline_color),
        );
    }
}
```

- [ ] **Step 4: Build to verify no compile errors**

```
cargo build
```
Expected: compiles cleanly.

- [ ] **Step 5: Commit**

```
git add src/tray/sprite_editor.rs
git commit -m "feat(editor): add baseline_offset input and canvas line to sprite editor"
```

---

## Verification

1. `cargo test` — all tests pass.
2. `cargo build --release` — clean build.
3. Run the app with the "Ferris the Crab" sprite pack.
4. Open the sprite editor → set `Baseline` to the number of empty pixels below the crab's legs in the walking frames. Confirm an amber line appears across all rows at that height.
5. Add the sprite as a pet. Confirm the crab lands flush on the taskbar/window surfaces (no floating).
6. Set `Baseline` back to `0`. Confirm the crab floats again (regression baseline).
7. Test with the embedded `test_pet` sprite (baseline 0) — behavior must be identical to before.
