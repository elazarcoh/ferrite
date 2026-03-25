# Config Dialog Redesign Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Win95-era config dialog with a modern dark-themed sprite-first design featuring a gallery, animated preview, and custom sprite install flow.

**Architecture:** Extract the pure dialog model into `config/dialog_state.rs`, add gallery logic in `window/sprite_gallery.rs`, and rewrite `tray/config_window.rs` as Win32-only glue with dark theme, owner-draw gallery, and WM_TIMER preview animation.

**Tech Stack:** Rust, windows-sys (Win32 raw FFI), rust-embed (embedded assets), image crate (PNG decode)

**Spec:** `docs/superpowers/specs/2026-03-16-config-dialog-redesign.md`

---

## Chunk 1: `src/config/dialog_state.rs` — Pure Model

### Task 1: Add missing Cargo.toml feature flag

The gallery listbox owner-draw requires `DRAWITEMSTRUCT` and `MEASUREITEMSTRUCT` from `Win32_UI_Controls`.

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1.1: Add Win32_UI_Controls feature**

Edit `Cargo.toml` windows-sys features array to add `"Win32_UI_Controls"`:

```toml
windows-sys = { version = "0.61", features = [
    "Win32_Foundation",
    "Win32_Graphics_Gdi",
    "Win32_UI_Controls",
    "Win32_UI_WindowsAndMessaging",
    "Win32_UI_Controls_Dialogs",
    "Win32_UI_Shell",
    "Win32_System_LibraryLoader",
    "Win32_System_SystemServices",
] }
```

- [ ] **Step 1.2: Verify build still compiles**

Run: `cargo build 2>&1 | tail -5`
Expected: no errors

---

### Task 2: Write failing tests for new dialog_state features

These tests must fail before implementation — they reference types/methods that don't exist yet.

**Files:**
- Modify: `tests/e2e/test_config_dialog_e2e.rs`

- [ ] **Step 2.1: Add imports for new types at top of test file**

The existing import line in `tests/e2e/test_config_dialog_e2e.rs`:
```rust
use my_pet::{
    config::schema::{Config, PetConfig},
    tray::config_window::{ConfigDialogState, DialogResult},
};
```
Add below it:
```rust
use my_pet::config::dialog_state::SpriteKey;
use std::path::PathBuf;
```

- [ ] **Step 2.2: Append new tests to the file**

```rust
// ── SpriteKey tests ──────────────────────────────────────────────────────────

#[test]
fn sprite_key_roundtrip_embedded() {
    let key = SpriteKey::Embedded("esheep".into());
    let path = key.to_sheet_path();
    assert_eq!(path, "embedded://esheep");
    assert_eq!(SpriteKey::from_sheet_path(&path), key);
}

#[test]
fn sprite_key_roundtrip_installed() {
    let key = SpriteKey::Installed(PathBuf::from("C:/sprites/my_cat.json"));
    let path = key.to_sheet_path();
    assert_eq!(SpriteKey::from_sheet_path(&path), key);
}

// ── select_sprite / selected_sprite tests ────────────────────────────────────

#[test]
fn dialog_state_select_sprite_updates_path() {
    let mut state = ConfigDialogState::new(Config::default());
    state.select_sprite(SpriteKey::Embedded("esheep".into()));
    assert_eq!(state.config.pets[0].sheet_path, "embedded://esheep");
    assert_eq!(state.selected_sprite, SpriteKey::Embedded("esheep".into()));
}

#[test]
fn dialog_state_new_derives_selected_sprite_from_sheet_path() {
    let cfg = Config {
        pets: vec![PetConfig { sheet_path: "embedded://esheep".into(), ..PetConfig::default() }],
    };
    let state = ConfigDialogState::new(cfg);
    assert_eq!(state.selected_sprite, SpriteKey::Embedded("esheep".into()));
}

// ── update_walk_speed tests ───────────────────────────────────────────────────

#[test]
fn dialog_state_update_walk_speed_valid() {
    let mut state = ConfigDialogState::new(Config::default());
    assert!(state.update_walk_speed("80"));
    assert!((state.config.pets[0].walk_speed - 80.0).abs() < 0.001);
    assert!(state.update_walk_speed("80.5"));
    assert!((state.config.pets[0].walk_speed - 80.5).abs() < 0.001);
    assert!(state.update_walk_speed("1"));
    assert!(state.update_walk_speed("500"));
}

#[test]
fn dialog_state_update_walk_speed_invalid() {
    let mut state = ConfigDialogState::new(Config::default());
    let original = state.config.pets[0].walk_speed;
    assert!(!state.update_walk_speed("0"));
    assert!(!state.update_walk_speed("-1"));
    assert!(!state.update_walk_speed("abc"));
    assert!(!state.update_walk_speed("501"));
    assert!(!state.update_walk_speed("0.5"));
    // State must not have been mutated
    assert!((state.config.pets[0].walk_speed - original).abs() < 0.001);
}
```

- [ ] **Step 2.3: Verify tests fail (types not yet defined)**

Run: `cargo test dialog_state_select_sprite_updates_path 2>&1 | tail -20`
Expected: compile error — `SpriteKey` not found or `select_sprite` not found

---

### Task 3: Create `src/config/dialog_state.rs`

**Files:**
- Create: `src/config/dialog_state.rs`

- [ ] **Step 3.1: Write the file**

```rust
use crate::config::schema::{Config, PetConfig};
use std::path::PathBuf;

// ─── SpriteKey ────────────────────────────────────────────────────────────────

/// Identifies a sprite in the gallery.
#[derive(Debug, Clone, PartialEq)]
pub enum SpriteKey {
    /// A sprite bundled with the app, referenced by its asset stem (e.g. "esheep").
    Embedded(String),
    /// A user-installed sprite, referenced by absolute path to its .json file.
    Installed(PathBuf),
}

impl SpriteKey {
    /// Returns the `sheet_path` string stored in `PetConfig`.
    pub fn to_sheet_path(&self) -> String {
        match self {
            SpriteKey::Embedded(stem) => format!("embedded://{stem}"),
            SpriteKey::Installed(p) => p.to_string_lossy().into_owned(),
        }
    }

    /// Parses a `sheet_path` string back into a `SpriteKey`.
    pub fn from_sheet_path(s: &str) -> Self {
        if let Some(stem) = s.strip_prefix("embedded://") {
            SpriteKey::Embedded(stem.to_string())
        } else {
            SpriteKey::Installed(PathBuf::from(s))
        }
    }
}

// ─── DialogResult ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum DialogResult {
    None,
    Ok,
    Cancel,
}

// ─── ConfigDialogState ───────────────────────────────────────────────────────

pub struct ConfigDialogState {
    pub config: Config,
    /// Index into `config.pets` for the currently selected pet chip.
    pub selected: usize,
    /// Currently highlighted gallery entry.
    pub selected_sprite: SpriteKey,
    pub result: DialogResult,
}

impl ConfigDialogState {
    pub fn new(config: Config) -> Self {
        let selected_sprite = config
            .pets
            .first()
            .map(|p| SpriteKey::from_sheet_path(&p.sheet_path))
            .unwrap_or_else(|| SpriteKey::Embedded("esheep".into()));
        ConfigDialogState { config, selected: 0, selected_sprite, result: DialogResult::None }
    }

    pub fn selected_pet(&self) -> Option<&PetConfig> {
        self.config.pets.get(self.selected)
    }

    fn selected_pet_mut(&mut self) -> Option<&mut PetConfig> {
        self.config.pets.get_mut(self.selected)
    }

    pub fn add_pet(&mut self) {
        let n = self.config.pets.len();
        self.config.pets.push(PetConfig { id: format!("pet_{n}"), ..PetConfig::default() });
        self.selected = self.config.pets.len() - 1;
    }

    pub fn remove_selected(&mut self) {
        if self.config.pets.is_empty() {
            return;
        }
        self.config.pets.remove(self.selected);
        if !self.config.pets.is_empty() && self.selected >= self.config.pets.len() {
            self.selected = self.config.pets.len() - 1;
        }
    }

    pub fn select(&mut self, index: usize) {
        if index < self.config.pets.len() {
            self.selected = index;
        }
    }

    /// Update the currently selected sprite and write its path to the selected pet's config.
    pub fn select_sprite(&mut self, key: SpriteKey) {
        let path = key.to_sheet_path();
        self.selected_sprite = key;
        self.update_sheet_path(path);
    }

    pub fn update_sheet_path(&mut self, path: String) {
        if let Some(p) = self.selected_pet_mut() {
            p.sheet_path = path;
        }
    }

    /// Returns `true` if scale was valid (1–4).
    pub fn update_scale(&mut self, s: &str) -> bool {
        match Self::parse_scale(s) {
            Some(v) => {
                if let Some(p) = self.selected_pet_mut() {
                    p.scale = v;
                }
                true
            }
            None => false,
        }
    }

    pub fn parse_scale(s: &str) -> Option<u32> {
        let v: u32 = s.trim().parse().ok()?;
        if (1..=4).contains(&v) { Some(v) } else { None }
    }

    pub fn update_x(&mut self, s: &str) -> bool {
        match s.trim().parse::<i32>() {
            Ok(v) => {
                if let Some(p) = self.selected_pet_mut() {
                    p.x = v;
                }
                true
            }
            Err(_) => false,
        }
    }

    pub fn update_y(&mut self, s: &str) -> bool {
        match s.trim().parse::<i32>() {
            Ok(v) => {
                if let Some(p) = self.selected_pet_mut() {
                    p.y = v;
                }
                true
            }
            Err(_) => false,
        }
    }

    /// Returns `true` if speed was valid (1.0–500.0 inclusive).
    /// Does not mutate state on invalid input.
    pub fn update_walk_speed(&mut self, s: &str) -> bool {
        let v: f32 = match s.trim().parse() {
            Ok(f) => f,
            Err(_) => return false,
        };
        if v < 1.0 || v > 500.0 {
            return false;
        }
        if let Some(p) = self.selected_pet_mut() {
            p.walk_speed = v;
        }
        true
    }

    pub fn accept(&mut self) {
        self.result = DialogResult::Ok;
    }

    pub fn cancel(&mut self) {
        self.result = DialogResult::Cancel;
    }
}
```

---

### Task 4: Update module declarations and re-exports

**Files:**
- Modify: `src/config/mod.rs`
- Modify: `src/tray/config_window.rs`

- [ ] **Step 4.1: Add `dialog_state` submodule to `src/config/mod.rs`**

Add `pub mod dialog_state;` at the top of `src/config/mod.rs` (after the existing `pub mod schema;` line).

- [ ] **Step 4.2: Strip old model code from `src/tray/config_window.rs` and add re-exports**

In `src/tray/config_window.rs`:

a) Remove the entire `DialogResult` enum definition (lines 39–44).
b) Remove the entire `ConfigDialogState` struct and its `impl` block (lines 46–148).
c) Replace the top-of-file import block with:

```rust
use crate::config::dialog_state::{ConfigDialogState, DialogResult, SpriteKey};
use crate::config::schema::Config;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::HBRUSH,
    System::LibraryLoader::GetModuleHandleW,
    UI::Controls::Dialogs::{GetOpenFileNameW, OPENFILENAMEW},
    UI::WindowsAndMessaging::*,
};

// Re-export for backward compatibility with existing tests
pub use crate::config::dialog_state::{ConfigDialogState, DialogResult};
```

d) Remove `ID_EDIT_PATH = 104`, `ID_BTN_BROWSE = 105`, `ID_EDIT_TAG = 107` from the constants block. Add `ID_EDIT_SPEED = 110`.

e) Remove the `update_tag` call in `read_fields` (it no longer exists). Update `read_fields` to read speed instead:

```rust
unsafe fn read_fields(hwnd: HWND, state: &mut ConfigDialogState) {
    let mut buf = [0u16; 512];
    macro_rules! get_text {
        ($id:expr) => {{
            let n = GetWindowTextW(GetDlgItem(hwnd, $id), buf.as_mut_ptr(), buf.len() as i32);
            String::from_utf16_lossy(&buf[..n.max(0) as usize])
        }};
    }
    let scale = get_text!(ID_EDIT_SCALE);
    state.update_scale(&scale);
    let x = get_text!(ID_EDIT_X);
    state.update_x(&x);
    let y = get_text!(ID_EDIT_Y);
    state.update_y(&y);
    let speed = get_text!(ID_EDIT_SPEED);
    state.update_walk_speed(&speed);
}
```

f) In `create_controls`, remove these three blocks verbatim (they create the old path/browse/tag/x/y controls):

```rust
    // Sheet path
    label!("Sheet path:", 10, 125, 90);
    edit!(10, 143, 330, ID_EDIT_PATH);
    btn!("Browse...", 350, 143, 100, 22, ID_BTN_BROWSE, BS_PUSHBUTTON);

    // Scale + tag
    label!("Scale (1-4):", 10, 179, 80);
    edit!(10, 199, 50, ID_EDIT_SCALE);
    label!("Start tag:", 80, 179, 70);
    edit!(80, 199, 160, ID_EDIT_TAG);

    // X / Y
    label!("X:", 10, 237, 20);
    edit!(10, 257, 70, ID_EDIT_X);
    label!("Y:", 100, 237, 20);
    edit!(100, 257, 70, ID_EDIT_Y);
```

And also remove the OK/Cancel buttons block (they will be re-added with `BS_OWNERDRAW` in Task 10):
```rust
    btn!("OK",     300, 310, 80, 28, DLG_OK,     BS_DEFPUSHBUTTON);
    btn!("Cancel", 395, 310, 80, 28, DLG_CANCEL, BS_PUSHBUTTON);
```

The entire `create_controls` function body after `// Pets list` becomes:

```rust
    // Pets list
    label!("Pets:", 10, 10, 300);
    CreateWindowExW(
        WS_EX_CLIENTEDGE,
        wide("LISTBOX").as_ptr(),
        wide("").as_ptr(),
        WS_CHILD | WS_VISIBLE | tab | LBS_NOTIFY as u32 | WS_VSCROLL,
        10, 30, 300, 80,
        hwnd, ID_LIST as usize as HMENU, hi, std::ptr::null(),
    );
    btn!("Add",    320, 30, 80, 24, ID_BTN_ADD,    BS_PUSHBUTTON);
    btn!("Remove", 320, 60, 80, 24, ID_BTN_REMOVE, BS_PUSHBUTTON);
    // (Gallery, preview, speed, X, Y, and Save/Cancel are created by create_controls in Task 10)
```

This stub keeps the old pet-list UI temporarily so the existing e2e tests still compile while the full rewrite happens in Chunk 3.

g) Remove the `browse_for_file` helper function (lines 326–353 of the original). Remove the `ID_BTN_BROWSE` match arm from `handle_command`. Remove the `update_tag` call from `read_fields`. The stub `refresh_fields` for now should only set scale, x, y, and speed:

```rust
unsafe fn refresh_fields(hwnd: HWND, state: &ConfigDialogState) {
    if let Some(pet) = state.selected_pet() {
        set_ctrl_text(hwnd, ID_EDIT_SCALE, &pet.scale.to_string());
        set_ctrl_text(hwnd, ID_EDIT_X, &pet.x.to_string());
        set_ctrl_text(hwnd, ID_EDIT_Y, &pet.y.to_string());
        set_ctrl_text(hwnd, ID_EDIT_SPEED, &pet.walk_speed.to_string());
    }
}
```

- [ ] **Step 4.3: Verify build compiles**

Run: `cargo build 2>&1 | tail -20`
Expected: no errors

---

### Task 5: Run all tests — chunk 1 complete

- [ ] **Step 5.1: Run all dialog state tests**

Run: `cargo test config_dialog 2>&1 | tail -30`
Expected: all existing tests pass; the 6 new tests (sprite_key_roundtrip_*, dialog_state_select_sprite*, dialog_state_update_walk_speed*) pass.

- [ ] **Step 5.2: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: all tests pass (no regressions)

- [ ] **Step 5.3: Commit**

```bash
git add src/config/dialog_state.rs src/config/mod.rs src/tray/config_window.rs Cargo.toml tests/e2e/test_config_dialog_e2e.rs
git commit -m "feat: extract ConfigDialogState to config/dialog_state.rs; add SpriteKey + update_walk_speed"
```

---

## Chunk 2: `src/window/sprite_gallery.rs` — Gallery Logic

### Task 6: Write failing gallery tests

**Files:**
- Create: `tests/integration/test_sprite_gallery.rs`
- Modify: `tests/integration.rs`

- [ ] **Step 6.1: Register the new test module in `tests/integration.rs`**

The project uses `include!()` macros, not bare `mod` declarations. Add this block to `tests/integration.rs`:

```rust
mod sprite_gallery {
    include!("integration/test_sprite_gallery.rs");
}
```

- [ ] **Step 6.2: Write failing tests**

Create `tests/integration/test_sprite_gallery.rs`:

```rust
//! Tests for SpriteGallery — gallery load, install, and appdata resolution.
//! Uses MY_PET_SPRITES_DIR env var to redirect installs to a tempdir.

use my_pet::window::sprite_gallery::{SourceKind, SpriteGallery};
use std::path::PathBuf;
use tempfile::TempDir;

/// Returns a TempDir and sets MY_PET_SPRITES_DIR to its path.
/// The TempDir must be kept alive for the duration of the test.
fn temp_sprites_dir() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    std::env::set_var("MY_PET_SPRITES_DIR", &path);
    (dir, path)
}

/// Copy test_pet assets from the embedded store to a temp dir to act as a
/// source for install() tests. Returns paths to the .json and .png files.
fn write_test_sprite_source(dir: &TempDir) -> (PathBuf, PathBuf) {
    use my_pet::assets::Assets;
    use rust_embed::Embed;
    let json_bytes = Assets::get("test_pet.json").unwrap();
    let png_bytes = Assets::get("test_pet.png").unwrap();
    let json_path = dir.path().join("test_pet.json");
    let png_path = dir.path().join("test_pet.png");
    std::fs::write(&json_path, &json_bytes.data).unwrap();
    std::fs::write(&png_path, &png_bytes.data).unwrap();
    (json_path, png_path)
}

#[test]
fn gallery_load_skips_test_pet() {
    let _sprites_dir = temp_sprites_dir();
    let gallery = SpriteGallery::load();
    let names: Vec<&str> = gallery.entries.iter().map(|e| e.display_name.as_str()).collect();
    assert!(!names.contains(&"test_pet"), "test_pet must not appear in user-visible gallery");
    // eSheep is embedded and should appear
    assert!(names.contains(&"esheep") || names.iter().any(|n| n.eq_ignore_ascii_case("esheep")));
}

#[test]
fn gallery_load_finds_installed() {
    let (sprites_dir, sprites_path) = temp_sprites_dir();
    // Write a custom sprite JSON into the sprites dir (simulates a prior install).
    // SpriteGallery::load() only scans for *.json files — it does NOT validate
    // or open the PNG, so we only need the JSON file for this test.
    let json = r#"{"frames":[{"frame":{"x":0,"y":0,"w":32,"h":32},"duration":100}],"meta":{"size":{"w":32,"h":32},"frameTags":[]}}"#;
    std::fs::write(sprites_path.join("my_cat.json"), json).unwrap();

    let gallery = SpriteGallery::load();
    let entry = gallery.entries.iter().find(|e| e.display_name == "my_cat");
    assert!(entry.is_some(), "installed sprite must appear in gallery");
    assert!(matches!(entry.unwrap().source, SourceKind::Custom));
    drop(sprites_dir);
}

#[test]
fn install_sprite_copies_files() {
    let (sprites_dir, sprites_path) = temp_sprites_dir();
    let src_dir = tempfile::tempdir().unwrap();
    let (json_path, _png_path) = write_test_sprite_source(&src_dir);

    let entry = SpriteGallery::install(&json_path).expect("install must succeed");
    assert_eq!(entry.display_name, "test_pet");
    assert!(sprites_path.join("test_pet.json").exists());
    assert!(sprites_path.join("test_pet.png").exists());
    assert!(matches!(entry.source, SourceKind::Custom));
    drop(sprites_dir);
}

#[test]
fn install_sprite_rejects_missing_png() {
    let (sprites_dir, _) = temp_sprites_dir();
    let src_dir = tempfile::tempdir().unwrap();
    let (json_path, png_path) = write_test_sprite_source(&src_dir);
    std::fs::remove_file(&png_path).unwrap();

    let result = SpriteGallery::install(&json_path);
    assert!(result.is_err(), "install must fail when PNG is absent");
    drop(sprites_dir);
}

#[test]
fn install_sprite_overwrites_existing() {
    let (sprites_dir, _) = temp_sprites_dir();
    let src_dir = tempfile::tempdir().unwrap();
    let (json_path, _) = write_test_sprite_source(&src_dir);

    SpriteGallery::install(&json_path).unwrap();
    // Second install must not error
    SpriteGallery::install(&json_path).expect("second install of same stem must succeed");
    drop(sprites_dir);
}
```

- [ ] **Step 6.3: Verify tests fail**

Run: `cargo test gallery_load 2>&1 | tail -20`
Expected: compile error — `sprite_gallery` module not found

---

### Task 7: Create `src/window/sprite_gallery.rs`

**Files:**
- Create: `src/window/sprite_gallery.rs`
- Modify: `src/window/mod.rs`

- [ ] **Step 7.1: Add `pub mod sprite_gallery;` to `src/window/mod.rs`**

Append `pub mod sprite_gallery;` to the file.

- [ ] **Step 7.2: Write the sprite_gallery module**

```rust
//! Gallery discovery, thumbnail loading, and custom sprite install.
//!
//! `load_thumbnail` and `destroy_thumbnails` use Win32 GDI and must be called
//! from the Win32 thread. All other methods are pure Rust.

use crate::assets::{self, Assets};
use crate::config::dialog_state::SpriteKey;
use crate::sprite::sheet::load_embedded;
use anyhow::{anyhow, Context, Result};
use rust_embed::Embed;
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::HWND,
    Graphics::Gdi::{
        BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC,
        ReleaseDC, SelectObject, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
        HBITMAP, SRCCOPY,
    },
    Graphics::Gdi::BI_RGB,
};

// ─── Public types ─────────────────────────────────────────────────────────────

/// Whether a sprite is bundled with the app or user-installed.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceKind {
    BuiltIn,
    Custom,
}

pub struct GalleryEntry {
    pub key: SpriteKey,
    pub display_name: String,
    pub source: SourceKind,
    /// 28×28 HBITMAP thumbnail; `None` until `load_thumbnail` is called.
    #[cfg(target_os = "windows")]
    pub thumbnail: Option<HBITMAP>,
    #[cfg(not(target_os = "windows"))]
    pub thumbnail: Option<()>,
}

pub struct SpriteGallery {
    /// Real sprite entries only. The Browse sentinel is drawn separately.
    pub entries: Vec<GalleryEntry>,
}

// ─── Embedded asset stems ────────────────────────────────────────────────────

/// Collect stems of embedded sprites that have BOTH a .json and a .png.
/// Excludes "test_pet" (internal only, not user-visible).
fn embedded_stems() -> Vec<String> {
    let mut jsons: std::collections::HashSet<String> = Default::default();
    let mut pngs: std::collections::HashSet<String> = Default::default();
    for path in Assets::iter() {
        let path = path.as_ref();
        if let Some(stem) = path.strip_suffix(".json") {
            jsons.insert(stem.to_string());
        } else if let Some(stem) = path.strip_suffix(".png") {
            pngs.insert(stem.to_string());
        }
    }
    let mut stems: Vec<String> = jsons
        .intersection(&pngs)
        .filter(|s| *s != "test_pet")
        .cloned()
        .collect();
    stems.sort();
    stems
}

// ─── SpriteGallery ───────────────────────────────────────────────────────────

impl SpriteGallery {
    /// Discover all available sprites (embedded + installed).
    /// Does NOT load thumbnails — call `load_thumbnail` per entry before painting.
    pub fn load() -> Self {
        let mut entries: Vec<GalleryEntry> = Vec::new();

        // Embedded sprites
        for stem in embedded_stems() {
            entries.push(GalleryEntry {
                key: SpriteKey::Embedded(stem.clone()),
                display_name: stem,
                source: SourceKind::BuiltIn,
                thumbnail: None,
            });
        }

        // Installed custom sprites
        let dir = Self::appdata_sprites_dir();
        if dir.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&dir) {
                let mut custom: Vec<PathBuf> = rd
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
                    .collect();
                custom.sort();
                for json_path in custom {
                    let stem = json_path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    entries.push(GalleryEntry {
                        display_name: stem.clone(),
                        key: SpriteKey::Installed(json_path),
                        source: SourceKind::Custom,
                        thumbnail: None,
                    });
                }
            }
        }

        SpriteGallery { entries }
    }

    /// Returns the directory where custom sprites are stored.
    /// In tests, overridable via `MY_PET_SPRITES_DIR` environment variable.
    pub fn appdata_sprites_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("MY_PET_SPRITES_DIR") {
            return PathBuf::from(dir);
        }
        let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
        PathBuf::from(base).join("my-pet").join("sprites")
    }

    /// Validate and copy a `.json` + adjacent `.png` into the sprites directory.
    /// Returns a new `GalleryEntry` with `thumbnail = None`; caller must call
    /// `load_thumbnail` before painting.
    pub fn install(json_path: &Path) -> Result<GalleryEntry> {
        // Validate: the JSON must be parseable and the PNG must exist alongside it.
        let json_bytes = std::fs::read(json_path)
            .with_context(|| format!("read {}", json_path.display()))?;
        // Parse to validate — reuse load_embedded with a tiny 1x1 PNG if PNG is absent,
        // then check PNG separately.
        let stem = json_path
            .file_stem()
            .ok_or_else(|| anyhow!("json_path has no stem"))?
            .to_string_lossy()
            .into_owned();
        let png_path = json_path.with_extension("png");
        if !png_path.exists() {
            return Err(anyhow!("missing PNG adjacent to JSON: {}", png_path.display()));
        }
        let png_bytes = std::fs::read(&png_path)
            .with_context(|| format!("read {}", png_path.display()))?;
        // Validate JSON is a real SpriteSheet
        load_embedded(&json_bytes, &png_bytes)
            .with_context(|| format!("invalid sprite at {}", json_path.display()))?;

        // Copy to sprites directory
        let dest_dir = Self::appdata_sprites_dir();
        std::fs::create_dir_all(&dest_dir).context("create sprites dir")?;
        let dest_json = dest_dir.join(format!("{stem}.json"));
        let dest_png = dest_dir.join(format!("{stem}.png"));
        std::fs::copy(json_path, &dest_json)
            .with_context(|| format!("copy JSON to {}", dest_json.display()))?;
        std::fs::copy(&png_path, &dest_png)
            .with_context(|| format!("copy PNG to {}", dest_png.display()))?;

        Ok(GalleryEntry {
            key: SpriteKey::Installed(dest_json),
            display_name: stem,
            source: SourceKind::Custom,
            thumbnail: None,
        })
    }

    /// Load a 28×28 thumbnail HBITMAP for the given entry.
    /// Stores the result in `entry.thumbnail`. No-op if already loaded.
    /// Must be called from the Win32 thread.
    #[cfg(target_os = "windows")]
    pub fn load_thumbnail(entry: &mut GalleryEntry) {
        if entry.thumbnail.is_some() {
            return;
        }
        let sheet = match &entry.key {
            SpriteKey::Embedded(stem) => {
                let Some((json, png)) = assets::embedded_sheet(stem) else { return };
                match load_embedded(&json, &png) {
                    Ok(s) => s,
                    Err(_) => return,
                }
            }
            SpriteKey::Installed(path) => {
                let Ok(json) = std::fs::read(path) else { return };
                let Ok(png) = std::fs::read(path.with_extension("png")) else { return };
                match load_embedded(&json, &png) {
                    Ok(s) => s,
                    Err(_) => return,
                }
            }
        };

        // Find idle tag frame index; fall back to frame 0
        let frame_idx = sheet
            .tags
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case("idle"))
            .map(|t| t.from)
            .unwrap_or(0);
        let Some(frame) = sheet.frames.get(frame_idx) else { return };

        // Convert RGBA → BGRA (Win32 DIB order)
        let img_w = sheet.image.width() as i32;
        let img_h = sheet.image.height() as i32;
        let bgra: Vec<u8> = sheet
            .image
            .pixels()
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();

        unsafe {
            // Create a 28×28 top-down DIBSection as the render target
            let hdc_screen = GetDC(std::ptr::null_mut());
            let hdc_mem = CreateCompatibleDC(hdc_screen);

            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = 28;
            bmi.bmiHeader.biHeight = -28; // top-down
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB as u32;

            let mut bits = std::ptr::null_mut();
            let hbmp = CreateDIBSection(hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            if hbmp.is_null() {
                DeleteDC(hdc_mem);
                ReleaseDC(std::ptr::null_mut(), hdc_screen);
                return;
            }
            SelectObject(hdc_mem, hbmp as *mut _);

            // Source BITMAPINFO for StretchDIBits (full spritesheet image)
            let mut src_bmi: BITMAPINFO = std::mem::zeroed();
            src_bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            src_bmi.bmiHeader.biWidth = img_w;
            src_bmi.bmiHeader.biHeight = -img_h; // top-down
            src_bmi.bmiHeader.biPlanes = 1;
            src_bmi.bmiHeader.biBitCount = 32;
            src_bmi.bmiHeader.biCompression = BI_RGB as u32;

            StretchDIBits(
                hdc_mem,
                0, 0, 28, 28,                                            // dest rect
                frame.x as i32, frame.y as i32,
                frame.w as i32, frame.h as i32,                          // src rect (frame)
                bgra.as_ptr() as *const _,
                &src_bmi,
                DIB_RGB_COLORS,
                SRCCOPY,
            );

            ReleaseDC(std::ptr::null_mut(), hdc_screen);
            DeleteDC(hdc_mem);

            entry.thumbnail = Some(hbmp);
        }
    }

    /// Delete all GDI HBITMAP handles. Must be called from `WM_DESTROY`.
    #[cfg(target_os = "windows")]
    pub fn destroy_thumbnails(&mut self) {
        for entry in &mut self.entries {
            if let Some(hbmp) = entry.thumbnail.take() {
                unsafe { DeleteObject(hbmp as *mut _); }
            }
        }
    }
}
```

- [ ] **Step 7.3: Verify build compiles**

Run: `cargo build 2>&1 | tail -20`
Expected: no errors

---

### Task 8: Run gallery tests

- [ ] **Step 8.1: Run gallery tests**

Run: `cargo test test_sprite_gallery 2>&1 | tail -30`
Expected: all 5 gallery tests pass

- [ ] **Step 8.2: Verify `write_test_sprite_source` works**

`install_sprite_copies_files`, `install_sprite_rejects_missing_png`, and `install_sprite_overwrites_existing` use `write_test_sprite_source`, which reads the real `test_pet.json` + `test_pet.png` from embedded assets via `Assets::get(...)`. Since `rust-embed` with `debug_include = false` (default) reads from the filesystem in debug mode, verify the asset files exist at `assets/test_pet.json` and `assets/test_pet.png`. Run:

```bash
ls assets/test_pet.json assets/test_pet.png
```

Expected: both files listed. No changes needed if they exist.

- [ ] **Step 8.3: Run full test suite**

Run: `cargo test 2>&1 | tail -10`
Expected: all tests pass

- [ ] **Step 8.4: Commit**

```bash
git add src/window/sprite_gallery.rs src/window/mod.rs tests/integration/test_sprite_gallery.rs tests/integration.rs
git commit -m "feat: add SpriteGallery with load, install, thumbnail loading"
```

---

## Chunk 3: `src/tray/config_window.rs` Rewrite — Dark Theme + Controls

This chunk rewrites `config_window.rs` as pure Win32 glue. The file will grow substantially; the model lives in `dialog_state.rs` so the glue file stays focused on Win32.

### Task 9: Define DialogCtx and register window classes

**Files:**
- Modify: `src/tray/config_window.rs`

- [ ] **Step 9.1: Add new imports to `config_window.rs`**

Replace the imports block with:

```rust
// EnableWindow lives in Win32_UI_Input_KeyboardAndMouse which is not in our feature set.
// Keep the manual FFI declaration from the original file:
#[link(name = "user32")]
unsafe extern "system" {
    fn EnableWindow(hwnd: HWND, enable: i32) -> i32;
}

// Re-export for backward compatibility with existing tests that import these from tray::config_window.
// These are also used internally — the pub use serves as the single binding so there is no duplicate.
pub use crate::config::dialog_state::{ConfigDialogState, DialogResult};
use crate::config::dialog_state::SpriteKey;
use crate::config::schema::Config;
use crate::sprite::animation::AnimationState;
use crate::sprite::sheet::load_embedded;
use crate::assets;
use crate::window::sprite_gallery::{GalleryEntry, SourceKind, SpriteGallery};

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, CreateSolidBrush,
        DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, GetClientRect, GetDC,
        ReleaseDC, SelectObject, SetBkColor, SetBkMode, SetTextColor, StretchDIBits,
        UpdateWindow, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DT_CENTER,
        DT_LEFT, DT_SINGLELINE, DT_TOP, DT_VCENTER, HBITMAP, HBRUSH, PAINTSTRUCT,
        SRCCOPY, TRANSPARENT, BI_RGB,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_SELECTED},
    UI::Controls::Dialogs::{GetOpenFileNameW, OPENFILENAMEW},
    UI::WindowsAndMessaging::*,
};
```

- [ ] **Step 9.2: Define control IDs and colors**

```rust
// ─── Control IDs ──────────────────────────────────────────────────────────────
const ID_LIST_GALLERY:   i32 = 101;
const ID_BTN_ADD_PET:    i32 = 102;
const ID_BTN_REMOVE_PET: i32 = 103;
const ID_EDIT_SCALE:     i32 = 106;
const ID_EDIT_X:         i32 = 108;
const ID_EDIT_Y:         i32 = 109;
const ID_EDIT_SPEED:     i32 = 110;
const DLG_OK:            i32 = 1;  // IDOK
const DLG_CANCEL:        i32 = 2;  // IDCANCEL
const TIMER_ANIM:        usize = 1001;

// ─── Colors (dark VS Code-ish theme) ─────────────────────────────────────────
// Win32 COLORREF format: 0x00BBGGRR  i.e. R | (G<<8) | (B<<16)
const fn clr_bg()       -> u32 { 0x1e | (0x1e << 8) | (0x1e << 16) } // #1e1e1e
const fn clr_bg_card()  -> u32 { 0x26 | (0x25 << 8) | (0x25 << 16) } // #252526
const fn clr_bg_ctrl()  -> u32 { 0x3c | (0x3c << 8) | (0x3c << 16) } // #3c3c3c
const fn clr_bg_sel()   -> u32 { 0x71 | (0x47 << 8) | (0x09 << 16) } // #094771
const fn clr_accent()   -> u32 { 0xcc | (0x7a << 8) | (0x00 << 16) } // #007acc
const fn clr_text()     -> u32 { 0xcc | (0xcc << 8) | (0xcc << 16) } // #cccccc
const fn clr_label()    -> u32 { 0x85 | (0x85 << 8) | (0x85 << 16) } // #858585
const fn clr_text_acc() -> u32 { 0xf7 | (0xc3 << 8) | (0x4f << 16) } // #4fc3f7 accent blue
```

- [ ] **Step 9.3: Define DialogCtx struct**

```rust
/// Heap-allocated context stored in GWLP_USERDATA.
struct DialogCtx {
    state: ConfigDialogState,
    gallery: SpriteGallery,
    chip_hwnds: Vec<HWND>,       // one per pet in state.config.pets
    preview_hwnd: HWND,          // SpritePreview child window
    preview_sheet: Option<crate::sprite::sheet::SpriteSheet>,
    preview_anim: AnimationState,
    dark_bg_brush: HBRUSH,       // CreateSolidBrush(CLR_BG) — deleted in WM_DESTROY
    ctrl_brush: HBRUSH,          // CreateSolidBrush(CLR_BG_CTRL) for edits
    card_brush: HBRUSH,          // CreateSolidBrush(CLR_BG_CARD) for listbox bg
}

impl DialogCtx {
    unsafe fn new(config: Config) -> Box<Self> {
        let state = ConfigDialogState::new(config);
        let gallery = SpriteGallery::load();
        Box::new(DialogCtx {
            state,
            gallery,
            chip_hwnds: Vec::new(),
            preview_hwnd: std::ptr::null_mut(),
            preview_sheet: None,
            preview_anim: AnimationState::new(""),
            dark_bg_brush: CreateSolidBrush(clr_bg()),
            ctrl_brush: CreateSolidBrush(clr_bg_ctrl()),
            card_brush: CreateSolidBrush(clr_bg()),
        })
    }

    unsafe fn destroy_brushes(&self) {
        DeleteObject(self.dark_bg_brush as *mut _);
        DeleteObject(self.ctrl_brush as *mut _);
        DeleteObject(self.card_brush as *mut _);
    }
}
```

- [ ] **Step 9.4: Register window classes**

Register three classes with `Once`:
1. `"MyPetConfigDlg"` — the main dialog
2. `"PetChip"` — pill-shaped pet selector chip
3. `"SpritePreview"` — animated preview pane

```rust
const DLG_CLASS:     &str = "MyPetConfigDlg";
const CHIP_CLASS:    &str = "PetChip";
const PREVIEW_CLASS: &str = "SpritePreview";

static CLASS_ONCE: std::sync::Once = std::sync::Once::new();

fn register_classes() {
    CLASS_ONCE.call_once(|| unsafe {
        let hi = GetModuleHandleW(std::ptr::null());

        // ── Main dialog ──
        let cls = wide(DLG_CLASS);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(config_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hi,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(), // handled in WM_ERASEBKGND
            lpszMenuName: std::ptr::null(),
            lpszClassName: cls.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);

        // ── PetChip ──
        let cls2 = wide(CHIP_CLASS);
        let wc2 = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(chip_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hi,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: cls2.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc2);

        // ── SpritePreview ──
        let cls3 = wide(PREVIEW_CLASS);
        let wc3 = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(preview_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hi,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: cls3.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc3);
    });
}
```

- [ ] **Step 9.5: Update `show_config_dialog` entry point**

```rust
pub fn show_config_dialog(parent: HWND, config: &Config) -> Option<Config> {
    register_classes();
    unsafe {
        let mut ctx = DialogCtx::new(config.clone());
        let ctx_ptr: *mut DialogCtx = &mut *ctx;

        let cls = wide(DLG_CLASS);
        let title = wide("My Pet — Configure");
        let style = WS_CAPTION | WS_SYSMENU | WS_CLIPSIBLINGS | WS_CLIPCHILDREN | WS_VISIBLE;

        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            cls.as_ptr(),
            title.as_ptr(),
            style,
            CW_USEDEFAULT, CW_USEDEFAULT,
            560, 440,      // new size: 560×440 px
            parent,
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return None;
        }
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ctx_ptr as isize);

        if !parent.is_null() {
            EnableWindow(parent, 0);
        }
        center_window(hwnd);

        // Modal message loop
        loop {
            let mut msg: MSG = std::mem::zeroed();
            let ret = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if ret == 0 {
                PostQuitMessage(msg.wParam as i32);
                break;
            }
            if ret == -1 { break; }
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if IsWindow(hwnd) == 0 { break; }
        }

        if !parent.is_null() {
            EnableWindow(parent, 1);
        }

        if ctx.state.result == DialogResult::Ok { Some(ctx.state.config) } else { None }
    }
}
```

- [ ] **Step 9.6: Verify build**

Run: `cargo build 2>&1 | tail -20`
Expected: no errors (some dead_code warnings on unimplemented wndprocs are fine)

---

### Task 10: WM_CREATE — create all controls and initialize state

**Files:**
- Modify: `src/tray/config_window.rs`

- [ ] **Step 10.1: Write `create_controls`**

```rust
unsafe fn create_controls(hwnd: HWND, ctx: &mut DialogCtx) {
    let hi = GetModuleHandleW(std::ptr::null());
    let tab = WS_TABSTOP;

    macro_rules! static_text {
        ($text:expr, $x:expr, $y:expr, $w:expr, $h:expr) => {
            CreateWindowExW(0, wide("STATIC").as_ptr(), wide($text).as_ptr(),
                WS_CHILD | WS_VISIBLE | 0 /*SS_LEFT*/,
                $x, $y, $w, $h, hwnd, std::ptr::null_mut(), hi, std::ptr::null())
        };
    }
    macro_rules! edit_ctrl {
        ($id:expr, $x:expr, $y:expr, $w:expr) => {
            CreateWindowExW(WS_EX_CLIENTEDGE,
                wide("EDIT").as_ptr(), wide("").as_ptr(),
                WS_CHILD | WS_VISIBLE | tab | ES_AUTOHSCROLL as u32,
                $x, $y, $w, 22, hwnd, $id as usize as HMENU, hi, std::ptr::null())
        };
    }
    macro_rules! push_btn {
        ($text:expr, $id:expr, $x:expr, $y:expr, $w:expr, $h:expr) => {
            CreateWindowExW(0, wide("BUTTON").as_ptr(), wide($text).as_ptr(),
                WS_CHILD | WS_VISIBLE | tab | BS_OWNERDRAW as u32,
                $x, $y, $w, $h, hwnd, $id as usize as HMENU, hi, std::ptr::null())
        };
    }

    // ── Section label: PETS ──
    static_text!("PETS", 14, 14, 60, 14);

    // ── Add Pet button (top-right of chip strip) ──
    push_btn!("+ Add pet", ID_BTN_ADD_PET, 460, 11, 80, 24);

    // ── Pet chips are created by create_pet_chips() after this fn ──

    // ── Divider area handled by WM_PAINT ──

    // ── Section label: SPRITE ──
    static_text!("SPRITE", 14, 58, 80, 14);

    // ── Gallery listbox (owner-draw, 150px wide) ──
    CreateWindowExW(
        0,
        wide("LISTBOX").as_ptr(),
        wide("").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | tab
            | LBS_NOTIFY as u32
            | LBS_OWNERDRAWFIXED as u32
            | LBS_HASSTRINGS as u32,
        14, 76, 150, 224,
        hwnd,
        ID_LIST_GALLERY as usize as HMENU,
        hi,
        std::ptr::null(),
    );

    // ── SpritePreview pane (fills the right half of the gallery row) ──
    let preview = CreateWindowExW(
        0,
        wide(PREVIEW_CLASS).as_ptr(),
        wide("").as_ptr(),
        WS_CHILD | WS_VISIBLE,
        174, 76, 368, 224,
        hwnd, std::ptr::null_mut(), hi, std::ptr::null(),
    );
    ctx.preview_hwnd = preview;

    // ── Divider ──
    // (drawn in WM_PAINT as a 1px dark line)

    // ── Settings row ──
    static_text!("Scale",  14, 314, 40, 14);
    edit_ctrl!(ID_EDIT_SCALE, 14, 330, 40);
    static_text!("X",      64, 314, 16, 14);
    edit_ctrl!(ID_EDIT_X,  64, 330, 60);
    static_text!("Y",     134, 314, 16, 14);
    edit_ctrl!(ID_EDIT_Y, 134, 330, 60);
    static_text!("Speed",  204, 314, 40, 14);
    edit_ctrl!(ID_EDIT_SPEED, 204, 330, 56);

    // ── Save / Cancel ──
    push_btn!("Cancel", DLG_CANCEL, 358, 395, 80, 28);
    push_btn!("Save",   DLG_OK,    450, 395, 80, 28);
}
```

- [ ] **Step 10.2: Write `populate_gallery_listbox`**

```rust
/// Fill the gallery listbox with `entries.len() + 1` string items.
/// The last item (index = entries.len()) is the Browse sentinel.
unsafe fn populate_gallery_listbox(hwnd: HWND, gallery: &SpriteGallery) {
    let lb = GetDlgItem(hwnd, ID_LIST_GALLERY);
    SendMessageW(lb, LB_RESETCONTENT, 0, 0);
    for entry in &gallery.entries {
        let w = wide(&entry.display_name);
        SendMessageW(lb, LB_ADDSTRING, 0, w.as_ptr() as LPARAM);
    }
    // Browse sentinel
    let browse = wide("Browse\u{2026}");
    SendMessageW(lb, LB_ADDSTRING, 0, browse.as_ptr() as LPARAM);
}
```

- [ ] **Step 10.3: Write `create_pet_chips`**

```rust
/// Destroy existing chip HWNDs and recreate from current state.
unsafe fn refresh_pet_chips(hwnd: HWND, ctx: &mut DialogCtx) {
    for chip in ctx.chip_hwnds.drain(..) {
        DestroyWindow(chip);
    }
    let hi = GetModuleHandleW(std::ptr::null());
    let mut x = 14i32;
    let chip_y = 32i32;
    let chip_h = 24i32;
    for (i, pet) in ctx.state.config.pets.iter().enumerate() {
        let label = wide(&format!("\u{1F436} {}", pet.id)); // 🐕 pet-id
        // Measure text width roughly: 8px per char + padding
        let w = (label.len() as i32 * 7 + 30).max(60).min(140);
        let chip = CreateWindowExW(
            0, wide(CHIP_CLASS).as_ptr(), label.as_ptr(),
            WS_CHILD | WS_VISIBLE,
            x, chip_y, w, chip_h,
            hwnd, std::ptr::null_mut(), hi, std::ptr::null(),
        );
        // Store pet index in GWLP_USERDATA
        SetWindowLongPtrW(chip, GWLP_USERDATA, i as isize);
        ctx.chip_hwnds.push(chip);
        x += w + 6;
    }
}
```

- [ ] **Step 10.4: Write `load_preview_for_sprite` helper**

```rust
/// Load preview sheet + animation for the currently selected sprite.
unsafe fn load_preview_for_sprite(ctx: &mut DialogCtx) {
    let sheet = match &ctx.state.selected_sprite {
        SpriteKey::Embedded(stem) => {
            let Some((json, png)) = assets::embedded_sheet(stem) else { return };
            load_embedded(&json, &png).ok()
        }
        SpriteKey::Installed(path) => {
            let Ok(json) = std::fs::read(path) else { return };
            let Ok(png) = std::fs::read(path.with_extension("png")) else { return };
            load_embedded(&json, &png).ok()
        }
    };
    if let Some(s) = sheet {
        // Find idle tag (case-insensitive), fall back to first tag, then ""
        let tag_name = s.tags.iter()
            .find(|t| t.name.eq_ignore_ascii_case("idle"))
            .or_else(|| s.tags.first())
            .map(|t| t.name.clone())
            .unwrap_or_default();
        ctx.preview_anim = AnimationState::new(&tag_name);
        ctx.preview_sheet = Some(s);
    } else {
        ctx.preview_sheet = None;
    }
    if !ctx.preview_hwnd.is_null() {
        InvalidateRect(ctx.preview_hwnd, std::ptr::null(), 0);
    }
}
```

- [ ] **Step 10.5: Wire WM_CREATE in config_wnd_proc and call setup from show_config_dialog**

In `config_wnd_proc`, `WM_CREATE` is handled as a no-op because the ctx pointer is not yet stored in `GWLP_USERDATA` at that point (Win32 delivers `WM_CREATE` inside `CreateWindowExW`, before the caller can call `SetWindowLongPtrW`):

```rust
WM_CREATE => {
    // Context pointer is set by show_config_dialog immediately after
    // CreateWindowExW returns. All setup runs in setup_dialog_controls.
    0
}
```

In `show_config_dialog`, add the call to `setup_dialog_controls` immediately after `SetWindowLongPtrW`. The full updated section of `show_config_dialog` (between `CreateWindowExW` and the modal loop) is:

```rust
        if hwnd.is_null() {
            return None;
        }
        // Store ctx pointer BEFORE calling setup (setup reads it via GWLP_USERDATA).
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ctx_ptr as isize);
        // Create all child controls, populate gallery, load preview sheet, start timer.
        setup_dialog_controls(hwnd, &mut *ctx_ptr);

        if !parent.is_null() {
            EnableWindow(parent, 0);
        }
        center_window(hwnd);
        // ... modal message loop follows unchanged ...
```

```rust
unsafe fn setup_dialog_controls(hwnd: HWND, ctx: &mut DialogCtx) {
    create_controls(hwnd, ctx);
    populate_gallery_listbox(hwnd, &ctx.gallery);
    refresh_pet_chips(hwnd, ctx);
    // Select the gallery entry matching the current sprite
    sync_gallery_selection(hwnd, ctx);
    // Fill settings fields
    refresh_fields(hwnd, &ctx.state);
    // Load preview sheet and start timer
    load_preview_for_sprite(ctx);
    SetTimer(hwnd, TIMER_ANIM, 100, None);
}
```

```rust
/// Select the listbox item that matches state.selected_sprite.
/// Requires `SpriteKey: PartialEq`, which is derived in `config/dialog_state.rs` (Chunk 1, Task 3).
unsafe fn sync_gallery_selection(hwnd: HWND, ctx: &DialogCtx) {
    let lb = GetDlgItem(hwnd, ID_LIST_GALLERY);
    for (i, entry) in ctx.gallery.entries.iter().enumerate() {
        if entry.key == ctx.state.selected_sprite {
            SendMessageW(lb, LB_SETCURSEL, i, 0);
            return;
        }
    }
    // No match — select Browse sentinel
    SendMessageW(lb, LB_SETCURSEL, ctx.gallery.entries.len(), 0);
}
```

- [ ] **Step 10.6: Verify build**

Run: `cargo build 2>&1 | tail -20`
Expected: no errors

---

### Task 11: Dark theme — WM_CTLCOLOR* + WM_ERASEBKGND

**Files:**
- Modify: `src/tray/config_window.rs`

- [ ] **Step 11.1: Add WM_CTLCOLOR* and WM_ERASEBKGND handlers to config_wnd_proc**

In `config_wnd_proc`, add these match arms. The context pointer is retrieved via `GWLP_USERDATA`:

```rust
WM_ERASEBKGND => {
    let ctx = get_ctx(hwnd);
    if ctx.is_null() { return DefWindowProcW(hwnd, msg, wparam, lparam); }
    let hdc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
    let mut rc: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rc);
    FillRect(hdc, &rc, (*ctx).dark_bg_brush);
    1 // non-zero = handled
}
WM_CTLCOLORSTATIC => {
    let ctx = get_ctx(hwnd);
    if ctx.is_null() { return DefWindowProcW(hwnd, msg, wparam, lparam); }
    let hdc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
    SetTextColor(hdc, clr_label());
    SetBkMode(hdc, TRANSPARENT as i32);
    (*ctx).dark_bg_brush as LRESULT
}
WM_CTLCOLOREDIT => {
    let ctx = get_ctx(hwnd);
    if ctx.is_null() { return DefWindowProcW(hwnd, msg, wparam, lparam); }
    let hdc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
    SetTextColor(hdc, clr_text());
    SetBkColor(hdc, clr_bg_ctrl());
    (*ctx).ctrl_brush as LRESULT
}
WM_CTLCOLORLISTBOX => {
    let ctx = get_ctx(hwnd);
    if ctx.is_null() { return DefWindowProcW(hwnd, msg, wparam, lparam); }
    let hdc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
    SetBkColor(hdc, clr_bg());
    (*ctx).card_brush as LRESULT
}
```

Helper:
```rust
unsafe fn get_ctx(hwnd: HWND) -> *mut DialogCtx {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DialogCtx
}
```

---

### Task 12: Owner-draw buttons (Save + Cancel)

**Files:**
- Modify: `src/tray/config_window.rs`

- [ ] **Step 12.1: Add stub for `draw_gallery_card`**

`WM_DRAWITEM` references `draw_gallery_card` which is fully implemented in Chunk 4. Add a stub here so Chunk 3 compiles:

```rust
/// Draw a gallery listbox card. Stub replaced in Chunk 4.
unsafe fn draw_gallery_card(_dis: &DRAWITEMSTRUCT) {
    // Full implementation in Task 14.
}
```

- [ ] **Step 12.2: Add WM_DRAWITEM handler for buttons**

Both Save and Cancel use `BS_OWNERDRAW`. In `WM_DRAWITEM`, check the item ID:

```rust
WM_DRAWITEM => {
    let dis = &*(lparam as *const DRAWITEMSTRUCT);
    let id = dis.CtlID as i32;
    match id {
        DLG_OK => {
            // Save button: blue fill, white text
            let hbr = CreateSolidBrush(clr_accent());
            FillRect(dis.hDC, &dis.rcItem, hbr);
            DeleteObject(hbr as *mut _);
            SetTextColor(dis.hDC, 0x00FFFFFF);
            SetBkMode(dis.hDC, TRANSPARENT as i32);
            let text = wide("Save");
            let mut rc = dis.rcItem;
            DrawTextW(dis.hDC, text.as_ptr(), -1, &mut rc,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }
        DLG_CANCEL => {
            // Cancel button: dark fill, dim text
            let hbr = CreateSolidBrush(clr_bg_ctrl());
            FillRect(dis.hDC, &dis.rcItem, hbr);
            DeleteObject(hbr as *mut _);
            SetTextColor(dis.hDC, clr_text());
            SetBkMode(dis.hDC, TRANSPARENT as i32);
            let text = wide("Cancel");
            let mut rc = dis.rcItem;
            DrawTextW(dis.hDC, text.as_ptr(), -1, &mut rc,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }
        ID_LIST_GALLERY => {
            // Gallery card drawing — handled in Task 13
            draw_gallery_card(dis);
        }
        _ => {}
    }
    1
}
```

All these symbols (`DrawTextW`, `DT_CENTER`, `DT_VCENTER`, `DT_SINGLELINE`) are already in the imports block from Step 9.1.

- [ ] **Step 12.3: Verify build**

Run: `cargo build 2>&1 | tail -20`
Expected: no errors

---

### Task 13: Pet chip wndproc

**Files:**
- Modify: `src/tray/config_window.rs`

- [ ] **Step 13.1: Write chip_wnd_proc**

```rust
unsafe extern "system" fn chip_wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rc);

            // Determine if this chip is selected
            let pet_idx = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as usize;
            let parent = GetParent(hwnd);
            let ctx = get_ctx(parent);
            let selected = !ctx.is_null() && (*ctx).state.selected == pet_idx;

            // Draw rounded rectangle background
            let (bg, border) = if selected {
                (clr_bg_sel(), clr_accent())
            } else {
                (0x2d2d2d_u32, 0x444444_u32)
            };
            let hbr = CreateSolidBrush(bg);
            FillRect(hdc, &rc, hbr);
            DeleteObject(hbr as *mut _);

            // Draw the label
            let mut buf = [0u16; 128];
            let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            let text_color = if selected { 0x00F7C34F_u32 } else { clr_text() };
            SetTextColor(hdc, text_color);
            SetBkMode(hdc, TRANSPARENT as i32);
            let mut text_rc = rc;
            text_rc.left += 8;
            text_rc.right -= 8;
            DrawTextW(hdc, buf.as_ptr(), n, &mut text_rc,
                DT_VCENTER | DT_SINGLELINE | DT_LEFT);

            EndPaint(hwnd, &ps);
            0
        }
        WM_LBUTTONDOWN => {
            // Notify parent that this chip was clicked
            let pet_idx = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as usize;
            let parent = GetParent(hwnd);
            // Use WM_COMMAND with ID = 2000 + pet_idx to signal chip selection
            PostMessageW(parent, WM_COMMAND, (2000 + pet_idx) as WPARAM, hwnd as LPARAM);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
```

In `handle_command`, add a range match for chip clicks:

```rust
id if id >= 2000 => {
    // Chip click: id - 2000 is the pet index
    let pet_idx = (id - 2000) as usize;
    ctx.state.select(pet_idx);
    refresh_fields(hwnd, &ctx.state);
    sync_gallery_selection(hwnd, ctx);
    load_preview_for_sprite(ctx);
    // Repaint all chips to update selection highlight
    for chip in &ctx.chip_hwnds {
        InvalidateRect(*chip, std::ptr::null(), 1);
    }
}
```

- [ ] **Step 13.2: Verify build**

Run: `cargo build 2>&1 | tail -20`
Expected: no errors

- [ ] **Step 13.3: Run tests to catch regressions**

Run: `cargo test config_dialog 2>&1 | tail -20`
Expected: all tests pass

- [ ] **Step 13.4: Commit chunk 3**

```bash
git add src/tray/config_window.rs
git commit -m "feat: rewrite config dialog shell — dark theme, DialogCtx, pet chips, owner-draw buttons"
```

---

## Chunk 4: `src/tray/config_window.rs` — Gallery + Preview + Wiring

### Task 14: WM_MEASUREITEM + WM_DRAWITEM gallery cards

**Files:**
- Modify: `src/tray/config_window.rs`

- [ ] **Step 14.1: Add WM_MEASUREITEM handler**

```rust
WM_MEASUREITEM => {
    let mis = &mut *(lparam as *mut MEASUREITEMSTRUCT);
    if mis.CtlID == ID_LIST_GALLERY as u32 {
        mis.itemHeight = 44;
    }
    1
}
```

- [ ] **Step 14.2: Write `draw_gallery_card`**

```rust
unsafe fn draw_gallery_card(dis: &DRAWITEMSTRUCT) {
    // Get ctx from parent dialog
    let parent = GetParent(dis.hwndItem);
    let ctx = get_ctx(parent);
    if ctx.is_null() { return; }
    let ctx = &mut *ctx;

    let idx = dis.itemID as usize;
    let is_browse = idx == ctx.gallery.entries.len();
    let is_selected = (dis.itemState & ODS_SELECTED) != 0;

    // Background
    let bg_color = if is_selected { clr_bg_sel() } else { clr_bg() };
    let hbr_bg = CreateSolidBrush(bg_color);
    FillRect(dis.hDC, &dis.rcItem, hbr_bg);
    DeleteObject(hbr_bg as *mut _);

    // Left accent bar (2px, blue) when selected
    if is_selected {
        let mut bar = dis.rcItem;
        bar.right = bar.left + 2;
        let hbr_bar = CreateSolidBrush(clr_accent());
        FillRect(dis.hDC, &bar, hbr_bar);
        DeleteObject(hbr_bar as *mut _);
    }

    let rc = dis.rcItem;
    let thumb_x = rc.left + 8;
    let thumb_y = rc.top + 8;
    let thumb_w = 28i32;
    let thumb_h = 28i32;

    if is_browse {
        // Browse card: folder icon placeholder + "Browse…" text
        let hbr_thumb = CreateSolidBrush(0x3c3c3c_u32);
        let thumb_rc = RECT { left: thumb_x, top: thumb_y, right: thumb_x + thumb_w, bottom: thumb_y + thumb_h };
        FillRect(dis.hDC, &thumb_rc, hbr_thumb);
        DeleteObject(hbr_thumb as *mut _);

        SetBkMode(dis.hDC, TRANSPARENT as i32);
        SetTextColor(dis.hDC, 0x555555_u32);
        let text = wide("Browse\u{2026}");
        let mut text_rc = RECT {
            left: thumb_x + thumb_w + 8,
            top: rc.top,
            right: rc.right - 4,
            bottom: rc.bottom,
        };
        DrawTextW(dis.hDC, text.as_ptr(), -1, &mut text_rc,
            DT_VCENTER | DT_SINGLELINE | DT_LEFT);
        return;
    }

    // Real gallery entry
    let entry = &mut ctx.gallery.entries[idx];

    // Lazy-load thumbnail
    SpriteGallery::load_thumbnail(entry);

    // Draw thumbnail
    if let Some(hbmp) = entry.thumbnail {
        let hdc_mem = CreateCompatibleDC(dis.hDC);
        SelectObject(hdc_mem, hbmp as *mut _);
        BitBlt(dis.hDC, thumb_x, thumb_y, thumb_w, thumb_h, hdc_mem, 0, 0, SRCCOPY);
        DeleteDC(hdc_mem);
    } else {
        // Placeholder rect
        let hbr = CreateSolidBrush(0x2d2d2d_u32);
        let thumb_rc = RECT { left: thumb_x, top: thumb_y, right: thumb_x + thumb_w, bottom: thumb_y + thumb_h };
        FillRect(dis.hDC, &thumb_rc, hbr);
        DeleteObject(hbr as *mut _);
    }

    // Display name — COLORREF for #4fc3f7 (accent blue): R=0x4f,G=0xc3,B=0xf7 → 0x00F7C34F
    SetBkMode(dis.hDC, TRANSPARENT as i32);
    let name_color_ref: u32 = if is_selected { 0x4f | (0xc3 << 8) | (0xf7 << 16) } else { clr_text() };
    SetTextColor(dis.hDC, name_color_ref);
    let name_w = wide(&entry.display_name);
    let mut name_rc = RECT {
        left: thumb_x + thumb_w + 8,
        top: rc.top + 8,
        right: rc.right - 4,
        bottom: rc.top + 26,
    };
    DrawTextW(dis.hDC, name_w.as_ptr(), -1, &mut name_rc,
        DT_LEFT | DT_TOP | DT_SINGLELINE);

    // Source label ("built-in" / "custom")
    let src_text = match entry.source {
        SourceKind::BuiltIn => "built-in",
        SourceKind::Custom => "custom",
    };
    let src_color: u32 = if is_selected { (0x3d) | (0x85 << 8) | (0xc8 << 16) } else { 0x00555555 };
    SetTextColor(dis.hDC, src_color);
    let src_w = wide(src_text);
    let mut src_rc = RECT {
        left: thumb_x + thumb_w + 8,
        top: rc.top + 26,
        right: rc.right - 4,
        bottom: rc.bottom - 4,
    };
    DrawTextW(dis.hDC, src_w.as_ptr(), -1, &mut src_rc,
        DT_LEFT | DT_TOP | DT_SINGLELINE);
}
```

---

### Task 15: SpritePreview wndproc + WM_TIMER animation

**Files:**
- Modify: `src/tray/config_window.rs`

- [ ] **Step 15.1: Write `preview_wnd_proc`**

```rust
unsafe extern "system" fn preview_wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);

            let parent = GetParent(hwnd);
            let ctx = get_ctx(parent);

            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rc);
            let pane_w = rc.right - rc.left;
            let pane_h = rc.bottom - rc.top;

            if ctx.is_null() || (*ctx).preview_sheet.is_none() {
                let hbr = CreateSolidBrush(0x141414_u32);
                FillRect(hdc, &rc, hbr);
                DeleteObject(hbr as *mut _);
                EndPaint(hwnd, &ps);
                return 0;
            }

            let ctx = &mut *ctx;
            let sheet = ctx.preview_sheet.as_ref().unwrap();
            let abs = ctx.preview_anim.absolute_frame(sheet);
            let frame = &sheet.frames[abs];

            // Build BGRA pixels from RGBA image
            let img_w = sheet.image.width() as i32;
            let img_h = sheet.image.height() as i32;
            let bgra: Vec<u8> = sheet.image.pixels()
                .flat_map(|p| [p[2], p[1], p[0], p[3]])
                .collect();

            // Create off-screen DIBSection (pane size)
            let hdc_mem = CreateCompatibleDC(hdc);
            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = pane_w;
            bmi.bmiHeader.biHeight = -pane_h;
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB as u32;
            let mut bits = std::ptr::null_mut();
            let hbmp = CreateDIBSection(hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            // Capture the previously selected object so we can restore it before deleting hbmp.
            let old_bmp = SelectObject(hdc_mem, hbmp as *mut _);

            // Fill background
            let mut bg_rc = rc;
            let hbr_bg = CreateSolidBrush(0x141414_u32);
            FillRect(hdc_mem, &bg_rc, hbr_bg);
            DeleteObject(hbr_bg as *mut _);

            // Draw frame centered in pane
            let draw_w = (frame.w as i32 * 2).min(pane_w - 8);
            let draw_h = (frame.h as i32 * 2).min(pane_h - 8);
            let draw_x = (pane_w - draw_w) / 2;
            let draw_y = (pane_h - draw_h) / 2;

            let mut src_bmi: BITMAPINFO = std::mem::zeroed();
            src_bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            src_bmi.bmiHeader.biWidth = img_w;
            src_bmi.bmiHeader.biHeight = -img_h;
            src_bmi.bmiHeader.biPlanes = 1;
            src_bmi.bmiHeader.biBitCount = 32;
            src_bmi.bmiHeader.biCompression = BI_RGB as u32;

            StretchDIBits(
                hdc_mem,
                draw_x, draw_y, draw_w, draw_h,
                frame.x as i32, frame.y as i32, frame.w as i32, frame.h as i32,
                bgra.as_ptr() as *const _,
                &src_bmi,
                DIB_RGB_COLORS,
                SRCCOPY,
            );

            // Blit off-screen to screen
            BitBlt(hdc, 0, 0, pane_w, pane_h, hdc_mem, 0, 0, SRCCOPY);

            // Deselect hbmp before deleting; deleting a selected object is a no-op (GDI leak).
            SelectObject(hdc_mem, old_bmp as *mut _);
            DeleteObject(hbmp as *mut _);
            DeleteDC(hdc_mem);
            EndPaint(hwnd, &ps);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
```

- [ ] **Step 15.2: Add WM_TIMER to config_wnd_proc**

`AnimationState::tick` takes `&mut self` (mutable) and `&SpriteSheet` (immutable). Both live inside `DialogCtx`, so a naive `ctx.preview_anim.tick(&ctx.preview_sheet.unwrap(), 100)` violates the borrow checker. Resolve this by using raw pointer arithmetic to split the borrow:

```rust
WM_TIMER => {
    if wparam == TIMER_ANIM {
        let ctx = get_ctx(hwnd);
        if !ctx.is_null() {
            let ctx = &mut *ctx;
            if let Some(sheet) = ctx.preview_sheet.as_ref() {
                // SAFETY: preview_anim and preview_sheet are disjoint fields of DialogCtx.
                // We read sheet immutably while ticking anim mutably.
                let anim = &mut ctx.preview_anim as *mut AnimationState;
                (*anim).tick(sheet, 100);
            }
            if !ctx.preview_hwnd.is_null() {
                InvalidateRect(ctx.preview_hwnd, std::ptr::null(), 0);
                UpdateWindow(ctx.preview_hwnd);
            }
        }
    }
    0
}
```

---

### Task 16: WM_COMMAND — wire all handlers

**Files:**
- Modify: `src/tray/config_window.rs`

- [ ] **Step 16.1: Add WM_COMMAND match arm to config_wnd_proc**

In `config_wnd_proc`, add this match arm (notify code is in HIWORD of wparam; control ID in LOWORD):

```rust
WM_COMMAND => {
    let id     = (wparam & 0xFFFF) as i32;
    let notify = ((wparam >> 16) & 0xFFFF) as u16;
    let ctx = get_ctx(hwnd);
    if !ctx.is_null() {
        handle_command(hwnd, id, notify, &mut *ctx);
    }
    0
}
```

- [ ] **Step 16.2: Write complete `handle_command`**

```rust
unsafe fn handle_command(hwnd: HWND, id: i32, notify: u16, ctx: &mut DialogCtx) {
    match id {
        DLG_OK => {
            read_fields(hwnd, &mut ctx.state);
            ctx.state.accept();
            DestroyWindow(hwnd);
        }
        DLG_CANCEL => {
            ctx.state.cancel();
            DestroyWindow(hwnd);
        }
        ID_BTN_ADD_PET => {
            read_fields(hwnd, &mut ctx.state);
            ctx.state.add_pet();
            refresh_pet_chips(hwnd, ctx);
            refresh_fields(hwnd, &ctx.state);
        }
        ID_BTN_REMOVE_PET => {
            ctx.state.remove_selected();
            refresh_pet_chips(hwnd, ctx);
            refresh_fields(hwnd, &ctx.state);
        }
        ID_LIST_GALLERY => {
            if notify == LBN_SELCHANGE as u16 {
                let lb = GetDlgItem(hwnd, ID_LIST_GALLERY);
                let sel = SendMessageW(lb, LB_GETCURSEL, 0, 0) as usize;
                if sel < ctx.gallery.entries.len() {
                    // Real gallery entry selected
                    let key = ctx.gallery.entries[sel].key.clone();
                    ctx.state.select_sprite(key);
                    load_preview_for_sprite(ctx);
                    refresh_fields(hwnd, &ctx.state);
                } else if sel == ctx.gallery.entries.len() {
                    // Browse sentinel clicked
                    if browse_and_install(hwnd, ctx).is_some() {
                        let new_idx = ctx.gallery.entries.len() - 1;
                        populate_gallery_listbox(hwnd, &ctx.gallery);
                        let lb = GetDlgItem(hwnd, ID_LIST_GALLERY);
                        SendMessageW(lb, LB_SETCURSEL, new_idx, 0);
                        let key = ctx.gallery.entries[new_idx].key.clone();
                        ctx.state.select_sprite(key);
                        load_preview_for_sprite(ctx);
                        refresh_fields(hwnd, &ctx.state);
                    }
                }
            }
        }
        id if id >= 2000 => {
            // Chip selection
            let pet_idx = (id - 2000) as usize;
            ctx.state.select(pet_idx);
            // Update selected_sprite from new pet's sheet_path
            if let Some(pet) = ctx.state.selected_pet() {
                ctx.state.selected_sprite = SpriteKey::from_sheet_path(&pet.sheet_path.clone());
            }
            refresh_fields(hwnd, &ctx.state);
            sync_gallery_selection(hwnd, ctx);
            load_preview_for_sprite(ctx);
            for chip in &ctx.chip_hwnds {
                InvalidateRect(*chip, std::ptr::null(), 1);
            }
        }
        _ => {}
    }
}
```

- [ ] **Step 16.3: Add `#[derive(Clone)]` to `GalleryEntry` in `src/window/sprite_gallery.rs`**

`browse_and_install` calls `ctx.gallery.entries.last().cloned()`, which requires `GalleryEntry: Clone`. `HBITMAP` is a raw pointer — cloning it copies the pointer value, which is safe here because freshly-installed entries always have `thumbnail = None`.

Change the struct declaration from:
```rust
pub struct GalleryEntry {
```
to:
```rust
#[derive(Clone)]
pub struct GalleryEntry {
```

- [ ] **Step 16.4: Write `browse_and_install`**

```rust
/// Opens a file dialog, installs the chosen sprite, appends it to gallery.
/// Returns Some(()) on success or None on cancel/error.
unsafe fn browse_and_install(hwnd: HWND, ctx: &mut DialogCtx) -> Option<GalleryEntry> {
    let mut buf = [0u16; 512];
    let mut filter: Vec<u16> = Vec::new();
    for chunk in &["Aseprite JSON (*.json)", "*.json", "All Files (*.*)", "*.*"] {
        filter.extend(chunk.encode_utf16());
        filter.push(0);
    }
    filter.push(0);

    let mut ofn: OPENFILENAMEW = std::mem::zeroed();
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = hwnd;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = buf.as_mut_ptr();
    ofn.nMaxFile = buf.len() as u32;
    ofn.Flags = 0x00001000 | 0x00000800; // OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST

    if GetOpenFileNameW(&mut ofn) == 0 {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let path_str = String::from_utf16_lossy(&buf[..end]);
    let path = std::path::Path::new(&path_str);

    match SpriteGallery::install(path) {
        Ok(entry) => {
            ctx.gallery.entries.push(entry);
            ctx.gallery.entries.last().cloned()
        }
        Err(e) => {
            // Show error in message box
            let msg = wide(&format!("Failed to install sprite:\n{e}"));
            let title = wide("Install Error");
            MessageBoxW(hwnd, msg.as_ptr(), title.as_ptr(), MB_ICONERROR | MB_OK);
            None
        }
    }
}
```

**Note:** `GalleryEntry` doesn't derive `Clone`. Add `#[derive(Clone)]` to `GalleryEntry` in `sprite_gallery.rs` if not already present. Actually, since `HBITMAP` is a raw pointer, derive Clone is fine — thumbnails start as None for new installs.

Update `src/window/sprite_gallery.rs`: add `#[derive(Clone)]` attribute to `GalleryEntry`:
```rust
#[derive(Clone)]
pub struct GalleryEntry { ... }
```

---

### Task 17: WM_DESTROY + WM_CLOSE cleanup

**Files:**
- Modify: `src/tray/config_window.rs`

- [ ] **Step 17.1: Add WM_DESTROY and WM_CLOSE handlers**

```rust
WM_CLOSE => {
    let ctx = get_ctx(hwnd);
    if !ctx.is_null() {
        (*ctx).state.cancel();
    }
    DestroyWindow(hwnd);
    0
}
WM_DESTROY => {
    let ctx = get_ctx(hwnd);
    if !ctx.is_null() {
        KillTimer(hwnd, TIMER_ANIM);
        (*ctx).gallery.destroy_thumbnails();
        (*ctx).destroy_brushes();
        // Clear GWLP_USERDATA so stray messages don't use the pointer after this.
        // IMPORTANT: do NOT call Box::from_raw here. The Box<DialogCtx> is owned
        // by show_config_dialog's stack frame (`let mut ctx = DialogCtx::new(...)`)
        // and will be dropped automatically when show_config_dialog returns after
        // the modal message loop exits. Calling Box::from_raw here would cause a
        // double-free.
        //
        // Lifetime note: DestroyWindow on the Win32 calling thread delivers WM_DESTROY
        // synchronously (the handler runs to completion before DestroyWindow returns).
        // Therefore ctx is still alive on the stack when this handler executes, and
        // the raw pointer dereference above is safe.
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
    }
    0
}
```

---

### Task 18: `refresh_fields` and `read_fields` — final versions

**Files:**
- Modify: `src/tray/config_window.rs`

- [ ] **Step 18.1: Write final `refresh_fields`**

```rust
unsafe fn refresh_fields(hwnd: HWND, state: &ConfigDialogState) {
    if let Some(pet) = state.selected_pet() {
        set_ctrl_text(hwnd, ID_EDIT_SCALE, &pet.scale.to_string());
        set_ctrl_text(hwnd, ID_EDIT_X, &pet.x.to_string());
        set_ctrl_text(hwnd, ID_EDIT_Y, &pet.y.to_string());
        set_ctrl_text(hwnd, ID_EDIT_SPEED, &pet.walk_speed.to_string());
    }
}
```

- [ ] **Step 18.2: Write final `read_fields`**

```rust
unsafe fn read_fields(hwnd: HWND, state: &mut ConfigDialogState) {
    let mut buf = [0u16; 512];
    macro_rules! get_text {
        ($id:expr) => {{
            let n = GetWindowTextW(GetDlgItem(hwnd, $id), buf.as_mut_ptr(), buf.len() as i32);
            String::from_utf16_lossy(&buf[..n.max(0) as usize])
        }};
    }
    state.update_scale(&get_text!(ID_EDIT_SCALE));
    state.update_x(&get_text!(ID_EDIT_X));
    state.update_y(&get_text!(ID_EDIT_Y));
    state.update_walk_speed(&get_text!(ID_EDIT_SPEED));
}
```

---

### Task 19: Final build, test, commit

- [ ] **Step 19.1: Build release**

Run: `cargo build 2>&1 | tail -20`
Expected: no errors (warnings about unused variables are acceptable)

- [ ] **Step 19.2: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: all tests pass — existing config_dialog_e2e tests continue to work via re-exports

- [ ] **Step 19.3: Smoke-test the dialog manually**

Run: `cargo run`
- Right-click tray icon → Configure
- Verify: dark dialog opens at 560×440
- Verify: sprite gallery shows eSheep as built-in entry
- Verify: clicking eSheep starts animated preview
- Verify: Scale/X/Y/Speed fields are editable
- Verify: Save returns updated config; Cancel discards

- [ ] **Step 19.4: Commit**

```bash
git add src/tray/config_window.rs src/window/sprite_gallery.rs
git commit -m "feat: complete config dialog redesign — dark theme, sprite gallery, animated preview"
```
