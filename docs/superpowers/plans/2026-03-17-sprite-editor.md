# Sprite Editor — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an in-app sprite editor window that lets users define a uniform grid over a PNG, assign frame ranges to named tags with behavior mappings, and save or export a standard Aseprite-compatible JSON + PNG pair.

**Architecture:** Four sequential tasks: (1) add `load_with_tag_map` to the existing sheet parser, (2) create pure-Rust `SpriteEditorState` with JSON serialization, (3) build the Win32 editor window using the same `Box::into_raw`/`Box::from_raw` ownership pattern as the config dialog, (4) wire "Edit…" and "New from PNG…" buttons into the config dialog.

**Tech Stack:** Rust, windows-sys 0.61 (Win32 raw FFI), serde_json, image crate, crossbeam-channel

---

## Chunk 1: Pure Rust data layer

### Task 1: Add `load_with_tag_map` to `src/sprite/sheet.rs`

**Files:**
- Modify: `src/sprite/sheet.rs`

`load_with_tag_map` parses the non-standard `meta.myPetTagMap` field from Aseprite JSON alongside the normal sheet. Rules: if the field is absent → `None`; if `idle` or `walk` is missing/empty → `None` (entire map dropped); optional fields that are non-strings → silently ignored.

- [ ] **Step 1: Write the failing tests**

Add these tests inside the existing `#[cfg(test)] mod tests { ... }` block in `src/sprite/sheet.rs`, after the existing tests. They import `AnimTagMap` from `crate::sprite::behavior`:

```rust
use crate::sprite::behavior::AnimTagMap;

#[test]
fn load_with_tag_map_absent_returns_none() {
    // test_pet.json has no myPetTagMap field
    let (_, tag_map) = load_with_tag_map(test_json(), test_png()).unwrap();
    assert!(tag_map.is_none(), "no myPetTagMap → None");
}

#[test]
fn load_with_tag_map_round_trip() {
    let json = r#"{
        "frames": [
            {"frame":{"x":0,"y":0,"w":32,"h":32},"duration":100},
            {"frame":{"x":32,"y":0,"w":32,"h":32},"duration":100}
        ],
        "meta": {
            "frameTags": [{"name":"idle","from":0,"to":1,"direction":"forward"}],
            "myPetTagMap": {"idle":"idle_loop","walk":"walk_cycle","run":"run_fast"}
        }
    }"#;
    let (sheet, tag_map) = load_with_tag_map(json.as_bytes(), test_png()).unwrap();
    assert_eq!(sheet.frames.len(), 2);
    let tm = tag_map.expect("should have tag map");
    assert_eq!(tm.idle, "idle_loop");
    assert_eq!(tm.walk, "walk_cycle");
    assert_eq!(tm.run, Some("run_fast".into()));
    assert_eq!(tm.sit, None);
}

#[test]
fn load_with_tag_map_missing_required_drops_map() {
    let json = r#"{
        "frames": [{"frame":{"x":0,"y":0,"w":32,"h":32},"duration":100}],
        "meta": {"frameTags": [], "myPetTagMap": {"idle":"idle"}}
    }"#;
    let (_, tag_map) = load_with_tag_map(json.as_bytes(), test_png()).unwrap();
    assert!(tag_map.is_none(), "missing walk → None");
}

#[test]
fn load_with_tag_map_empty_required_drops_map() {
    let json = r#"{
        "frames": [{"frame":{"x":0,"y":0,"w":32,"h":32},"duration":100}],
        "meta": {"frameTags": [], "myPetTagMap": {"idle":"","walk":"walk"}}
    }"#;
    let (_, tag_map) = load_with_tag_map(json.as_bytes(), test_png()).unwrap();
    assert!(tag_map.is_none(), "empty idle → None");
}

#[test]
fn load_with_tag_map_bad_optional_ignored() {
    let json = r#"{
        "frames": [{"frame":{"x":0,"y":0,"w":32,"h":32},"duration":100}],
        "meta": {"frameTags": [], "myPetTagMap": {"idle":"idle","walk":"walk","run":42}}
    }"#;
    let (_, tag_map) = load_with_tag_map(json.as_bytes(), test_png()).unwrap();
    let tm = tag_map.expect("map returned despite bad optional");
    assert_eq!(tm.idle, "idle");
    assert_eq!(tm.walk, "walk");
    assert_eq!(tm.run, None, "non-string run silently ignored");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd D:/elazar/private/my-pet && cargo test load_with_tag_map -- --test-threads=1
```

Expected: FAIL with `error[E0425]: cannot find function load_with_tag_map`

- [ ] **Step 3: Implement `load_with_tag_map` and `parse_my_pet_tag_map`**

Add these to `src/sprite/sheet.rs` (after the `load_embedded` function, before the `#[cfg(test)]` block):

```rust
/// Load a spritesheet and, if present, the `myPetTagMap` behavior mapping.
/// Returns `(sheet, None)` if `myPetTagMap` is absent or has missing/empty
/// required fields (`idle` or `walk`). Optional fields that are non-strings
/// are silently ignored.
pub fn load_with_tag_map(
    json_bytes: &[u8],
    png_bytes: &[u8],
) -> Result<(SpriteSheet, Option<crate::sprite::behavior::AnimTagMap>)> {
    let sheet = load_embedded(json_bytes, png_bytes)?;
    let root: Value = serde_json::from_slice(json_bytes)
        .context("re-parse JSON for myPetTagMap")?;
    let tag_map = parse_my_pet_tag_map(&root);
    Ok((sheet, tag_map))
}

fn parse_my_pet_tag_map(
    root: &Value,
) -> Option<crate::sprite::behavior::AnimTagMap> {
    let map = root.pointer("/meta/myPetTagMap")?.as_object()?;
    let idle = map.get("idle")?.as_str().filter(|s| !s.is_empty())?.to_string();
    let walk = map.get("walk")?.as_str().filter(|s| !s.is_empty())?.to_string();
    let opt = |key: &str| map.get(key).and_then(|v| v.as_str()).map(str::to_string);
    Some(crate::sprite::behavior::AnimTagMap {
        idle,
        walk,
        run:     opt("run"),
        sit:     opt("sit"),
        sleep:   opt("sleep"),
        wake:    opt("wake"),
        grabbed: opt("grabbed"),
        petted:  opt("petted"),
        react:   opt("react"),
        fall:    opt("fall"),
        thrown:  opt("thrown"),
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd D:/elazar/private/my-pet && cargo test load_with_tag_map -- --test-threads=1
```

Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
cd D:/elazar/private/my-pet && git add src/sprite/sheet.rs && git commit -m "feat: add load_with_tag_map to read myPetTagMap from Aseprite JSON"
```

---

### Task 2: Create `src/sprite/editor_state.rs`

**Files:**
- Create: `src/sprite/editor_state.rs`
- Modify: `src/sprite/mod.rs`
- Create: `tests/integration/test_sprite_editor.rs`

Pure-Rust editor data model with no Win32 dependency. `SpriteEditorState` holds the working state (png path, image, grid dimensions, tags, behavior mapping). Key methods: `frame_rect`, `frames_for_tag`, `is_saveable`, `to_json`, `to_clean_json`, `save_to_dir`.

- [ ] **Step 1: Register the module**

In `src/sprite/mod.rs`, add:

```rust
pub mod editor_state;
```

- [ ] **Step 2: Write the failing integration test**

Create `tests/integration/test_sprite_editor.rs`:

```rust
//! Integration tests for SpriteEditorState — pure Rust, no Win32.

use my_pet::sprite::editor_state::{EditorTag, SpriteEditorState};
use my_pet::sprite::sheet::{load_with_tag_map, TagDirection};
use my_pet::sprite::behavior::AnimTagMap;
use tempfile::{tempdir, TempDir};

fn test_png_bytes() -> &'static [u8] {
    include_bytes!("../../assets/test_pet.png")
}

fn make_state() -> (SpriteEditorState, TempDir) {
    let tmp = tempdir().unwrap();
    let png_path = tmp.path().join("test_pet.png");
    std::fs::write(&png_path, test_png_bytes()).unwrap();
    let image = image::load_from_memory_with_format(test_png_bytes(), image::ImageFormat::Png)
        .unwrap()
        .into_rgba8();
    let state = SpriteEditorState::new(png_path, image);
    (state, tmp)
}

#[test]
fn frame_rect_uniform_grid() {
    let (mut state, _tmp) = make_state();
    // test_pet.png is 64×32 (2 frames wide, 1 row)
    state.rows = 1;
    state.cols = 2;
    assert_eq!(state.frame_rect(0), (0, 0, 32, 32));
    assert_eq!(state.frame_rect(1), (32, 0, 32, 32));
}

#[test]
fn to_json_produces_valid_aseprite() {
    let (mut state, _tmp) = make_state();
    state.rows = 1;
    state.cols = 2;
    state.tags.push(EditorTag {
        name: "idle".into(),
        from: 0,
        to: 1,
        direction: TagDirection::PingPong,
        color: 0,
    });
    state.tag_map.idle = "idle".into();
    state.tag_map.walk = "walk".into();

    let json = state.to_json();
    // Must parse via from_json_and_image without error
    let image = image::load_from_memory_with_format(test_png_bytes(), image::ImageFormat::Png)
        .unwrap()
        .into_rgba8();
    my_pet::sprite::sheet::SpriteSheet::from_json_and_image(&json, image)
        .expect("to_json must produce valid Aseprite JSON");
    // Must also round-trip through load_with_tag_map
    let (_, tag_map) = load_with_tag_map(&json, test_png_bytes()).unwrap();
    let tm = tag_map.expect("to_json must embed myPetTagMap");
    assert_eq!(tm.idle, "idle");
    assert_eq!(tm.walk, "walk");
}

#[test]
fn clean_json_strips_tag_map() {
    let (mut state, _tmp) = make_state();
    state.rows = 1;
    state.cols = 2;
    state.tag_map.idle = "idle".into();
    state.tag_map.walk = "walk".into();

    let json = state.to_clean_json();
    // Must parse cleanly
    let image = image::load_from_memory_with_format(test_png_bytes(), image::ImageFormat::Png)
        .unwrap()
        .into_rgba8();
    my_pet::sprite::sheet::SpriteSheet::from_json_and_image(&json, image)
        .expect("to_clean_json must produce valid Aseprite JSON");
    // Must NOT contain myPetTagMap
    let text = std::str::from_utf8(&json).unwrap();
    assert!(!text.contains("myPetTagMap"), "clean export must not contain myPetTagMap");
}

#[test]
fn direction_round_trip() {
    use my_pet::sprite::sheet::TagDirection;
    let cases = [
        (TagDirection::Forward,         "forward"),
        (TagDirection::Reverse,         "reverse"),
        (TagDirection::PingPong,        "pingpong"),
        (TagDirection::PingPongReverse, "pingpong_reverse"),
    ];
    for (dir, expected_str) in cases {
        let (mut state, _tmp) = make_state();
        state.rows = 1;
        state.cols = 2;
        state.tags.push(EditorTag { name: "t".into(), from: 0, to: 1, direction: dir, color: 0 });
        state.tag_map.idle = "idle".into();
        state.tag_map.walk = "walk".into();
        let json = state.to_json();
        let text = std::str::from_utf8(&json).unwrap();
        assert!(text.contains(expected_str),
            "direction {:?} must serialize to \"{}\"", dir, expected_str);
        // Must round-trip: parse back, tag direction must match
        let image = image::load_from_memory_with_format(test_png_bytes(), image::ImageFormat::Png)
            .unwrap()
            .into_rgba8();
        let sheet = my_pet::sprite::sheet::SpriteSheet::from_json_and_image(&json, image).unwrap();
        assert_eq!(sheet.tags[0].direction, state.tags[0].direction);
    }
}

#[test]
fn is_saveable_requires_idle_and_walk() {
    let (mut state, _tmp) = make_state();
    assert!(!state.is_saveable(), "empty idle+walk → not saveable");
    state.tag_map.idle = "idle".into();
    assert!(!state.is_saveable(), "missing walk → not saveable");
    state.tag_map.walk = "walk".into();
    assert!(state.is_saveable(), "both set → saveable");
    state.tag_map.idle = String::new();
    assert!(!state.is_saveable(), "empty idle → not saveable");
}

#[test]
fn tag_color_assignment() {
    // 10 tags → all get distinct colors without panic
    let colors: Vec<u32> = (0..10).map(SpriteEditorState::assign_color).collect();
    // At least within the 8-color palette they cycle — just verify no panic and
    // that adjacent indices that are < 8 apart get different values
    for i in 0..8 {
        assert_ne!(colors[i], colors[(i + 1) % 8], "adjacent tag colors should differ");
    }
}

#[test]
fn save_to_dir_writes_json_and_png() {
    let (mut state, _state_dir) = make_state();
    let tmp = tempdir().unwrap();
    state.rows = 1;
    state.cols = 2;
    state.tags.push(EditorTag {
        name: "idle".into(), from: 0, to: 1,
        direction: TagDirection::PingPong, color: 0,
    });
    state.tags.push(EditorTag {
        name: "walk".into(), from: 0, to: 1,
        direction: TagDirection::Forward, color: 1,
    });
    state.tag_map.idle = "idle".into();
    state.tag_map.walk = "walk".into();

    state.save_to_dir(tmp.path()).expect("save_to_dir must succeed");

    let json_path = tmp.path().join("test_pet.json");
    let png_path = tmp.path().join("test_pet.png");
    assert!(json_path.exists(), "JSON must be written");
    assert!(png_path.exists(), "PNG must be copied");

    // Reload via load_with_tag_map → parses cleanly and tag map round-trips
    let json_bytes = std::fs::read(&json_path).unwrap();
    let png_bytes = std::fs::read(&png_path).unwrap();
    let (_, tag_map) = load_with_tag_map(&json_bytes, &png_bytes).unwrap();
    let tm = tag_map.expect("saved JSON must contain myPetTagMap");
    assert_eq!(tm.idle, "idle");
    assert_eq!(tm.walk, "walk");
}
```

- [ ] **Step 2b: Register the test in `tests/integration.rs`**

In `tests/integration.rs`, add at the end:

```rust
mod sprite_editor {
    include!("integration/test_sprite_editor.rs");
}
```

- [ ] **Step 3: Run integration tests to verify they fail**

```bash
cd D:/elazar/private/my-pet && cargo test --test integration 2>&1 | head -20
```

Expected: FAIL — compile error `error[E0432]: unresolved import my_pet::sprite::editor_state`

- [ ] **Step 4: Create `src/sprite/editor_state.rs`**

```rust
//! Pure-Rust sprite editor state. No Win32 dependency.

use anyhow::{anyhow, Context, Result};
use image::RgbaImage;
use std::path::{Path, PathBuf};

use crate::sprite::behavior::AnimTagMap;
use crate::sprite::sheet::TagDirection;

// ─── Tag color palette (Win32 COLORREF: 0x00BBGGRR) ──────────────────────────

const TAG_COLORS: &[u32] = &[
    0x0000ffff, // yellow
    0x00ffff00, // cyan
    0x00ff00ff, // magenta
    0x000080ff, // orange
    0x0000ff00, // lime
    0x000000ff, // red
    0x00ff0000, // blue
    0x008080ff, // pink
];

// ─── Public types ─────────────────────────────────────────────────────────────

pub struct EditorTag {
    pub name: String,
    pub from: usize,
    pub to: usize,
    pub direction: TagDirection,
    /// Win32 COLORREF (0x00BBGGRR) assigned from TAG_COLORS.
    pub color: u32,
}

pub struct SpriteEditorState {
    pub png_path: PathBuf,
    pub image: RgbaImage,
    pub rows: u32,
    pub cols: u32,
    pub tags: Vec<EditorTag>,
    pub tag_map: AnimTagMap,
    pub selected_tag: Option<usize>,
}

// ─── impl SpriteEditorState ───────────────────────────────────────────────────

impl SpriteEditorState {
    pub fn new(png_path: PathBuf, image: RgbaImage) -> Self {
        SpriteEditorState {
            png_path,
            image,
            rows: 1,
            cols: 1,
            tags: Vec::new(),
            tag_map: AnimTagMap {
                idle: String::new(),
                walk: String::new(),
                run: None, sit: None, sleep: None, wake: None,
                grabbed: None, petted: None, react: None, fall: None, thrown: None,
            },
            selected_tag: None,
        }
    }

    /// Returns `(x, y, w, h)` for frame `i` in a uniform grid.
    pub fn frame_rect(&self, i: usize) -> (u32, u32, u32, u32) {
        let w = self.image.width() / self.cols;
        let h = self.image.height() / self.rows;
        let col = (i as u32) % self.cols;
        let row = (i as u32) / self.cols;
        (col * w, row * h, w, h)
    }

    /// Frame indices (inclusive range) covered by tag `tag_idx`.
    pub fn frames_for_tag(&self, tag_idx: usize) -> Vec<usize> {
        match self.tags.get(tag_idx) {
            Some(t) => (t.from..=t.to).collect(),
            None => vec![],
        }
    }

    /// True iff both `idle` and `walk` behavior states are mapped (non-empty).
    pub fn is_saveable(&self) -> bool {
        !self.tag_map.idle.is_empty() && !self.tag_map.walk.is_empty()
    }

    /// COLORREF for tag at `idx` (cycles through TAG_COLORS palette).
    pub fn assign_color(idx: usize) -> u32 {
        TAG_COLORS[idx % TAG_COLORS.len()]
    }

    /// Serialise to Aseprite array-format JSON including `myPetTagMap`.
    pub fn to_json(&self) -> Vec<u8> {
        let json = self.build_json(true);
        serde_json::to_vec_pretty(&json).expect("serialize JSON")
    }

    /// Serialise to Aseprite array-format JSON WITHOUT `myPetTagMap` (for export).
    pub fn to_clean_json(&self) -> Vec<u8> {
        let json = self.build_json(false);
        serde_json::to_vec_pretty(&json).expect("serialize clean JSON")
    }

    /// Write JSON + copy PNG to `dir`, overwriting any existing files.
    pub fn save_to_dir(&self, dir: &Path) -> Result<()> {
        let stem = self.png_path
            .file_stem()
            .ok_or_else(|| anyhow!("png_path has no stem"))?
            .to_string_lossy();
        let dest_json = dir.join(format!("{stem}.json"));
        let dest_png = dir.join(format!("{stem}.png"));
        std::fs::write(&dest_json, self.to_json())
            .with_context(|| format!("write {}", dest_json.display()))?;
        std::fs::copy(&self.png_path, &dest_png)
            .with_context(|| format!("copy PNG to {}", dest_png.display()))?;
        Ok(())
    }

    // ─── Private helpers ───────────────────────────────────────────────────

    fn build_json(&self, include_tag_map: bool) -> serde_json::Value {
        let total = (self.rows * self.cols) as usize;
        let frames: Vec<serde_json::Value> = (0..total)
            .map(|i| {
                let (x, y, w, h) = self.frame_rect(i);
                serde_json::json!({"frame": {"x": x, "y": y, "w": w, "h": h}, "duration": 100})
            })
            .collect();

        let frame_tags: Vec<serde_json::Value> = self.tags
            .iter()
            .map(|t| serde_json::json!({
                "name": t.name,
                "from": t.from,
                "to": t.to,
                "direction": direction_to_str(&t.direction),
            }))
            .collect();

        if include_tag_map {
            let mut map = serde_json::Map::new();
            for (key, val) in tag_map_populated_entries(&self.tag_map) {
                map.insert(key, val.into());
            }
            serde_json::json!({
                "frames": frames,
                "meta": {"frameTags": frame_tags, "myPetTagMap": map},
            })
        } else {
            serde_json::json!({
                "frames": frames,
                "meta": {"frameTags": frame_tags},
            })
        }
    }
}

// ─── Free helpers ─────────────────────────────────────────────────────────────

fn direction_to_str(d: &TagDirection) -> &'static str {
    match d {
        TagDirection::Forward         => "forward",
        TagDirection::Reverse         => "reverse",
        TagDirection::PingPong        => "pingpong",
        TagDirection::PingPongReverse => "pingpong_reverse",
    }
}

/// Returns only populated (non-empty) entries from the tag map.
fn tag_map_populated_entries(tm: &AnimTagMap) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if !tm.idle.is_empty() { out.push(("idle".into(), tm.idle.clone())); }
    if !tm.walk.is_empty() { out.push(("walk".into(), tm.walk.clone())); }
    if let Some(v) = &tm.run     { out.push(("run".into(),     v.clone())); }
    if let Some(v) = &tm.sit     { out.push(("sit".into(),     v.clone())); }
    if let Some(v) = &tm.sleep   { out.push(("sleep".into(),   v.clone())); }
    if let Some(v) = &tm.wake    { out.push(("wake".into(),    v.clone())); }
    if let Some(v) = &tm.grabbed { out.push(("grabbed".into(), v.clone())); }
    if let Some(v) = &tm.petted  { out.push(("petted".into(),  v.clone())); }
    if let Some(v) = &tm.react   { out.push(("react".into(),   v.clone())); }
    if let Some(v) = &tm.fall    { out.push(("fall".into(),    v.clone())); }
    if let Some(v) = &tm.thrown  { out.push(("thrown".into(),  v.clone())); }
    out
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cd D:/elazar/private/my-pet && cargo test --test integration sprite_editor -- --test-threads=1
```

Expected: all 7 integration tests pass.

Also verify all existing tests still pass:

```bash
cd D:/elazar/private/my-pet && cargo test -- --test-threads=1
```

Expected: all tests pass (35 existing + 7 new = 42 total).

- [ ] **Step 6: Commit**

```bash
cd D:/elazar/private/my-pet && git add src/sprite/mod.rs src/sprite/editor_state.rs tests/integration/test_sprite_editor.rs tests/integration.rs && git commit -m "feat: add SpriteEditorState with uniform grid, tag model, and JSON serialisation"
```

---

## Chunk 2: Win32 editor window

### Task 3: Create `src/tray/sprite_editor.rs`

**Files:**
- Create: `src/tray/sprite_editor.rs`
- Modify: `src/tray/mod.rs`

The editor window follows exactly the same Win32 ownership pattern as `config_window.rs`: `Box::into_raw` in `show_sprite_editor`, `Box::from_raw` in `WM_DESTROY`. A `static AtomicPtr` guards against opening a second window. No unit tests for Win32 GUI code; the step-by-step compile checks and smoke test are the verification.

**Layout (client area ~760 × 500):**
- Canvas area: left=10, top=10, width=370, height=380 — paint spritesheet + grid + highlights
- Below canvas: "Rows:" label + rows edit (30px wide), "Cols:" label + cols edit, at y=400
- Right panel starts at x=394, full height
- Right panel controls (y positions from top): labels/listbox/buttons/preview/save

**Control IDs:**
```
ID_EDIT_ROWS=201, ID_EDIT_COLS=202
ID_LIST_TAGS=203
ID_BTN_ADD_TAG=204, ID_BTN_REMOVE_TAG=205
ID_EDIT_TAG_NAME=210, ID_EDIT_TAG_FROM=211, ID_EDIT_TAG_TO=212
ID_COMBO_DIR=213, ID_BTN_TAG_OK=214
ID_COMBO_BEHAVIOR=206
ID_BTN_SAVE=207, ID_BTN_EXPORT=208
ID_STATIC_STATUS=209
TIMER_PREVIEW=2001
```

- [ ] **Step 1: Register the module and add skeleton**

In `src/tray/mod.rs`, add:
```rust
pub mod sprite_editor;
```

Create `src/tray/sprite_editor.rs` with the window class registration and `show_sprite_editor` skeleton:

```rust
#![allow(unsafe_op_in_unsafe_fn)]

use crate::sprite::editor_state::{EditorTag, SpriteEditorState};
use crate::sprite::sheet::{load_embedded, TagDirection};
use crate::sprite::animation::AnimationState;
use image::RgbaImage;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::ffi::c_void;

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, CreatePen,
        CreateSolidBrush, DeleteDC, DeleteObject, EndPaint, FillRect,
        GetModuleHandleW, MoveToEx, LineTo, Rectangle, SelectObject,
        SetBkMode, SetTextColor, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, HBRUSH, HPEN, PAINTSTRUCT, PS_SOLID, SRCCOPY,
        TRANSPARENT, DrawTextW, DT_LEFT, DT_SINGLELINE, DT_VCENTER,
    },
    System::LibraryLoader::GetModuleHandleW as SysGetModuleHandleW,
    UI::WindowsAndMessaging::*,
};

// ─── Single-instance guard ────────────────────────────────────────────────────

static EDITOR_HWND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

// ─── Control IDs ─────────────────────────────────────────────────────────────

const ID_EDIT_ROWS:      i32 = 201;
const ID_EDIT_COLS:      i32 = 202;
const ID_LIST_TAGS:      i32 = 203;
const ID_BTN_ADD_TAG:    i32 = 204;
const ID_BTN_REMOVE_TAG: i32 = 205;
const ID_COMBO_BEHAVIOR: i32 = 206;
const ID_BTN_SAVE:       i32 = 207;
const ID_BTN_EXPORT:     i32 = 208;
const ID_STATIC_STATUS:  i32 = 209;
const ID_EDIT_TAG_NAME:  i32 = 210;
const ID_EDIT_TAG_FROM:  i32 = 211;
const ID_EDIT_TAG_TO:    i32 = 212;
const ID_COMBO_DIR:      i32 = 213;
const ID_BTN_TAG_OK:     i32 = 214;
const TIMER_PREVIEW:     usize = 2001;

// ─── Colors (same dark theme as config_window.rs) ────────────────────────────

const fn clr_bg()      -> u32 { 0x1e | (0x1e << 8) | (0x1e << 16) }
const fn clr_bg_ctrl() -> u32 { 0x3c | (0x3c << 8) | (0x3c << 16) }
const fn clr_text()    -> u32 { 0xcc | (0xcc << 8) | (0xcc << 16) }
const fn clr_label()   -> u32 { 0x85 | (0x85 << 8) | (0x85 << 16) }

// ─── Window class name ────────────────────────────────────────────────────────

const EDITOR_CLASS: &str = "MyPetSpriteEditor";

// ─── Context ──────────────────────────────────────────────────────────────────

struct SpriteEditorCtx {
    state: SpriteEditorState,
    /// BGRA cache of state.image for StretchDIBits (converted once on open).
    bgra_cache: Vec<u8>,
    /// Current preview frame index within the selected tag.
    preview_frame: usize,
    preview_elapsed_ms: u32,
    /// Whether the "add tag" inline form is visible.
    add_form_visible: bool,
    dark_bg_brush: HBRUSH,
    ctrl_brush: HBRUSH,
}

impl SpriteEditorCtx {
    unsafe fn new(state: SpriteEditorState) -> Box<Self> {
        let bgra_cache = rgba_to_bgra(&state.image);
        Box::new(SpriteEditorCtx {
            state,
            bgra_cache,
            preview_frame: 0,
            preview_elapsed_ms: 0,
            add_form_visible: false,
            dark_bg_brush: CreateSolidBrush(clr_bg()),
            ctrl_brush: CreateSolidBrush(clr_bg_ctrl()),
        })
    }

    unsafe fn destroy_brushes(&self) {
        DeleteObject(self.dark_bg_brush as *mut _);
        DeleteObject(self.ctrl_brush as *mut _);
    }
}

fn rgba_to_bgra(image: &RgbaImage) -> Vec<u8> {
    image.pixels()
        .flat_map(|p| [p[2], p[1], p[0], p[3]])
        .collect()
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

unsafe fn get_ctx(hwnd: HWND) -> *mut SpriteEditorCtx {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut SpriteEditorCtx
}

unsafe fn center_window(hwnd: HWND) {
    let mut rc: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut rc);
    let w = rc.right - rc.left;
    let h = rc.bottom - rc.top;
    let sw = GetSystemMetrics(SM_CXSCREEN);
    let sh = GetSystemMetrics(SM_CYSCREEN);
    SetWindowPos(hwnd, std::ptr::null_mut(),
        (sw - w) / 2, (sh - h) / 2, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
}

// ─── Window class registration ────────────────────────────────────────────────

fn register_editor_class() {
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(|| unsafe {
        let hi = SysGetModuleHandleW(std::ptr::null());
        let cls_name = wide(EDITOR_CLASS);
        let mut wc: WNDCLASSEXW = std::mem::zeroed();
        wc.cbSize        = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc   = Some(editor_wnd_proc);
        wc.hInstance     = hi;
        wc.hCursor       = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
        wc.lpszClassName = cls_name.as_ptr();
        RegisterClassExW(&wc);
    });
}

// ─── Public entry point ───────────────────────────────────────────────────────

/// Open the sprite editor for `state`. Only one editor window is allowed at a
/// time — if one is already open it is brought to the foreground.
pub fn show_sprite_editor(parent: HWND, state: SpriteEditorState) {
    let stored = EDITOR_HWND.load(Ordering::Relaxed);
    if !stored.is_null() {
        unsafe {
            if IsWindow(stored as HWND) != 0 {
                SetForegroundWindow(stored as HWND);
                return;
            }
        }
    }

    register_editor_class();

    unsafe {
        let ctx = SpriteEditorCtx::new(state);
        let ctx_ptr = Box::into_raw(ctx);

        let cls   = wide(EDITOR_CLASS);
        let title = wide("My Pet \u{2014} Sprite Editor");
        let style = WS_CAPTION | WS_SYSMENU | WS_CLIPSIBLINGS | WS_CLIPCHILDREN | WS_VISIBLE;

        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            cls.as_ptr(),
            title.as_ptr(),
            style,
            CW_USEDEFAULT, CW_USEDEFAULT,
            780, 540,
            parent,
            std::ptr::null_mut(),
            SysGetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        if hwnd.is_null() {
            drop(Box::from_raw(ctx_ptr));
            return;
        }
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ctx_ptr as isize);
        create_editor_controls(hwnd, &mut *ctx_ptr);
        refresh_tags_list(hwnd, &(*ctx_ptr).state);
        update_save_button(hwnd, &(*ctx_ptr).state);
        center_window(hwnd);
        EDITOR_HWND.store(hwnd as *mut c_void, Ordering::Relaxed);
        SetTimer(hwnd, TIMER_PREVIEW, 100, None);
    }
}
```

- [ ] **Step 2: Verify it compiles (partial)**

```bash
cd D:/elazar/private/my-pet && cargo build 2>&1 | grep "^error"
```

Expected: compile errors about missing functions (`create_editor_controls`, `refresh_tags_list`, etc.) — those come next.

- [ ] **Step 3: Add `create_editor_controls`**

Append to `src/tray/sprite_editor.rs`:

```rust
// ─── Controls ─────────────────────────────────────────────────────────────────

unsafe fn create_editor_controls(hwnd: HWND, ctx: &mut SpriteEditorCtx) {
    let hi = SysGetModuleHandleW(std::ptr::null());
    let tab = WS_TABSTOP;

    macro_rules! label {
        ($text:expr, $x:expr, $y:expr, $w:expr, $h:expr) => {
            CreateWindowExW(0, wide("STATIC").as_ptr(), wide($text).as_ptr(),
                WS_CHILD | WS_VISIBLE, $x, $y, $w, $h,
                hwnd, std::ptr::null_mut(), hi, std::ptr::null())
        };
    }
    macro_rules! edit {
        ($id:expr, $text:expr, $x:expr, $y:expr, $w:expr) => {
            CreateWindowExW(WS_EX_CLIENTEDGE,
                wide("EDIT").as_ptr(), wide($text).as_ptr(),
                WS_CHILD | WS_VISIBLE | tab | ES_AUTOHSCROLL as u32,
                $x, $y, $w, 22, hwnd, $id as usize as HMENU, hi, std::ptr::null())
        };
    }
    macro_rules! btn {
        ($text:expr, $id:expr, $x:expr, $y:expr, $w:expr, $h:expr) => {
            CreateWindowExW(0, wide("BUTTON").as_ptr(), wide($text).as_ptr(),
                WS_CHILD | WS_VISIBLE | tab | BS_PUSHBUTTON as u32,
                $x, $y, $w, $h, hwnd, $id as usize as HMENU, hi, std::ptr::null())
        };
    }
    macro_rules! combo {
        ($id:expr, $x:expr, $y:expr, $w:expr, $h:expr) => {
            CreateWindowExW(0, wide("COMBOBOX").as_ptr(), wide("").as_ptr(),
                WS_CHILD | WS_VISIBLE | tab | CBS_DROPDOWNLIST as u32,
                $x, $y, $w, $h, hwnd, $id as usize as HMENU, hi, std::ptr::null())
        };
    }

    // ── Below-canvas: Rows / Cols ────────────────────────────────────────────
    label!("Rows:", 10, 400, 40, 20);
    let rows_str = ctx.state.rows.to_string();
    edit!(ID_EDIT_ROWS, &rows_str, 54, 398, 36, 22);
    label!("Cols:", 100, 400, 40, 20);
    let cols_str = ctx.state.cols.to_string();
    edit!(ID_EDIT_COLS, &cols_str, 144, 398, 36, 22);

    // ── Right panel ──────────────────────────────────────────────────────────
    let rx = 394;

    label!("TAGS", rx, 10, 200, 14);
    // Tags listbox
    CreateWindowExW(0,
        wide("LISTBOX").as_ptr(), wide("").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | tab | LBS_NOTIFY as u32,
        rx, 28, 340, 120,
        hwnd, ID_LIST_TAGS as usize as HMENU, hi, std::ptr::null());

    btn!("+ Add Tag",    ID_BTN_ADD_TAG,    rx,       154, 100, 24);
    btn!("Remove Tag",   ID_BTN_REMOVE_TAG, rx + 110, 154, 100, 24);

    // Inline "add tag" form (initially hidden)
    edit!(ID_EDIT_TAG_NAME, "tag_name", rx,       184, 120, 22);
    label!("From:",        rx + 130, 186, 36, 18);
    edit!(ID_EDIT_TAG_FROM, "0",       rx + 168,  184,  36, 22);
    label!("To:",          rx + 214,  186, 24, 18);
    edit!(ID_EDIT_TAG_TO,  "0",        rx + 240,  184,  36, 22);
    // Direction combo
    let dir_combo = combo!(ID_COMBO_DIR, rx + 285, 183, 110, 120);
    for dir in &["forward", "reverse", "pingpong", "pingpong_reverse"] {
        let w = wide(dir);
        SendMessageW(dir_combo, CB_ADDSTRING, 0, w.as_ptr() as LPARAM);
    }
    SendMessageW(dir_combo, CB_SETCURSEL, 0, 0);
    btn!("OK", ID_BTN_TAG_OK, rx + 400, 183, 50, 24);

    // Hide add-form controls initially
    for id in &[ID_EDIT_TAG_NAME, ID_EDIT_TAG_FROM, ID_EDIT_TAG_TO,
                ID_COMBO_DIR, ID_BTN_TAG_OK] {
        ShowWindow(GetDlgItem(hwnd, *id), SW_HIDE);
    }

    // ── Behavior mapping ─────────────────────────────────────────────────────
    label!("Behavior for selected tag:", rx, 218, 200, 14);
    let beh_combo = combo!(ID_COMBO_BEHAVIOR, rx, 234, 200, 200);
    SendMessageW(beh_combo, CB_ADDSTRING, 0, wide("— not set —").as_ptr() as LPARAM);
    for state_name in &["idle","walk","run","sit","sleep","wake",
                        "grabbed","petted","react","fall","thrown"] {
        let w = wide(state_name);
        SendMessageW(beh_combo, CB_ADDSTRING, 0, w.as_ptr() as LPARAM);
    }
    SendMessageW(beh_combo, CB_SETCURSEL, 0, 0);

    // ── Preview area label ───────────────────────────────────────────────────
    label!("PREVIEW", rx, 270, 200, 14);
    // (preview drawn directly in WM_PAINT at rx, 288, 150, 150)

    // ── Save / Export ────────────────────────────────────────────────────────
    btn!("Save",     ID_BTN_SAVE,   rx,       440, 80, 28);
    btn!("Export…",  ID_BTN_EXPORT, rx + 90,  440, 80, 28);

    // Status label
    CreateWindowExW(0, wide("STATIC").as_ptr(),
        wide("Assign idle and walk to enable Save").as_ptr(),
        WS_CHILD | WS_VISIBLE,
        rx, 472, 340, 18, hwnd,
        ID_STATIC_STATUS as usize as HMENU, hi, std::ptr::null());
}

/// Populate the tags listbox from `state.tags`.
unsafe fn refresh_tags_list(hwnd: HWND, state: &SpriteEditorState) {
    let lb = GetDlgItem(hwnd, ID_LIST_TAGS);
    SendMessageW(lb, LB_RESETCONTENT, 0, 0);
    for (i, tag) in state.tags.iter().enumerate() {
        let behavior = behavior_for_tag(state, &tag.name);
        let entry = format!("{} [{}-{}] → {}", tag.name, tag.from, tag.to,
            behavior.unwrap_or("— not set —"));
        let w = wide(&entry);
        SendMessageW(lb, LB_ADDSTRING, 0, w.as_ptr() as LPARAM);
    }
    if let Some(sel) = state.selected_tag {
        SendMessageW(lb, LB_SETCURSEL, sel, 0);
    }
}

/// Enable/disable Save button based on `is_saveable`.
unsafe fn update_save_button(hwnd: HWND, state: &SpriteEditorState) {
    let saveable = state.is_saveable();
    EnableWindow(GetDlgItem(hwnd, ID_BTN_SAVE), if saveable { 1 } else { 0 });
    let status = GetDlgItem(hwnd, ID_STATIC_STATUS);
    if saveable {
        let w = wide("Ready to save");
        SetWindowTextW(status, w.as_ptr());
    } else {
        let w = wide("Assign idle and walk to enable Save");
        SetWindowTextW(status, w.as_ptr());
    }
}

/// Find which behavior state maps to `tag_name` (reverse lookup of tag_map).
fn behavior_for_tag<'a>(state: &'a SpriteEditorState, tag_name: &str) -> Option<&'a str> {
    let tm = &state.tag_map;
    if tm.idle == tag_name { return Some("idle"); }
    if tm.walk == tag_name { return Some("walk"); }
    if tm.run.as_deref()     == Some(tag_name) { return Some("run"); }
    if tm.sit.as_deref()     == Some(tag_name) { return Some("sit"); }
    if tm.sleep.as_deref()   == Some(tag_name) { return Some("sleep"); }
    if tm.wake.as_deref()    == Some(tag_name) { return Some("wake"); }
    if tm.grabbed.as_deref() == Some(tag_name) { return Some("grabbed"); }
    if tm.petted.as_deref()  == Some(tag_name) { return Some("petted"); }
    if tm.react.as_deref()   == Some(tag_name) { return Some("react"); }
    if tm.fall.as_deref()    == Some(tag_name) { return Some("fall"); }
    if tm.thrown.as_deref()  == Some(tag_name) { return Some("thrown"); }
    None
}

// Need EnableWindow — import from windows-sys
// (already available via Win32_UI_WindowsAndMessaging wildcard)
```

- [ ] **Step 4: Add canvas painting and WM_PAINT**

Append to `src/tray/sprite_editor.rs`:

```rust
// ─── Canvas painting ──────────────────────────────────────────────────────────

/// Paint the spritesheet with grid lines and tag highlights into the canvas
/// rectangle (left=10, top=10, width=370, height=380) of the window DC.
unsafe fn paint_canvas(hdc: windows_sys::Win32::Graphics::Gdi::HDC,
                       ctx: &SpriteEditorCtx) {
    let cx = 10i32;
    let cy = 10i32;
    let cw = 370i32;
    let ch = 380i32;

    // Background
    let mut canvas_rc = RECT { left: cx, top: cy, right: cx + cw, bottom: cy + ch };
    FillRect(hdc, &canvas_rc, ctx.dark_bg_brush);

    let img_w = ctx.state.image.width() as i32;
    let img_h = ctx.state.image.height() as i32;
    if img_w == 0 || img_h == 0 { return; }

    // Scale to fit, preserving aspect ratio
    let scale_x_num = cw;
    let scale_x_den = img_w;
    let scale_y_num = ch;
    let scale_y_den = img_h;
    // Use integer arithmetic: compare scale_x vs scale_y
    let (sw, sh) = if scale_x_num * img_h <= scale_y_num * img_w {
        // scale_x is smaller
        let sw = cw;
        let sh = img_h * cw / img_w;
        (sw, sh)
    } else {
        let sh = ch;
        let sw = img_w * ch / img_h;
        (sw, sh)
    };

    // Center in canvas
    let ox = cx + (cw - sw) / 2;
    let oy = cy + (ch - sh) / 2;

    // Draw spritesheet via StretchDIBits
    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader.biSize        = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth       = img_w;
    bmi.bmiHeader.biHeight      = -img_h; // top-down
    bmi.bmiHeader.biPlanes      = 1;
    bmi.bmiHeader.biBitCount    = 32;
    bmi.bmiHeader.biCompression = BI_RGB as u32;

    StretchDIBits(
        hdc,
        ox, oy, sw, sh,
        0, 0, img_w, img_h,
        ctx.bgra_cache.as_ptr() as *const _,
        &bmi, DIB_RGB_COLORS, SRCCOPY,
    );

    // Cell dimensions in screen coords
    let cell_w = sw / ctx.state.cols as i32;
    let cell_h = sh / ctx.state.rows as i32;
    let total  = (ctx.state.rows * ctx.state.cols) as usize;

    // Draw tag frame highlights (colored rectangle outlines)
    for (tag_idx, tag) in ctx.state.state_tags_iter() {
        let is_selected = ctx.state.selected_tag == Some(tag_idx);
        let pen_width = if is_selected { 3 } else { 1 };
        let pen = CreatePen(PS_SOLID as i32, pen_width, tag.color);
        let old_pen = SelectObject(hdc, pen as *mut _);
        let old_brush = SelectObject(hdc,
            windows_sys::Win32::Graphics::Gdi::GetStockObject(
                windows_sys::Win32::Graphics::Gdi::NULL_BRUSH as i32) as *mut _);

        for frame_idx in tag.from..=tag.to {
            let col = (frame_idx % ctx.state.cols as usize) as i32;
            let row = (frame_idx / ctx.state.cols as usize) as i32;
            let fx = ox + col * cell_w;
            let fy = oy + row * cell_h;
            Rectangle(hdc, fx, fy, fx + cell_w, fy + cell_h);
        }

        SelectObject(hdc, old_pen as *mut _);
        SelectObject(hdc, old_brush as *mut _);
        DeleteObject(pen as *mut _);
    }

    // Draw gray grid lines
    let gray_pen = CreatePen(PS_SOLID as i32, 1, 0x00555555);
    let old = SelectObject(hdc, gray_pen as *mut _);
    for col in 1..ctx.state.cols as i32 {
        let x = ox + col * cell_w;
        MoveToEx(hdc, x, oy, std::ptr::null_mut());
        LineTo(hdc, x, oy + sh);
    }
    for row in 1..ctx.state.rows as i32 {
        let y = oy + row * cell_h;
        MoveToEx(hdc, ox, y, std::ptr::null_mut());
        LineTo(hdc, ox + sw, y);
    }
    SelectObject(hdc, old as *mut _);
    DeleteObject(gray_pen as *mut _);
}

/// Paint the preview of the selected tag's current frame at (rx, 288, 150, 150).
unsafe fn paint_preview(hdc: windows_sys::Win32::Graphics::Gdi::HDC,
                        ctx: &SpriteEditorCtx) {
    let px = 394i32;
    let py = 288i32;
    let pw = 150i32;
    let ph = 150i32;

    let mut rc = RECT { left: px, top: py, right: px + pw, bottom: py + ph };
    FillRect(hdc, &rc, ctx.dark_bg_brush);

    let sel = match ctx.state.selected_tag {
        Some(idx) => idx,
        None => return,
    };
    let tag = match ctx.state.tags.get(sel) {
        Some(t) => t,
        None => return,
    };

    let total_tag_frames = tag.to.saturating_sub(tag.from) + 1;
    if total_tag_frames == 0 { return; }
    let frame_in_tag = ctx.preview_frame % total_tag_frames;
    let frame_idx = tag.from + frame_in_tag;

    let (fx, fy, fw, fh) = ctx.state.frame_rect(frame_idx);
    if fw == 0 || fh == 0 { return; }

    let img_w = ctx.state.image.width() as i32;
    let img_h = ctx.state.image.height() as i32;

    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader.biSize        = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth       = img_w;
    bmi.bmiHeader.biHeight      = -img_h;
    bmi.bmiHeader.biPlanes      = 1;
    bmi.bmiHeader.biBitCount    = 32;
    bmi.bmiHeader.biCompression = BI_RGB as u32;

    StretchDIBits(
        hdc,
        px, py, pw, ph,
        fx as i32, fy as i32, fw as i32, fh as i32,
        ctx.bgra_cache.as_ptr() as *const _,
        &bmi, DIB_RGB_COLORS, SRCCOPY,
    );
}
```

Note: `state_tags_iter` doesn't exist yet — add it to `SpriteEditorState` in `editor_state.rs`:

```rust
/// Iterator of `(tag_idx, &EditorTag)` — used by the canvas painter.
pub fn state_tags_iter(&self) -> impl Iterator<Item = (usize, &EditorTag)> {
    self.tags.iter().enumerate()
}
```

- [ ] **Step 5: Add the window procedure**

Append to `src/tray/sprite_editor.rs`:

```rust
// ─── Window procedure ─────────────────────────────────────────────────────────

unsafe extern "system" fn editor_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let ctx = get_ctx(hwnd);
            if !ctx.is_null() {
                // Fill entire background
                let mut rc: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut rc);
                FillRect(hdc, &rc, (*ctx).dark_bg_brush);
                paint_canvas(hdc, &*ctx);
                paint_preview(hdc, &*ctx);
            }
            EndPaint(hwnd, &ps);
            0
        }

        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORBTN => {
            let hdc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
            SetBkMode(hdc, TRANSPARENT as i32);
            SetTextColor(hdc, clr_text());
            let ctx = get_ctx(hwnd);
            if ctx.is_null() { return DefWindowProcW(hwnd, msg, wparam, lparam); }
            (*ctx).dark_bg_brush as LRESULT
        }

        WM_TIMER => {
            if wparam == TIMER_PREVIEW {
                let ctx = get_ctx(hwnd);
                if !ctx.is_null() && (*ctx).state.selected_tag.is_some() {
                    (*ctx).preview_elapsed_ms += 100;
                    if (*ctx).preview_elapsed_ms >= 100 {
                        (*ctx).preview_elapsed_ms = 0;
                        (*ctx).preview_frame = (*ctx).preview_frame.wrapping_add(1);
                    }
                    // Repaint only the preview area
                    let rc = RECT { left: 394, top: 288, right: 544, bottom: 438 };
                    InvalidateRect(hwnd, &rc, 0);
                }
            }
            0
        }

        WM_COMMAND => {
            let id     = (wparam & 0xffff) as i32;
            let notify = ((wparam >> 16) & 0xffff) as u16;
            let ctx = get_ctx(hwnd);
            if !ctx.is_null() {
                handle_editor_command(hwnd, id, notify, &mut *ctx);
            }
            0
        }

        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }

        WM_DESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut SpriteEditorCtx;
            if !ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                KillTimer(hwnd, TIMER_PREVIEW);
                let ctx = Box::from_raw(ptr);
                ctx.destroy_brushes();
                // ctx dropped here
            }
            EDITOR_HWND.store(std::ptr::null_mut(), Ordering::Relaxed);
            0
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
```

- [ ] **Step 6: Add the command handler**

Append to `src/tray/sprite_editor.rs`:

```rust
// ─── Command handler ──────────────────────────────────────────────────────────

unsafe fn handle_editor_command(
    hwnd: HWND,
    id: i32,
    notify: u16,
    ctx: &mut SpriteEditorCtx,
) {
    match id {
        ID_EDIT_ROWS | ID_EDIT_COLS => {
            if notify == EN_KILLFOCUS as u16 {
                let rows = read_u32_field(hwnd, ID_EDIT_ROWS, 1, 64).unwrap_or(1);
                let cols = read_u32_field(hwnd, ID_EDIT_COLS, 1, 64).unwrap_or(1);
                ctx.state.rows = rows;
                ctx.state.cols = cols;
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
        }

        ID_LIST_TAGS => {
            if notify == LBN_SELCHANGE as u16 {
                let lb = GetDlgItem(hwnd, ID_LIST_TAGS);
                let sel = SendMessageW(lb, LB_GETCURSEL, 0, 0) as isize;
                if sel >= 0 {
                    ctx.state.selected_tag = Some(sel as usize);
                    ctx.preview_frame = 0;
                    // Update behavior combo to show current mapping
                    if let Some(tag) = ctx.state.tags.get(sel as usize) {
                        let beh = behavior_for_tag(&ctx.state, &tag.name);
                        let combo = GetDlgItem(hwnd, ID_COMBO_BEHAVIOR);
                        let idx = match beh {
                            None         => 0,
                            Some("idle") => 1,  Some("walk")    => 2,
                            Some("run")  => 3,  Some("sit")     => 4,
                            Some("sleep")=> 5,  Some("wake")    => 6,
                            Some("grabbed")=>7, Some("petted")  => 8,
                            Some("react")=> 9,  Some("fall")    => 10,
                            Some("thrown")=>11, _               => 0,
                        };
                        SendMessageW(combo, CB_SETCURSEL, idx, 0);
                    }
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
            }
        }

        ID_BTN_ADD_TAG => {
            ctx.add_form_visible = !ctx.add_form_visible;
            let vis = if ctx.add_form_visible { SW_SHOW } else { SW_HIDE };
            for ctrl_id in &[ID_EDIT_TAG_NAME, ID_EDIT_TAG_FROM, ID_EDIT_TAG_TO,
                              ID_COMBO_DIR, ID_BTN_TAG_OK] {
                ShowWindow(GetDlgItem(hwnd, *ctrl_id), vis);
            }
        }

        ID_BTN_TAG_OK => {
            // Read the add-tag form and create a new EditorTag
            let name = read_window_text(GetDlgItem(hwnd, ID_EDIT_TAG_NAME));
            if name.is_empty() { return; }
            let from = read_u32_field(hwnd, ID_EDIT_TAG_FROM, 0, 9999).unwrap_or(0) as usize;
            let to   = read_u32_field(hwnd, ID_EDIT_TAG_TO,   0, 9999).unwrap_or(0) as usize;
            let dir_idx = SendMessageW(GetDlgItem(hwnd, ID_COMBO_DIR), CB_GETCURSEL, 0, 0) as usize;
            let direction = match dir_idx {
                1 => TagDirection::Reverse,
                2 => TagDirection::PingPong,
                3 => TagDirection::PingPongReverse,
                _ => TagDirection::Forward,
            };
            let color = SpriteEditorState::assign_color(ctx.state.tags.len());
            ctx.state.tags.push(EditorTag { name, from, to, direction, color });
            ctx.state.selected_tag = Some(ctx.state.tags.len() - 1);
            refresh_tags_list(hwnd, &ctx.state);
            update_save_button(hwnd, &ctx.state);
            // Hide form
            ctx.add_form_visible = false;
            for ctrl_id in &[ID_EDIT_TAG_NAME, ID_EDIT_TAG_FROM, ID_EDIT_TAG_TO,
                              ID_COMBO_DIR, ID_BTN_TAG_OK] {
                ShowWindow(GetDlgItem(hwnd, *ctrl_id), SW_HIDE);
            }
            InvalidateRect(hwnd, std::ptr::null(), 0);
        }

        ID_BTN_REMOVE_TAG => {
            if let Some(sel) = ctx.state.selected_tag {
                if sel < ctx.state.tags.len() {
                    ctx.state.tags.remove(sel);
                    ctx.state.selected_tag = if ctx.state.tags.is_empty() {
                        None
                    } else {
                        Some(sel.saturating_sub(1))
                    };
                    refresh_tags_list(hwnd, &ctx.state);
                    update_save_button(hwnd, &ctx.state);
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
            }
        }

        ID_COMBO_BEHAVIOR => {
            if notify == CBN_SELCHANGE as u16 {
                let sel = ctx.state.selected_tag;
                if let Some(tag_idx) = sel {
                    if let Some(tag) = ctx.state.tags.get(tag_idx) {
                        let tag_name = tag.name.clone();
                        let combo = GetDlgItem(hwnd, ID_COMBO_BEHAVIOR);
                        let beh_idx = SendMessageW(combo, CB_GETCURSEL, 0, 0) as usize;
                        let beh_names = ["", "idle","walk","run","sit","sleep","wake",
                                         "grabbed","petted","react","fall","thrown"];
                        let beh = beh_names.get(beh_idx).copied().unwrap_or("");
                        set_behavior_mapping(&mut ctx.state.tag_map, beh, &tag_name);
                        refresh_tags_list(hwnd, &ctx.state);
                        update_save_button(hwnd, &ctx.state);
                    }
                }
            }
        }

        ID_BTN_SAVE => {
            if ctx.state.is_saveable() {
                let dir = crate::window::sprite_gallery::SpriteGallery::appdata_sprites_dir();
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    show_error(hwnd, &format!("Could not create sprites dir: {e}"));
                    return;
                }
                match ctx.state.save_to_dir(&dir) {
                    Ok(()) => {
                        let msg = wide("Sprite saved. It will appear in the gallery next time you open Configure.");
                        let title = wide("Saved");
                        MessageBoxW(hwnd, msg.as_ptr(), title.as_ptr(), MB_ICONINFORMATION | MB_OK);
                    }
                    Err(e) => show_error(hwnd, &format!("Save failed: {e}")),
                }
            }
        }

        ID_BTN_EXPORT => {
            export_dialog(hwnd, ctx);
        }

        _ => {}
    }
}

/// Map a behavior state name to the given tag name in `tag_map`.
/// Clears any previous mapping for this behavior first.
fn set_behavior_mapping(
    tm: &mut crate::sprite::behavior::AnimTagMap,
    behavior: &str,
    tag_name: &str,
) {
    match behavior {
        "idle"    => tm.idle    = tag_name.to_string(),
        "walk"    => tm.walk    = tag_name.to_string(),
        "run"     => tm.run     = Some(tag_name.to_string()),
        "sit"     => tm.sit     = Some(tag_name.to_string()),
        "sleep"   => tm.sleep   = Some(tag_name.to_string()),
        "wake"    => tm.wake    = Some(tag_name.to_string()),
        "grabbed" => tm.grabbed = Some(tag_name.to_string()),
        "petted"  => tm.petted  = Some(tag_name.to_string()),
        "react"   => tm.react   = Some(tag_name.to_string()),
        "fall"    => tm.fall    = Some(tag_name.to_string()),
        "thrown"  => tm.thrown  = Some(tag_name.to_string()),
        _         => {} // "— not set —" → no-op
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

unsafe fn read_u32_field(hwnd: HWND, id: i32, min: u32, max: u32) -> Option<u32> {
    let text = read_window_text(GetDlgItem(hwnd, id));
    let v: u32 = text.trim().parse().ok()?;
    if v >= min && v <= max { Some(v) } else { None }
}

unsafe fn read_window_text(hwnd: HWND) -> String {
    let len = GetWindowTextLengthW(hwnd) as usize;
    if len == 0 { return String::new(); }
    let mut buf = vec![0u16; len + 1];
    GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    String::from_utf16_lossy(&buf[..len])
}

unsafe fn show_error(hwnd: HWND, msg: &str) {
    let w_msg   = wide(msg);
    let w_title = wide("Error");
    MessageBoxW(hwnd, w_msg.as_ptr(), w_title.as_ptr(), MB_ICONERROR | MB_OK);
}

unsafe fn export_dialog(hwnd: HWND, ctx: &SpriteEditorCtx) {
    use windows_sys::Win32::UI::Controls::Dialogs::{GetSaveFileNameW, OPENFILENAMEW};
    let stem = ctx.state.png_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let filter = wide("JSON files\0*.json\0All files\0*.*\0");
    let mut file_buf: Vec<u16> = wide(&stem);
    file_buf.resize(1024, 0);

    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner   = hwnd;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile   = file_buf.as_mut_ptr();
    ofn.nMaxFile    = file_buf.len() as u32;
    ofn.Flags       = windows_sys::Win32::UI::Controls::Dialogs::OFN_OVERWRITEPROMPT;

    if GetSaveFileNameW(&mut ofn) == 0 { return; }

    let json_path = std::path::PathBuf::from(String::from_utf16_lossy(
        &file_buf[..file_buf.iter().position(|&c| c == 0).unwrap_or(0)],
    ));
    let dir = json_path.parent().unwrap_or(std::path::Path::new("."));

    // Write clean JSON to chosen path directly
    match std::fs::write(&json_path, ctx.state.to_clean_json()) {
        Ok(()) => {
            // Also copy the PNG next to it
            let png_dest = json_path.with_extension("png");
            if let Err(e) = std::fs::copy(&ctx.state.png_path, &png_dest) {
                show_error(hwnd, &format!("Could not copy PNG: {e}"));
            } else {
                let msg = wide(&format!("Exported to {}", json_path.display()));
                let title = wide("Exported");
                MessageBoxW(hwnd, msg.as_ptr(), title.as_ptr(), MB_ICONINFORMATION | MB_OK);
            }
        }
        Err(e) => show_error(hwnd, &format!("Export failed: {e}")),
    }
}
```

- [ ] **Step 7: Verify it compiles**

```bash
cd D:/elazar/private/my-pet && cargo build 2>&1 | grep "^error"
```

Expected: no errors. (Warnings about unused items are OK.)

- [ ] **Step 8: Commit**

```bash
cd D:/elazar/private/my-pet && git add src/tray/sprite_editor.rs src/tray/mod.rs src/sprite/editor_state.rs && git commit -m "feat: add Win32 sprite editor window"
```

---

## Chunk 3: Config dialog integration

> **Prerequisite:** Chunk 2 (Task 3) must be complete and committed before starting this chunk. This chunk calls `crate::tray::sprite_editor::show_sprite_editor`, which requires `pub mod sprite_editor;` in `src/tray/mod.rs` (added in Chunk 2 Task 3 Step 1) and the `SpriteEditorState` type (from Chunk 1 Task 2).

### Task 4: Wire "Edit…" and "New from PNG…" in `src/tray/config_window.rs`

**Files:**
- Modify: `src/tray/config_window.rs`

Add two buttons to the config dialog next to the gallery listbox. "Edit…" opens the sprite editor for the selected sprite (copying embedded sprites first). "New from PNG…" opens a file picker then the editor on a blank grid.

- [ ] **Step 1: Add control IDs**

In `src/tray/config_window.rs`, add these constants alongside the existing `const ID_*` lines:

```rust
const ID_BTN_EDIT_SPRITE: i32 = 120;
const ID_BTN_NEW_SPRITE:  i32 = 121;
```

- [ ] **Step 2: Add the buttons in `create_controls`**

In `create_controls` (the function that creates all child windows), add after the gallery listbox creation:

```rust
push_btn!("Edit…",        ID_BTN_EDIT_SPRITE, 14,  306, 72, 22);
push_btn!("New from PNG…",ID_BTN_NEW_SPRITE,  90,  306, 92, 22);
// Edit… is disabled until a gallery entry is selected
EnableWindow(GetDlgItem(hwnd, ID_BTN_EDIT_SPRITE), 0);
```

The gallery listbox is defined at `14, 76, 150, 224` (x, y, w, h), so its bottom edge is at y = 76 + 224 = 300. The buttons sit 6px below at y = 306, which is what the macro calls above already specify.

- [ ] **Step 3: Handle the buttons and enable/disable logic in `handle_command`**

In `handle_command`, add two arms before the `_ => {}` catch-all:

```rust
ID_BTN_EDIT_SPRITE => {
    edit_selected_sprite(hwnd, ctx);
}
ID_BTN_NEW_SPRITE => {
    new_sprite_from_png(hwnd, ctx);
}
```

Also, in the **existing** `ID_LIST_GALLERY` arm, add enable/disable of the Edit button whenever the gallery selection changes. Find the `if notify == LBN_SELCHANGE as u16` block and add a line to enable the button only when a real sprite (not the Browse sentinel) is selected:

```rust
// Inside the existing ID_LIST_GALLERY arm, at the top of the LBN_SELCHANGE handler:
let lb = GetDlgItem(hwnd, ID_LIST_GALLERY);
let sel = SendMessageW(lb, LB_GETCURSEL, 0, 0) as usize;
let has_sprite = sel < ctx.gallery.entries.len();
EnableWindow(GetDlgItem(hwnd, ID_BTN_EDIT_SPRITE), if has_sprite { 1 } else { 0 });
// … rest of existing LBN_SELCHANGE logic (key clone, select_sprite, etc.) unchanged
```

- [ ] **Step 4: Implement `edit_selected_sprite` and `new_sprite_from_png`**

Add these functions to `config_window.rs` (near the other free functions):

```rust
unsafe fn edit_selected_sprite(hwnd: HWND, ctx: &mut DialogCtx) {
    // SpriteKey is already imported at the top of config_window.rs.
    use crate::sprite::editor_state::SpriteEditorState;

    let key = ctx.state.selected_sprite.clone();
    let (png_path, image) = match &key {
        SpriteKey::Embedded(stem) => {
            // Offer to copy embedded sprite to AppData for editing
            let msg = wide("This is a built-in sprite.\nCreate an editable copy in your sprites folder?");
            let title = wide("Edit Sprite");
            if MessageBoxW(hwnd, msg.as_ptr(), title.as_ptr(),
                           MB_ICONQUESTION | MB_YESNO) != IDYES as i32 { return; }
            let Some((json_bytes, png_bytes)) = crate::assets::embedded_sheet(stem) else { return };
            let dest_dir = crate::window::sprite_gallery::SpriteGallery::appdata_sprites_dir();
            let _ = std::fs::create_dir_all(&dest_dir);
            let dest_png = dest_dir.join(format!("{stem}.png"));
            let dest_json = dest_dir.join(format!("{stem}.json"));
            if let Err(e) = std::fs::write(&dest_png, png_bytes) {
                let msg = wide(&format!("Could not copy PNG: {e}"));
                let t = wide("Error");
                MessageBoxW(hwnd, msg.as_ptr(), t.as_ptr(), MB_ICONERROR | MB_OK);
                return;
            }
            if let Err(e) = std::fs::write(&dest_json, json_bytes) {
                let msg = wide(&format!("Could not copy JSON: {e}"));
                let t = wide("Error");
                MessageBoxW(hwnd, msg.as_ptr(), t.as_ptr(), MB_ICONERROR | MB_OK);
                return;
            }
            let image = match image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png) {
                Ok(img) => img.into_rgba8(),
                Err(_) => return,
            };
            (dest_png, image)
        }
        SpriteKey::Installed(path) => {
            let png_path = path.with_extension("png");
            let png_bytes = match std::fs::read(&png_path) { Ok(b) => b, Err(_) => return };
            let image = match image::load_from_memory_with_format(&png_bytes, image::ImageFormat::Png) {
                Ok(img) => img.into_rgba8(),
                Err(_) => return,
            };
            (png_path, image)
        }
    };

    let mut state = SpriteEditorState::new(png_path, image);
    // Pre-load existing tag definitions from installed JSON if available
    if let SpriteKey::Installed(json_path) = &key {
        if let Ok(json_bytes) = std::fs::read(json_path) {
            let png_bytes_opt = std::fs::read(json_path.with_extension("png")).ok();
            if let Some(png_bytes) = png_bytes_opt {
                if let Ok((sheet, tag_map_opt)) = crate::sprite::sheet::load_with_tag_map(&json_bytes, &png_bytes) {
                    // Import existing tags
                    for t in &sheet.tags {
                        let color = SpriteEditorState::assign_color(state.tags.len());
                        state.tags.push(crate::sprite::editor_state::EditorTag {
                            name: t.name.clone(),
                            from: t.from,
                            to: t.to,
                            direction: t.direction.clone(),
                            color,
                        });
                    }
                    if let Some(tm) = tag_map_opt { state.tag_map = tm; }
                }
            }
        }
    }

    crate::tray::sprite_editor::show_sprite_editor(hwnd, state);
}

unsafe fn new_sprite_from_png(hwnd: HWND, _ctx: &mut DialogCtx) {
    // GetOpenFileNameW and OPENFILENAMEW are already imported at the top of config_window.rs.
    use crate::sprite::editor_state::SpriteEditorState;

    let filter = wide("PNG images\0*.png\0All files\0*.*\0");
    let mut file_buf = vec![0u16; 1024];

    let mut ofn: OPENFILENAMEW = std::mem::zeroed();
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner   = hwnd;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile   = file_buf.as_mut_ptr();
    ofn.nMaxFile    = file_buf.len() as u32;
    // OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST — same numeric literals as browse_and_install
    ofn.Flags       = 0x00001000 | 0x00000800;

    if GetOpenFileNameW(&mut ofn) == 0 { return; }

    let path = std::path::PathBuf::from(String::from_utf16_lossy(
        &file_buf[..file_buf.iter().position(|&c| c == 0).unwrap_or(0)],
    ));
    let png_bytes = match std::fs::read(&path) { Ok(b) => b, Err(_) => return };
    let image = match image::load_from_memory_with_format(&png_bytes, image::ImageFormat::Png) {
        Ok(img) => img.into_rgba8(),
        Err(e) => {
            let msg = wide(&format!("Could not load PNG: {e}"));
            let t = wide("Error");
            MessageBoxW(hwnd, msg.as_ptr(), t.as_ptr(), MB_ICONERROR | MB_OK);
            return;
        }
    };
    let state = SpriteEditorState::new(path, image);
    crate::tray::sprite_editor::show_sprite_editor(hwnd, state);
}
```

- [ ] **Step 5: Build and verify**

```bash
cd D:/elazar/private/my-pet && cargo build 2>&1 | grep "^error"
```

Expected: no errors.

- [ ] **Step 6: Run all tests**

```bash
cd D:/elazar/private/my-pet && cargo test -- --test-threads=1
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
cd D:/elazar/private/my-pet && git add src/tray/config_window.rs && git commit -m "feat: add Edit and New from PNG buttons to config dialog"
```

---

## Verification

After all tasks complete:

```bash
cargo test -- --test-threads=1
cargo build --release
```

Manual smoke test:
1. `cargo run`
2. Right-click tray → Configure
3. Select "arrows" in gallery → click "Edit…"
4. App asks to create an editable copy → confirm
5. Editor window opens showing the spritesheet with a 1×1 grid
6. Change Cols to 2 → grid shows 2 cells
7. Click "+ Add Tag" → enter name "idle", from 0, to 1, direction pingpong → OK
8. Click "idle" in tags list → select behavior "idle" from dropdown → tag shows "idle [0-1] → idle"
9. Click "+ Add Tag" → enter "walk", from 0, to 1, forward → OK → select behavior "walk"
10. "Save" button becomes enabled → click Save → success message
11. Click "Export…" → save to Desktop → verify JSON + PNG written, JSON has no `myPetTagMap`
12. Close editor → re-open Configure → gallery shows the saved sprite
