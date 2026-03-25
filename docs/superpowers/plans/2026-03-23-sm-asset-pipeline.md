# SM Asset Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `smMappings` to sprite sheets, implement the `.petbundle` ZIP format, wire the config window's SM selector, and add the SM coverage panel to the sprite editor.

**Architecture:** `smMappings` is stored in sprite JSON under `meta.smMappings` (keyed by SM name). A new `bundle.rs` module handles ZIP import/export. The sprite editor gains an SM switcher dropdown and a coverage panel. The config window gains an SM picker replacing the old tag map editor.

**Tech Stack:** Rust, `zip` crate (new dependency), `rfd` (existing native dialogs), `egui` (existing)

**Prerequisite:** SM Core Engine plan must be complete (`SMRunner`, `CompiledSM`, `CompileError` all exist).

**Threading model for `SmGallery`:** `SmGallery` is wrapped in `Arc<Mutex<SmGallery>>` and shared between the app thread and egui threads (SM editor, sprite editor). Egui threads lock it briefly to read names and source text for display, and to call `save()`/`import()`. The app thread locks it to load compiled SMs when building `PetInstance`. This is the same pattern used for other shared state in the codebase.

**Spec:** `docs/superpowers/specs/2026-03-23-user-defined-state-machines.md`

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src/sprite/sheet.rs` | **Modify** | Add `smMappings` field; add SM tag resolution API |
| `src/sprite/sm_gallery.rs` | **Create** | SM collection on disk: list, load, save, name-collision check |
| `src/sprite/sprite_gallery.rs` | **Create** | Sprite gallery on disk: `gallery.toml`, recommended SM storage |
| `src/bundle.rs` | **Create** | `.petbundle` ZIP import/export |
| `src/tray/sprite_editor.rs` | **Modify** | SM switcher + coverage panel; write `smMappings` on save |
| `src/tray/config_window.rs` | **Modify** | SM picker replacing tag map UI; recommended SM badge |
| `src/app.rs` | **Modify** | Load SM from disk path in `PetInstance`; handle new events |
| `src/event.rs` | **Modify** | Add `BundleImported`, `SMCollectionChanged` |
| `Cargo.toml` | **Modify** | Add `zip = "2"` dependency |

---

## Task 1: `smMappings` in `SpriteSheet`

**Files:**
- Modify: `src/sprite/sheet.rs`

- [ ] Add `smMappings` to `SpriteSheet`:

```rust
use std::collections::HashMap;

pub struct SpriteSheet {
    pub frames: Vec<Frame>,
    pub tags: Vec<FrameTag>,
    // NEW: keyed by SM name, then state name → tag name
    pub sm_mappings: HashMap<String, HashMap<String, String>>,
}
```

- [ ] In the JSON parser, read `meta.smMappings` into `sm_mappings`. If absent, produce an empty map.

- [ ] Add a tag resolution method:

```rust
impl SpriteSheet {
    /// Resolve SM state name to a sprite tag name.
    /// Resolution order: smMappings[sm_name][state], auto-match by name, None if not found.
    pub fn resolve_tag<'a>(&'a self, sm_name: &str, state_name: &str) -> Option<&'a str> {
        // 1. Explicit alias
        if let Some(mapping) = self.sm_mappings.get(sm_name) {
            if let Some(tag) = mapping.get(state_name) {
                return Some(tag.as_str());
            }
        }
        // 2. Auto-match: tag with same name exists
        if self.tags.iter().any(|t| t.name == state_name) {
            return Some(state_name);
        }
        None
    }
}
```

- [ ] Update `SMRunner::resolve_tag()` — Plan 1 Task 8 left a `// TODO(Plan-2-Task-1)` placeholder. Replace it now with a call to `sheet.resolve_tag(sm_name, state_name)`, falling back through `fallback` / `default_fallback` if `None` is returned:

```rust
fn resolve_tag<'a>(&self, sheet: &'a SpriteSheet, state_name: &str) -> &'a str {
    let sm_name = &self.sm.name;
    // Walk the fallback chain until we find a tag or exhaust options
    let mut candidate = state_name;
    loop {
        if let Some(tag) = sheet.resolve_tag(sm_name, candidate) {
            return tag;
        }
        // Try fallback
        let state = self.sm.states.get(candidate);
        if let Some(fb) = state.and_then(|s| s.fallback.as_deref()) {
            candidate = fb;
        } else {
            // Fall through to default_fallback
            return sheet.resolve_tag(sm_name, &self.sm.default_fallback)
                .unwrap_or(&self.sm.default_fallback);
        }
    }
}
```

- [ ] Write unit tests:

```rust
#[test]
fn resolve_tag_auto_match() {
    let sheet = sheet_with_tags(&["idle", "walk"]);
    assert_eq!(sheet.resolve_tag("Any SM", "idle"), Some("idle"));
}

#[test]
fn resolve_tag_explicit_alias() {
    let mut sheet = sheet_with_tags(&["idle_cycle"]);
    sheet.sm_mappings.insert("MyPet".into(), {
        let mut m = HashMap::new();
        m.insert("idle".into(), "idle_cycle".into());
        m
    });
    assert_eq!(sheet.resolve_tag("MyPet", "idle"), Some("idle_cycle"));
}

#[test]
fn resolve_tag_not_found() {
    let sheet = sheet_with_tags(&["idle"]);
    assert_eq!(sheet.resolve_tag("SM", "missing"), None);
}
```

- [ ] Run:
```
cargo test sheet
```

- [ ] Commit:
```
git add src/sprite/sheet.rs
git commit -m "feat: add smMappings to SpriteSheet with resolve_tag API"
```

---

## Task 2: SM collection on disk

**Files:**
- Create: `src/sprite/sm_gallery.rs`
- Modify: `src/sprite/mod.rs`

- [ ] Create `src/sprite/sm_gallery.rs`:

```rust
use std::path::{Path, PathBuf};
use crate::sprite::sm_compiler::{compile, CompiledSM, CompileError};
use crate::sprite::sm_format::SmFile;

use std::sync::Arc;

/// A loaded SM entry — either valid (compiled) or a draft (has errors).
pub enum SmEntry {
    Valid { name: String, path: PathBuf, source: String, sm: Arc<CompiledSM> },
    Draft { name: String, path: PathBuf, source: String, errors: Vec<CompileError> },
}

pub struct SmGallery {
    dir: PathBuf,
    draft_dir: PathBuf,
    entries: Vec<SmEntry>,
}

impl SmGallery {
    pub fn load(base_dir: &Path) -> Self { ... }

    /// List of valid SM names (for selection UI).
    pub fn valid_names(&self) -> Vec<&str> { ... }

    /// List of draft names (for SM editor browser).
    pub fn draft_names(&self) -> Vec<&str> { ... }

    /// Get a compiled SM by name (valid SMs only).
    pub fn get(&self, name: &str) -> Option<Arc<CompiledSM>> { ... }

    /// Get raw TOML source for a valid SM by name.
    pub fn source(&self, name: &str) -> Option<&str> { ... }

    /// Get raw TOML source for a draft SM by name.
    pub fn draft_source(&self, name: &str) -> Option<&str> { ... }

    /// Save source as live SM or draft depending on validation.
    /// Returns Ok(true) if saved as live, Ok(false) if saved as draft.
    pub fn save(&mut self, name: &str, source: &str) -> Result<bool, std::io::Error> { ... }

    /// Check for name collision with existing entries.
    pub fn name_exists(&self, name: &str) -> bool { ... }

    /// Import a .petstate file. Returns Err(collision_name) if name already exists.
    pub fn import(&mut self, source: &str) -> Result<bool, String> { ... }
}

fn sm_dir(base: &Path) -> PathBuf { base.join("state_machines") }
fn draft_dir(base: &Path) -> PathBuf { base.join("state_machines").join("drafts") }
```

- [ ] Add `pub mod sm_gallery;` to `src/sprite/mod.rs`.

- [ ] Write tests using a temp directory:

```rust
#[test]
fn save_valid_sm_appears_in_valid_list() {
    let dir = tempdir().unwrap();
    let mut gallery = SmGallery::load(dir.path());
    let result = gallery.save("Test SM", valid_sm_toml());
    assert_eq!(result.unwrap(), true);
    assert!(gallery.valid_names().contains(&"Test SM"));
}

#[test]
fn save_invalid_sm_appears_in_draft_list() {
    let dir = tempdir().unwrap();
    let mut gallery = SmGallery::load(dir.path());
    let result = gallery.save("Bad SM", invalid_sm_toml());
    assert_eq!(result.unwrap(), false);
    assert!(gallery.draft_names().contains(&"Bad SM"));
}
```

- [ ] Run:
```
cargo test sm_gallery
```

- [ ] Commit:
```
git add src/sprite/sm_gallery.rs src/sprite/mod.rs
git commit -m "feat: SM collection on disk with live/draft separation"
```

---

## Task 3: Sprite gallery with `gallery.toml`

**Files:**
- Create: `src/sprite/sprite_gallery.rs`
- Modify: `src/sprite/mod.rs`

- [ ] Create `src/sprite/sprite_gallery.rs`:

```rust
use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Default)]
struct GalleryFile {
    sprites: Vec<SpriteEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SpriteEntry {
    pub id: String,
    pub json_path: String,
    pub png_path: String,
    pub recommended_sm: Option<String>,  // SM name
}

pub struct SpriteGallery {
    dir: PathBuf,
    entries: Vec<SpriteEntry>,
}

impl SpriteGallery {
    pub fn load(base_dir: &Path) -> Self { ... }
    pub fn all(&self) -> &[SpriteEntry] { ... }
    pub fn get_by_id(&self, id: &str) -> Option<&SpriteEntry> { ... }
    pub fn add(&mut self, entry: SpriteEntry) -> Result<(), std::io::Error> { ... }
    pub fn set_recommended_sm(&mut self, sprite_id: &str, sm_name: Option<&str>) { ... }
    fn save_gallery_file(&self) -> Result<(), std::io::Error> { ... }
}
```

- [ ] Write minimal tests (load empty gallery, add entry, persist).

- [ ] Run:
```
cargo test sprite_gallery
```

- [ ] Commit:
```
git add src/sprite/sprite_gallery.rs src/sprite/mod.rs
git commit -m "feat: sprite gallery with gallery.toml and recommended SM tracking"
```

---

## Task 4: `.petbundle` ZIP format

**Files:**
- Modify: `Cargo.toml`
- Create: `src/bundle.rs`
- Modify: `src/main.rs`

- [ ] Add to `Cargo.toml`:
```toml
zip = "2"
```

- [ ] Create `src/bundle.rs`:

```rust
use std::io::{Read, Write, Cursor};
use zip::{ZipArchive, ZipWriter, write::FileOptions};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct BundleMeta {
    name: String,
    author: Option<String>,
    version: String,
    recommended_sm: Option<String>,
}

pub struct BundleContents {
    pub bundle_name: String,
    pub sprite_json: String,
    pub sprite_png: Vec<u8>,
    pub sm_source: Option<String>,          // .petstate text
    pub recommended_sm: Option<String>,     // SM name from bundle.toml
}

/// Import a .petbundle file from bytes.
pub fn import(data: &[u8]) -> Result<BundleContents, String> {
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    let meta: BundleMeta = {
        let mut f = archive.by_name("bundle.toml").map_err(|_| "missing bundle.toml")?;
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(|e| e.to_string())?;
        toml::from_str(&s).map_err(|e| e.to_string())?
    };

    let sprite_json = read_text(&mut archive, "sprite.json")?;
    let sprite_png  = read_bytes(&mut archive, "sprite.png")?;
    let sm_source   = read_text(&mut archive, "behavior.petstate").ok();

    Ok(BundleContents {
        bundle_name: meta.name,
        sprite_json,
        sprite_png,
        sm_source,
        recommended_sm: meta.recommended_sm,
    })
}

/// Export a .petbundle to bytes.
pub fn export(
    bundle_name: &str,
    author: Option<&str>,
    sprite_json: &str,
    sprite_png: &[u8],
    sm_source: Option<&str>,
    recommended_sm: Option<&str>,
) -> Result<Vec<u8>, String> { ... }

fn read_text(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<String, String> { ... }
fn read_bytes(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Vec<u8>, String> { ... }
```

- [ ] Add `mod bundle;` to `src/main.rs`.

- [ ] Write round-trip test:

```rust
#[test]
fn bundle_round_trip() {
    let json = r#"{"frames":[],"meta":{"frameTags":[]}}"#;
    let png = vec![0u8; 16]; // fake PNG
    let sm = "[meta]\nname = \"T\"\n...";

    let bytes = export("Test", None, json, &png, Some(sm), Some("T")).unwrap();
    let contents = import(&bytes).unwrap();

    assert_eq!(contents.bundle_name, "Test");
    assert_eq!(contents.sprite_json, json);
    assert_eq!(contents.sprite_png, png);
    assert!(contents.sm_source.is_some());
}

#[test]
fn sprite_only_bundle_imports_without_sm() {
    let bytes = export("Sprite Only", None, "{}", &[], None, None).unwrap();
    let contents = import(&bytes).unwrap();
    assert!(contents.sm_source.is_none());
}
```

- [ ] Run:
```
cargo test bundle
```

- [ ] Commit:
```
git add src/bundle.rs src/main.rs Cargo.toml Cargo.lock
git commit -m "feat: .petbundle ZIP import/export"
```

---

## Task 5: Bundle import flow in `app.rs`

**Files:**
- Modify: `src/app.rs`
- Modify: `src/event.rs`

- [ ] Add to `src/event.rs`:

```rust
BundleImported { sprite_id: String, sm_name: Option<String> },
SMCollectionChanged,
```

- [ ] Add a `import_bundle(path)` method to `App` that:
  1. Reads the file, calls `bundle::import()`
  2. Checks name collisions for sprite and SM; shows prompt (or auto-rename for now)
  3. Saves sprite JSON+PNG to `sprites/` directory
  4. Saves SM to SM gallery (live or draft)
  5. Updates `gallery.toml` with recommended SM association
  6. Sends `AppEvent::BundleImported`

- [ ] Add "Import Bundle" entry to the system tray menu.

- [ ] Handle `AppEvent::BundleImported` to show a status message (simple: log it; the config window will surface this in Plan 3).

- [ ] Run app and test importing a manually-created `.petbundle` zip:
```
cargo run
```

- [ ] Commit:
```
git add src/app.rs src/event.rs
git commit -m "feat: bundle import flow with sprite and SM gallery integration"
```

---

## Task 6: Bundle export in sprite editor

**Files:**
- Modify: `src/tray/sprite_editor.rs`

- [ ] Add an "Export Bundle" button to the sprite editor toolbar (next to the existing "Save" button).

- [ ] On click: open a dialog to select an SM from `SmGallery::valid_names()` (or "None — sprite only"). Then open a save dialog (`rfd::FileDialog::new().add_filter("Pet Bundle", &["petbundle"]).save_file()`). Call `bundle::export()` and write the file.

- [ ] Run app, open sprite editor, click "Export Bundle", verify the file is created and can be re-imported.

- [ ] Commit:
```
git add src/tray/sprite_editor.rs
git commit -m "feat: export bundle button in sprite editor"
```

---

## Task 7: `smMappings` editor in sprite editor — SM switcher

**Files:**
- Modify: `src/tray/sprite_editor.rs`

- [ ] Add an SM switcher `ComboBox` to the sprite editor's left panel header, populated from `SmGallery::valid_names()` + `"(none)"`.

- [ ] When SM is selected, store `selected_sm_name: Option<String>` in `SpriteEditorViewport`.

- [ ] Keep the full `smMappings` dict in `SpriteEditorViewport` memory (all SM mappings for the current sprite, not just the selected one). Load it when a sprite is opened from `sheet.sm_mappings`.

- [ ] On SM switch: the displayed mapping changes (different key in the dict); dirty flag unaffected by the switch itself.

- [ ] When saving the sprite, write back the full `smMappings` dict to the JSON.

- [ ] Write integration test: load a sheet, set an alias, switch SM, switch back, verify alias is preserved.

- [ ] Commit:
```
git add src/tray/sprite_editor.rs
git commit -m "feat: SM switcher in sprite editor with smMappings memory"
```

---

## Task 8: SM coverage panel in sprite editor

**Files:**
- Modify: `src/tray/sprite_editor.rs`

- [ ] When `selected_sm_name` is `Some`, render a coverage panel below the tag list in the left panel. For each state in the selected SM:

```
✓ auto       idle         (tag "idle" exists)
✓ walk_cycle → patrol     (explicit alias)
⚠ missing    walk_cycle   (alias tag deleted)
✗ REQUIRED   stand        (no resolution)
○ fallback   sunbathe → idle  (optional, will degrade)
```

- [ ] For `✗ REQUIRED` states: show a tag-name dropdown populated from the sprite's existing tags. Selecting sets the alias in `smMappings[sm_name][state_name]` and marks sprite dirty.

- [ ] Dangling alias detection: on each frame, check that all aliased tag names still exist in `sheet.tags`. Show `⚠` if not. On save, strip dangling entries and log a warning.

- [ ] Run app, open sprite editor, select an SM, verify coverage panel appears.

- [ ] Commit:
```
git add src/tray/sprite_editor.rs
git commit -m "feat: SM coverage panel in sprite editor with alias editing"
```

---

## Task 9: Config window — SM selector

**Files:**
- Modify: `src/tray/config_window.rs`

- [ ] Replace the tag map section in the config window with an SM picker:

```
State Machine:  [Default Pet ▼]   ★ Recommended: Default Pet [Use it]
```

- [ ] Populate the `ComboBox` from `SmGallery::valid_names()` plus `"embedded://default"`.

- [ ] Show the recommended SM badge when the selected sprite has a `recommended_sm` entry in the sprite gallery and it differs from the current `state_machine` value. The `[Use it]` button sets `cfg.state_machine = recommended_sm_name` and sends `ConfigChanged`.

- [ ] Wire: when `state_machine` changes in `PetConfig`, `app.rs::apply_config()` rebuilds the pet's `SMRunner` with the new SM.

- [ ] Run app, open config, change SM from default to another, verify pet reloads.

- [ ] Commit:
```
git add src/tray/config_window.rs
git commit -m "feat: SM picker in config window with recommended SM badge"
```

---

## Verification

After all tasks complete:

1. `cargo test` — all tests pass
2. Create a minimal `.petbundle` zip manually (any zip tool), import it — sprite and SM appear in their respective collections
3. Open sprite editor, load a sprite, select an SM — coverage panel shows state coverage
4. Set an alias for a missing state — saved to sprite JSON and persists on reload
5. Open config window, select a sprite with a recommended SM — badge appears, click "Use it" — pet changes behavior
