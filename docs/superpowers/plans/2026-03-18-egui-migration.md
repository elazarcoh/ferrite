# egui Migration Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hand-rolled Win32 config dialog and sprite editor with egui deferred viewports, migrating the event loop from Win32 `PeekMessageW` to `eframe::run_native`.

**Architecture:** `eframe::run_native` hosts a hidden main window that drives `App::update()` at 16 ms. Config dialog and sprite editor open as deferred viewports (`ctx.show_viewport_deferred`). Pet windows stay raw Win32 with `UpdateLayeredWindow`; winit's pump dispatches their messages automatically.

**Tech Stack:** eframe 0.33 (wgpu backend), egui (transitive), rfd 0.17 (file dialogs), existing crossbeam channel for events.

**Spec:** `docs/superpowers/specs/2026-03-17-egui-migration.md`

---

## Chunk 1: Foundation — Deps + Core Migration

### Task 1: Add eframe and rfd to Cargo.toml

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add dependencies**

In `Cargo.toml`, add these lines to `[dependencies]` after `env_logger = "0.11"`:

```toml
eframe = { version = "0.33", default-features = false, features = ["default_fonts", "wgpu"] }
rfd = "0.17"
```

- [ ] **Step 2: Verify the dependencies resolve**

```bash
cargo fetch
```

Expected: downloads eframe, egui, wgpu, rfd and their transitive deps without error.

- [ ] **Step 3: Verify build still compiles (no code changes yet)**

```bash
cargo build 2>&1 | tail -5
```

Expected: compiles successfully (eframe is downloaded but not used yet — that's fine).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add eframe and rfd dependencies"
```

---

### Task 2: Core Migration — SpriteKey, deletions, stub UIs, new event loop

This task is large but must be done atomically: deleting `dialog_state.rs` requires
`config_window.rs` to be rewritten in the same commit, and `app.rs` must use the new
APIs at the same time.

**Files:**
- Modify: `src/window/sprite_gallery.rs` — add SpriteKey, remove Win32 GDI code
- Modify: `src/config/mod.rs` — remove `pub mod dialog_state`
- Delete: `src/config/dialog_state.rs`
- Rewrite: `src/tray/config_window.rs` — stub egui implementation
- Rewrite: `src/tray/sprite_editor.rs` — stub egui implementation
- Rewrite: `src/app.rs` — implement `eframe::App`
- Rewrite: `src/main.rs` — call `eframe::run_native`
- Delete: `tests/e2e/test_config_dialog_e2e.rs`
- Modify: `tests/e2e.rs` — remove `mod config_dialog_e2e`

#### Step 1: Move SpriteKey into sprite_gallery.rs

`src/window/sprite_gallery.rs` currently imports `SpriteKey` from `dialog_state`. We're
moving the definition here.

Replace the top of `src/window/sprite_gallery.rs` — remove the `use crate::config::dialog_state::SpriteKey;` import and add the `SpriteKey` type definition (copy from `dialog_state.rs`) before `SourceKind`:

```rust
//! Gallery discovery and custom sprite install.
//! `SpriteKey` is defined here and used by both the gallery and the config window.

use crate::assets::{self, Assets};
use crate::sprite::sheet::load_embedded;
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

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
    pub fn to_sheet_path(&self) -> String {
        match self {
            SpriteKey::Embedded(stem) => format!("embedded://{stem}"),
            SpriteKey::Installed(p)   => p.to_string_lossy().into_owned(),
        }
    }

    pub fn from_sheet_path(s: &str) -> Self {
        if let Some(stem) = s.strip_prefix("embedded://") {
            SpriteKey::Embedded(stem.to_string())
        } else {
            SpriteKey::Installed(PathBuf::from(s))
        }
    }
}
```

- [ ] Remove `#[cfg(target_os = "windows")] use windows_sys::Win32::Graphics::Gdi::{...};` block (the whole Win32 GDI import block used only for thumbnails).
- [ ] Remove the `thumbnail` field from `GalleryEntry`:

```rust
#[derive(Debug, Clone)]
pub struct GalleryEntry {
    pub key: SpriteKey,
    pub display_name: String,
    pub source: SourceKind,
}
```

- [ ] Delete the `load_thumbnail` method (lines ~177–271) and `destroy_thumbnails` method (lines ~272–285) from `impl SpriteGallery`. These used Win32 GDI and are no longer needed.
- [ ] Delete `pub struct BrowseEntry;` (no longer used).
- [ ] Run `cargo check 2>&1 | grep "^error"` — fix any remaining compile errors in sprite_gallery.rs.

#### Step 2: Update config/mod.rs

In `src/config/mod.rs`, remove the line:
```rust
pub mod dialog_state;
```

#### Step 3: Delete dialog_state.rs

```bash
rm src/config/dialog_state.rs
```

#### Step 4: Delete e2e tests for ConfigDialogState

```bash
rm tests/e2e/test_config_dialog_e2e.rs
```

In `tests/e2e.rs`, remove these lines:
```rust
mod config_dialog_e2e {
    include!("e2e/test_config_dialog_e2e.rs");
}
```

#### Step 5: Write stub config_window.rs

Replace the entire `src/tray/config_window.rs` (all 1053 lines) with this stub.
The stub defines the types and function signatures that `app.rs` needs, with placeholder UI bodies.
The real implementation comes in Task 3.

```rust
//! Config dialog — egui deferred viewport.
//! Full implementation: Task 3.

use crate::config::schema::{Config, PetConfig};
use crate::event::AppEvent;
use crate::window::sprite_gallery::SpriteGallery;
use crossbeam_channel::Sender;
use std::sync::{Arc, Mutex};

pub struct ConfigWindowState {
    pub config: Config,
    pub selected_pet_idx: Option<usize>,
    pub gallery: SpriteGallery,
    pub tx: Sender<AppEvent>,
    pub should_close: bool,
    /// Set by the "Edit…" / "New from PNG…" buttons; consumed by App::update.
    pub open_editor_request: Option<Box<crate::tray::sprite_editor::SpriteEditorViewport>>,
}

impl ConfigWindowState {
    pub fn new(config: Config, tx: Sender<AppEvent>) -> Self {
        let selected_pet_idx = if config.pets.is_empty() { None } else { Some(0) };
        let gallery = SpriteGallery::load();
        ConfigWindowState {
            config,
            selected_pet_idx,
            gallery,
            tx,
            should_close: false,
            open_editor_request: None,
        }
    }
}

pub fn open_config_viewport(ctx: &egui::Context, state: Arc<Mutex<ConfigWindowState>>) {
    ctx.show_viewport_deferred(
        egui::ViewportId::from_hash_of("config_dialog"),
        egui::ViewportBuilder::default()
            .with_title("Configure My Pet")
            .with_inner_size([600.0, 480.0]),
        move |ctx, _class| {
            let mut s = state.lock().unwrap();
            draw_config_viewport(ctx, &mut s);
        },
    );
}

pub fn draw_config_viewport(ctx: &egui::Context, state: &mut ConfigWindowState) {
    if ctx.input(|i| i.viewport().close_requested()) {
        state.should_close = true;
        return;
    }
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.label("Config dialog — coming in Task 3");
        if ui.button("Close").clicked() {
            state.should_close = true;
        }
    });
}
```

#### Step 6: Write stub sprite_editor.rs

Replace the entire `src/tray/sprite_editor.rs` (all 814 lines) with this stub:

```rust
//! Sprite editor — egui deferred viewport.
//! Full implementation: Task 4.

use crate::sprite::animation::AnimationState;
use crate::sprite::editor_state::SpriteEditorState;
use crate::sprite::sheet::SpriteSheet;
use std::sync::{Arc, Mutex};

pub struct SpriteEditorViewport {
    pub state: SpriteEditorState,
    /// Cached egui texture; uploaded on first frame.
    pub texture: Option<egui::TextureHandle>,
    pub anim: AnimationState,
    pub preview_sheet: Option<SpriteSheet>,
    pub should_close: bool,
}

impl SpriteEditorViewport {
    pub fn new(state: SpriteEditorState) -> Self {
        let tag_name = state.tags.first()
            .map(|t| t.name.clone())
            .unwrap_or_default();
        SpriteEditorViewport {
            state,
            texture: None,
            anim: AnimationState::new(tag_name),
            preview_sheet: None,
            should_close: false,
        }
    }
}

pub fn open_sprite_editor_viewport(ctx: &egui::Context, state: Arc<Mutex<SpriteEditorViewport>>) {
    ctx.show_viewport_deferred(
        egui::ViewportId::from_hash_of("sprite_editor"),
        egui::ViewportBuilder::default()
            .with_title("Sprite Editor")
            .with_inner_size([900.0, 600.0]),
        move |ctx, _class| {
            let mut vp = state.lock().unwrap();
            draw_sprite_editor_viewport(ctx, &mut vp);
        },
    );
}

pub fn draw_sprite_editor_viewport(ctx: &egui::Context, vp: &mut SpriteEditorViewport) {
    if ctx.input(|i| i.viewport().close_requested()) {
        vp.should_close = true;
        return;
    }
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.label("Sprite editor — coming in Task 4");
        if ui.button("Close").clicked() {
            vp.should_close = true;
        }
    });
}
```

#### Step 7: Rewrite main.rs

Replace `src/main.rs` entirely:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assets;
mod config;
mod event;
mod sprite;
mod tray;
mod window;

fn main() -> anyhow::Result<()> {
    #[cfg(debug_assertions)]
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Debug)
        .init();
    #[cfg(not(debug_assertions))]
    env_logger::init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_visible(false)
            .with_taskbar(false),
        ..Default::default()
    };
    eframe::run_native(
        "my-pet",
        native_options,
        Box::new(|_cc| Ok(Box::new(app::App::new()?))),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {e}"))
}
```

#### Step 8: Rewrite app.rs

Replace `src/app.rs` entirely. Key changes from the old version:
- Remove `timer_id: usize` and `config_dialog_hwnd: HWND` fields.
- Add `config_window_state` and `sprite_editor_state` fields.
- Remove `pub fn run(&mut self)` (eframe drives the loop).
- Add `impl eframe::App for App`.
- `TrayOpenConfig` now creates `ConfigWindowState` instead of calling `show_config_dialog`.
- `Quit`/`TrayQuit` now uses `ctx.send_viewport_cmd` (only accessible in `update`; use a flag).

```rust
use crate::{
    assets,
    config::{self, schema::PetConfig, watcher::spawn_watcher},
    event::AppEvent,
    sprite::{
        animation::AnimationState,
        behavior::{BehaviorAi, BehaviorState, Facing},
        sheet::{self, SpriteSheet},
    },
    tray::{
        SystemTray,
        config_window::{ConfigWindowState, open_config_viewport},
        sprite_editor::{SpriteEditorViewport, open_sprite_editor_viewport},
    },
    window::pet_window::PetWindow,
};
use anyhow::{Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use windows_sys::Win32::{
    Foundation::RECT,
    UI::WindowsAndMessaging::*,
};

// ─── PetInstance (unchanged from before) ──────────────────────────────────────

pub struct PetInstance {
    pub cfg: PetConfig,
    pub sheet: SpriteSheet,
    pub window: PetWindow,
    pub anim: AnimationState,
    pub ai: BehaviorAi,
    pub x: i32,
    pub y: i32,
    elevated_ms: u32,
}

// ... (copy PetInstance impl, new, tick, render_current_frame, and Drop unchanged) ...

// ─── App ─────────────────────────────────────────────────────────────────────

pub struct App {
    tx: Sender<AppEvent>,
    rx: Receiver<AppEvent>,
    pets: HashMap<String, PetInstance>,
    _tray: SystemTray,
    _watcher: notify::RecommendedWatcher,
    last_tick_ms: Instant,
    config_window_state: Option<Arc<Mutex<ConfigWindowState>>>,
    sprite_editor_state: Option<Arc<Mutex<SpriteEditorViewport>>>,
    should_quit: bool,
}

impl App {
    pub fn new() -> Result<Self> {
        let (tx, rx) = bounded::<AppEvent>(256);
        crate::window::wndproc::init_event_sender(tx.clone());

        let cfg_path = config::config_path();
        let cfg = config::load(&cfg_path).unwrap_or_default();

        let mut pets = HashMap::new();
        for pet_cfg in &cfg.pets {
            match build_pet(pet_cfg) {
                Ok(inst) => { pets.insert(pet_cfg.id.clone(), inst); }
                Err(e) => log::warn!("failed to create pet '{}': {e}", pet_cfg.id),
            }
        }

        let tray = SystemTray::new(tx.clone()).context("create tray")?;
        let watcher = spawn_watcher(cfg_path, tx.clone()).context("create watcher")?;

        Ok(App {
            tx,
            rx,
            pets,
            _tray: tray,
            _watcher: watcher,
            last_tick_ms: Instant::now(),
            config_window_state: None,
            sprite_editor_state: None,
            should_quit: false,
        })
    }

    fn handle_event(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Quit | AppEvent::TrayQuit => {
                self.should_quit = true;
            }
            AppEvent::TrayAddPet => {
                let id = format!("pet_{}", self.pets.len());
                let cfg = PetConfig { id: id.clone(), ..PetConfig::default() };
                match build_pet(&cfg) {
                    Ok(inst) => { self.pets.insert(id, inst); }
                    Err(e) => log::warn!("add pet failed: {e}"),
                }
            }
            AppEvent::TrayRemovePet { pet_id } => {
                self.pets.remove(&pet_id);
            }
            AppEvent::TrayOpenConfig => {
                if self.config_window_state.is_none() {
                    let current = config::load(&config::config_path()).unwrap_or_default();
                    self.config_window_state = Some(Arc::new(Mutex::new(
                        ConfigWindowState::new(current, self.tx.clone()),
                    )));
                }
                // If already open, the deferred viewport is already shown; no action needed.
            }
            AppEvent::ConfigReloaded(new_cfg) => {
                if let Err(e) = self.apply_config(new_cfg) {
                    log::warn!("apply_config: {e}");
                }
            }
            AppEvent::ConfigChanged(cfg) => {
                if let Err(e) = config::save(&config::config_path(), &cfg) {
                    log::warn!("auto-save config failed: {e}");
                }
                if let Err(e) = self.apply_config(cfg) {
                    log::warn!("apply_config: {e}");
                }
            }
            AppEvent::PetClicked { pet_id } => {
                if let Some(p) = self.pets.get_mut(&pet_id) {
                    if matches!(p.ai.state, BehaviorState::Sleep) {
                        p.ai.wake();
                    } else {
                        p.ai.pet();
                    }
                }
            }
            AppEvent::PetDragStart { pet_id, cursor_x, cursor_y } => {
                if let Some(p) = self.pets.get_mut(&pet_id) {
                    p.ai.grab((cursor_x - p.x, cursor_y - p.y));
                }
            }
            AppEvent::PetDragEnd { pet_id, velocity } => {
                if let Some(p) = self.pets.get_mut(&pet_id) {
                    p.ai.release(velocity);
                }
            }
            AppEvent::Tick(_) => {}
        }
    }

    fn apply_config(&mut self, new_cfg: crate::config::schema::Config) -> Result<()> {
        let new_ids: std::collections::HashSet<_> =
            new_cfg.pets.iter().map(|p| p.id.clone()).collect();
        self.pets.retain(|id, _| new_ids.contains(id));
        for pet_cfg in new_cfg.pets {
            let needs_rebuild = self.pets.get(&pet_cfg.id)
                .map(|inst| inst.cfg != pet_cfg)
                .unwrap_or(true);
            if needs_rebuild {
                match build_pet(&pet_cfg) {
                    Ok(inst) => { self.pets.insert(pet_cfg.id.clone(), inst); }
                    Err(e) => log::warn!("reload pet '{}': {e}", pet_cfg.id),
                }
            }
        }
        Ok(())
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain channel.
        while let Ok(ev) = self.rx.try_recv() {
            self.handle_event(ev);
        }

        // Quit.
        if self.should_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // Tick pets.
        let now = Instant::now();
        let delta_ms = now.duration_since(self.last_tick_ms)
            .as_millis().min(200) as u32;
        self.last_tick_ms = now;
        for pet in self.pets.values_mut() {
            if let Err(e) = pet.tick(delta_ms) {
                log::warn!("pet tick: {e}");
            }
        }

        // Config viewport.
        if let Some(state) = self.config_window_state.clone() {
            open_config_viewport(ctx, state.clone());
            // Consume editor-open requests from the config dialog.
            if let Ok(mut s) = state.try_lock() {
                if let Some(vp) = s.open_editor_request.take() {
                    self.sprite_editor_state = Some(Arc::new(Mutex::new(*vp)));
                }
                if s.should_close {
                    self.config_window_state = None;
                }
            }
        }

        // Sprite editor viewport.
        if let Some(state) = self.sprite_editor_state.clone() {
            open_sprite_editor_viewport(ctx, state.clone());
            if let Ok(vp) = state.try_lock() {
                if vp.should_close {
                    drop(vp);
                    self.sprite_editor_state = None;
                }
            }
        }

        // Schedule next tick at 16 ms.
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn build_pet(cfg: &PetConfig) -> Result<PetInstance> {
    let sheet = load_sheet(&cfg.sheet_path)?;
    PetInstance::new(cfg.clone(), sheet)
}

fn load_sheet(path: &str) -> Result<SpriteSheet> {
    if let Some(stem) = path.strip_prefix("embedded://") {
        let (json, png) = assets::embedded_sheet(stem)
            .with_context(|| format!("embedded sheet '{stem}' not found"))?;
        return sheet::load_embedded(&json, &png);
    }
    let json = std::fs::read(path).with_context(|| format!("read {path}"))?;
    let json_path = std::path::Path::new(path);
    let png_path = json_path.with_extension("png");
    let png = std::fs::read(&png_path)
        .with_context(|| format!("read {}", png_path.display()))?;
    let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .context("decode PNG")?
        .into_rgba8();
    sheet::SpriteSheet::from_json_and_image(&json, image)
}
```

**Important:** Copy the `PetInstance` struct definition and all its methods (`new`, `tick`,
`render_current_frame`, `window_frame_buf_is_empty`, `window_frame_buf`, `window_width`)
from the old `src/app.rs` verbatim — they are unchanged.

- [ ] **Step 9: Build and fix errors**

```bash
cargo build 2>&1 | grep "^error"
```

Common issues and fixes:
- Missing `eframe`/`egui` in scope: add `use eframe; use egui;` at the crate root or use the full path. Actually `eframe` and `egui` are external crates — they're already in scope via Cargo.toml; just use them as `eframe::...` and `egui::...`.
- `SpriteGallery::load()` — ensure this method still exists after removing thumbnail code.
- Leftover `use windows_sys::...` in app.rs from old code — remove any that are now unused. The `RECT` and `GetWindowRect` etc. are still needed inside `PetInstance::tick`, so keep those imports there.
- `AnimationState::new` — check its signature in `src/sprite/animation.rs`.

- [ ] **Step 10: Run tests**

```bash
cargo test 2>&1 | tail -20
```

Expected: 130 tests pass (156 − 26 deleted config dialog e2e tests).
If some tests fail, read the failure messages and fix the root cause.

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "feat: migrate event loop to eframe; stub egui config and sprite editor"
```

---

## Chunk 2: Config Dialog egui Implementation

### Task 3: Full egui config dialog

Replace the stub `draw_config_viewport` / `draw_config_viewport` with a real egui
two-column layout as described in the spec.

**Files:**
- Rewrite: `src/tray/config_window.rs`

Keep `ConfigWindowState`, `open_config_viewport`, and the function signatures identical.
Only `draw_config_viewport` gets a real implementation.

- [ ] **Step 1: Prerequisites — make TAG_COLORS pub and add load_sheet_for_config**

In `src/sprite/editor_state.rs`, make `TAG_COLORS` public (it's currently private):
```rust
// Change:
const TAG_COLORS: &[u32] = &[ ... ];
// To:
pub const TAG_COLORS: &[u32] = &[ ... ];
```

In `src/app.rs`, add this public wrapper at the bottom of the file so config_window.rs can
call it without depending on private helpers:
```rust
pub fn load_sheet_for_config(path: &str) -> anyhow::Result<crate::sprite::sheet::SpriteSheet> {
    load_sheet(path)
}
```

In `src/sprite/sheet.rs`, add `Default`, `serde::Serialize`, `serde::Deserialize` derives
to `TagDirection`, and add a `.label()` method:
```rust
// Change:
#[derive(Debug, Clone, PartialEq)]
pub enum TagDirection { Forward, Reverse, PingPong, PingPongReverse }
// To:
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum TagDirection {
    #[default] Forward,
    Reverse,
    PingPong,
    PingPongReverse,
}
impl TagDirection {
    pub fn label(&self) -> &'static str {
        match self {
            TagDirection::Forward         => "Forward",
            TagDirection::Reverse         => "Reverse",
            TagDirection::PingPong        => "PingPong",
            TagDirection::PingPongReverse => "PingPongReverse",
        }
    }
}
```

Run `cargo build` and verify it still compiles with these three changes.

- [ ] **Step 2: Add necessary imports at the top of config_window.rs**

```rust
use crate::config::schema::{Config, PetConfig};
use crate::event::AppEvent;
use crate::sprite::behavior::AnimTagMap;
use crate::sprite::sheet::SpriteSheet;
use crate::window::sprite_gallery::{SpriteGallery, SpriteKey};
use crossbeam_channel::Sender;
use std::sync::{Arc, Mutex};
// rfd is an external crate — use it as rfd::FileDialog directly (no use statement needed)
```

Also add `loaded_sheet: Option<SpriteSheet>` to `ConfigWindowState` (caches the sheet for
the tag-map combo boxes; reloaded when `sheet_path` changes).

**Replace** the entire `ConfigWindowState` struct definition and `impl ConfigWindowState` in
`config_window.rs` with:

```rust
pub struct ConfigWindowState {
    pub config: Config,
    pub selected_pet_idx: Option<usize>,
    pub gallery: SpriteGallery,
    pub tx: Sender<AppEvent>,
    pub should_close: bool,
    pub open_editor_request: Option<Box<crate::tray::sprite_editor::SpriteEditorViewport>>,
    /// Cached sprite sheet for the selected pet — used to populate tag-map combo boxes.
    pub loaded_sheet: Option<SpriteSheet>,
    /// The sheet_path last loaded into `loaded_sheet`; detect changes.
    pub loaded_sheet_path: String,
}

impl ConfigWindowState {
    pub fn new(config: Config, tx: Sender<AppEvent>) -> Self {
        let selected_pet_idx = if config.pets.is_empty() { None } else { Some(0) };
        let gallery = SpriteGallery::load();
        ConfigWindowState {
            config,
            selected_pet_idx,
            gallery,
            tx,
            should_close: false,
            open_editor_request: None,
            loaded_sheet: None,
            loaded_sheet_path: String::new(),
        }
    }
}
```

- [ ] **Step 3: Implement draw_config_viewport**

Replace the stub body. The full implementation uses `ui.columns(2, |cols| { ... })`.

Left column: pet list + Add/Remove/Edit/New buttons:

```rust
pub fn draw_config_viewport(ctx: &egui::Context, state: &mut ConfigWindowState) {
    if ctx.input(|i| i.viewport().close_requested()) {
        state.should_close = true;
        return;
    }

    // Reload sheet if selected pet's path changed.
    reload_sheet_if_needed(state);

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.columns(2, |cols| {
            draw_pet_list(&mut cols[0], state);
            draw_pet_settings(&mut cols[1], state);
        });
    });
}

fn reload_sheet_if_needed(state: &mut ConfigWindowState) {
    let path = state.selected_pet_idx
        .and_then(|i| state.config.pets.get(i))
        .map(|p| p.sheet_path.clone())
        .unwrap_or_default();
    if path != state.loaded_sheet_path {
        state.loaded_sheet_path = path.clone();
        state.loaded_sheet = crate::app::load_sheet_for_config(&path).ok();
    }
}
```

- [ ] **Step 4: Implement draw_pet_list**

```rust
fn draw_pet_list(ui: &mut egui::Ui, state: &mut ConfigWindowState) {
    ui.heading("Pets");
    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
        let len = state.config.pets.len();
        for i in 0..len {
            let id = state.config.pets[i].id.clone();
            let selected = state.selected_pet_idx == Some(i);
            if ui.selectable_label(selected, &id).clicked() {
                state.selected_pet_idx = Some(i);
            }
        }
    });

    ui.horizontal(|ui| {
        if ui.button("Add Pet").clicked() {
            let n = state.config.pets.len();
            state.config.pets.push(PetConfig {
                id: format!("pet_{n}"),
                ..PetConfig::default()
            });
            state.selected_pet_idx = Some(state.config.pets.len() - 1);
            state.tx.send(AppEvent::ConfigChanged(state.config.clone())).ok();
        }
        let can_remove = state.selected_pet_idx.is_some();
        if ui.add_enabled(can_remove, egui::Button::new("Remove")).clicked() {
            if let Some(idx) = state.selected_pet_idx {
                state.config.pets.remove(idx);
                state.selected_pet_idx = if state.config.pets.is_empty() {
                    None
                } else {
                    Some(idx.min(state.config.pets.len() - 1))
                };
                state.tx.send(AppEvent::ConfigChanged(state.config.clone())).ok();
            }
        }
    });

    let has_sel = state.selected_pet_idx.is_some();
    ui.horizontal(|ui| {
        if ui.add_enabled(has_sel, egui::Button::new("Edit…")).clicked() {
            open_editor_for_selected(state, false);
        }
        if ui.button("New from PNG…").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("PNG image", &["png"])
                .pick_file()
            {
                let png_path = path;
                let editor_state = crate::sprite::editor_state::SpriteEditorState::new(
                    png_path.clone(),
                    image::open(&png_path).ok()
                        .map(|i| i.into_rgba8())
                        .unwrap_or_default(),
                );
                state.open_editor_request = Some(Box::new(
                    crate::tray::sprite_editor::SpriteEditorViewport::new(editor_state),
                ));
            }
        }
    });
}

fn open_editor_for_selected(state: &mut ConfigWindowState, _force_copy: bool) {
    let Some(idx) = state.selected_pet_idx else { return };
    let path = state.config.pets[idx].sheet_path.clone();
    let (json_bytes, png_bytes) = if let Some(stem) = path.strip_prefix("embedded://") {
        match crate::assets::embedded_sheet(stem) {
            Some(p) => p,
            None => return,
        }
    } else {
        let json = match std::fs::read(&path) { Ok(b) => b, Err(_) => return };
        let png_path = std::path::Path::new(&path).with_extension("png");
        let png = match std::fs::read(&png_path) { Ok(b) => b, Err(_) => return };
        (json, png)
    };
    let image = match image::load_from_memory_with_format(&png_bytes, image::ImageFormat::Png) {
        Ok(i) => i.into_rgba8(),
        Err(_) => return,
    };
    // Determine png_path for save_to_dir
    let png_path = if path.starts_with("embedded://") {
        // Embedded: copy to AppData sprites dir
        let dir = crate::window::sprite_gallery::SpriteGallery::appdata_sprites_dir();
        let stem = path.strip_prefix("embedded://").unwrap_or("sprite");
        dir.join(format!("{stem}.png"))
    } else {
        std::path::Path::new(&path).with_extension("png").to_path_buf()
    };
    let mut editor_state = crate::sprite::editor_state::SpriteEditorState::new(png_path, image);
    // Pre-load tags from the JSON.
    if let Ok((sheet, tag_map_opt)) = crate::sprite::sheet::load_with_tag_map(&json_bytes, &png_bytes) {
        let w = editor_state.image.width();
        let h = editor_state.image.height();
        if editor_state.cols > 0 && editor_state.rows > 0 {
            let fw = w / editor_state.cols;
            let fh = h / editor_state.rows;
            if fw > 0 && fh > 0 {
                editor_state.cols = w / fw;
                editor_state.rows = h / fh;
            }
        }
        for (i, tag) in sheet.tags.iter().enumerate() {
            let color = crate::sprite::editor_state::TAG_COLORS[i % crate::sprite::editor_state::TAG_COLORS.len()];
            editor_state.tags.push(crate::sprite::editor_state::EditorTag {
                name: tag.name.clone(),
                from: tag.from,
                to: tag.to,
                direction: tag.direction.clone(),
                color,
            });
        }
        if let Some(tm) = tag_map_opt {
            editor_state.tag_map = tm;
        }
    }
    state.open_editor_request = Some(Box::new(
        crate::tray::sprite_editor::SpriteEditorViewport::new(editor_state),
    ));
}
```

- [ ] **Step 5: Implement draw_pet_settings**

```rust
fn draw_pet_settings(ui: &mut egui::Ui, state: &mut ConfigWindowState) {
    let Some(idx) = state.selected_pet_idx else {
        ui.label("Select a pet to configure.");
        return;
    };

    let mut changed = false;
    let pet = &mut state.config.pets[idx];

    ui.heading("Settings");

    // Sheet selector
    ui.horizontal(|ui| {
        ui.label("Sheet:");
        let current_key = SpriteKey::from_sheet_path(&pet.sheet_path);
        let current_name = state.gallery.entries.iter()
            .find(|e| e.key == current_key)
            .map(|e| e.display_name.clone())
            .unwrap_or_else(|| pet.sheet_path.clone());
        egui::ComboBox::from_id_salt("sheet")
            .selected_text(&current_name)
            .show_ui(ui, |ui| {
                for entry in &state.gallery.entries {
                    let sel = entry.key == current_key;
                    if ui.selectable_label(sel, &entry.display_name).clicked() && !sel {
                        pet.sheet_path = entry.key.to_sheet_path();
                        changed = true;
                    }
                }
            });
    });

    // Scale
    ui.horizontal(|ui| {
        ui.label("Scale:");
        changed |= ui.add(egui::DragValue::new(&mut pet.scale).range(1_u32..=4)).changed();
    });

    // Walk speed
    ui.horizontal(|ui| {
        ui.label("Walk speed:");
        changed |= ui.add(
            egui::DragValue::new(&mut pet.walk_speed)
                .range(1.0_f32..=500.0)
                .suffix(" px/s"),
        ).changed();
    });

    // Position
    ui.horizontal(|ui| {
        ui.label("X:");
        changed |= ui.add(egui::DragValue::new(&mut pet.x)).changed();
        ui.label("Y:");
        changed |= ui.add(egui::DragValue::new(&mut pet.y)).changed();
    });

    // Flip
    changed |= ui.checkbox(&mut pet.flip_walk_left, "Flip walk left").changed();

    // Tag map
    ui.separator();
    ui.label("Behavior → Tag:");
    let tag_names: Vec<String> = state.loaded_sheet.as_ref()
        .map(|s| s.tags.iter().map(|t| t.name.clone()).collect())
        .unwrap_or_default();

    draw_tag_map_row(ui, "idle (required)", &tag_names, &mut pet.tag_map.idle, true, &mut changed);
    draw_tag_map_row(ui, "walk (required)", &tag_names, &mut pet.tag_map.walk, true, &mut changed);

    let opt_fields: &[(&str, &mut Option<String>)] = &[
        ("run",     &mut pet.tag_map.run),
        ("sit",     &mut pet.tag_map.sit),
        ("sleep",   &mut pet.tag_map.sleep),
        ("wake",    &mut pet.tag_map.wake),
        ("grabbed", &mut pet.tag_map.grabbed),
        ("petted",  &mut pet.tag_map.petted),
        ("react",   &mut pet.tag_map.react),
        ("fall",    &mut pet.tag_map.fall),
        ("thrown",  &mut pet.tag_map.thrown),
    ];
    // Note: can't iterate &mut slice of tuples with mutable refs simultaneously,
    // so write each one out:
    macro_rules! opt_row {
        ($label:literal, $field:expr) => {
            draw_tag_map_row_opt(ui, $label, &tag_names, $field, &mut changed);
        };
    }
    opt_row!("run",     &mut pet.tag_map.run);
    opt_row!("sit",     &mut pet.tag_map.sit);
    opt_row!("sleep",   &mut pet.tag_map.sleep);
    opt_row!("wake",    &mut pet.tag_map.wake);
    opt_row!("grabbed", &mut pet.tag_map.grabbed);
    opt_row!("petted",  &mut pet.tag_map.petted);
    opt_row!("react",   &mut pet.tag_map.react);
    opt_row!("fall",    &mut pet.tag_map.fall);
    opt_row!("thrown",  &mut pet.tag_map.thrown);

    if changed {
        state.tx.send(AppEvent::ConfigChanged(state.config.clone())).ok();
    }
}

fn draw_tag_map_row(
    ui: &mut egui::Ui,
    label: &str,
    tag_names: &[String],
    value: &mut String,
    _required: bool,
    changed: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(label)
            .selected_text(if value.is_empty() { "— not set —" } else { value.as_str() })
            .show_ui(ui, |ui| {
                for name in tag_names {
                    if ui.selectable_label(value == name, name).clicked() {
                        *value = name.clone();
                        *changed = true;
                    }
                }
            });
    });
}

fn draw_tag_map_row_opt(
    ui: &mut egui::Ui,
    label: &str,
    tag_names: &[String],
    value: &mut Option<String>,
    changed: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        let current = value.as_deref().unwrap_or("— not set —");
        egui::ComboBox::from_id_salt(label)
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui.selectable_label(value.is_none(), "— not set —").clicked() {
                    *value = None;
                    *changed = true;
                }
                for name in tag_names {
                    let sel = value.as_deref() == Some(name.as_str());
                    if ui.selectable_label(sel, name).clicked() {
                        *value = Some(name.clone());
                        *changed = true;
                    }
                }
            });
    });
}
```

- [ ] **Step 6: Build**

```bash
cargo build 2>&1 | grep "^error"
```

Fix any compile errors (missing imports, wrong field names, etc.).

- [ ] **Step 7: Run tests**

```bash
cargo test 2>&1 | tail -10
```

Expected: 130 tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/tray/config_window.rs src/app.rs src/sprite/editor_state.rs src/sprite/sheet.rs
git commit -m "feat: implement egui config dialog"
```

---

## Chunk 3: Sprite Editor egui Implementation

### Task 4: Full egui sprite editor

Replace the stub `draw_sprite_editor_viewport` with a real egui two-column layout.
`SpriteEditorViewport` struct definition stays identical to the stub.

**Files:**
- Rewrite: `src/tray/sprite_editor.rs`

- [ ] **Step 1: Add imports**

```rust
use crate::config::schema::Config;
use crate::event::AppEvent;
use crate::sprite::animation::AnimationState;
use crate::sprite::behavior::{AnimTagMap, TagDirection};
use crate::sprite::editor_state::{EditorTag, SpriteEditorState, TAG_COLORS};
use crate::sprite::sheet::{load_with_tag_map, SpriteSheet};
use egui::{ColorImage, TextureHandle};
use image::RgbaImage;
use std::sync::{Arc, Mutex};
use std::time::Duration;
```

- [ ] **Step 2: Keep SpriteEditorViewport struct and open_sprite_editor_viewport unchanged from stub**

These are already correct. Only `draw_sprite_editor_viewport` changes.

- [ ] **Step 3: Implement draw_sprite_editor_viewport**

```rust
pub fn draw_sprite_editor_viewport(ctx: &egui::Context, vp: &mut SpriteEditorViewport) {
    if ctx.input(|i| i.viewport().close_requested()) {
        vp.should_close = true;
        return;
    }

    // Upload texture on first frame.
    if vp.texture.is_none() {
        vp.texture = Some(upload_texture(ctx, &vp.state.image));
    }

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.columns(2, |cols| {
            draw_canvas_column(&mut cols[0], vp);
            draw_controls_column(ctx, &mut cols[1], vp);
        });
    });

    // Live preview repaint: schedule after the current frame's duration.
    if let Some(sheet) = &vp.preview_sheet {
        let frame_idx = vp.anim.absolute_frame(sheet);
        let dur_ms = sheet.frames.get(frame_idx)
            .map(|f| f.duration_ms as u64)
            .unwrap_or(100);
        ctx.request_repaint_after(Duration::from_millis(dur_ms));
    }
}
```

- [ ] **Step 4: Implement upload_texture**

```rust
fn upload_texture(ctx: &egui::Context, image: &RgbaImage) -> TextureHandle {
    let size = [image.width() as usize, image.height() as usize];
    let pixels: Vec<egui::Color32> = image.pixels()
        .map(|p| egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
        .collect();
    ctx.load_texture(
        "spritesheet",
        ColorImage { size, pixels },
        egui::TextureOptions::NEAREST,
    )
}
```

- [ ] **Step 5: Implement draw_canvas_column**

```rust
fn draw_canvas_column(ui: &mut egui::Ui, vp: &mut SpriteEditorViewport) {
    let Some(tex) = &vp.texture else { return };

    let avail = ui.available_width();
    let img_w = vp.state.image.width() as f32;
    let img_h = vp.state.image.height() as f32;
    let scale = avail / img_w;
    let display_size = egui::vec2(avail, img_h * scale);

    let (rect, _) = ui.allocate_exact_size(display_size, egui::Sense::hover());
    let painter = ui.painter();

    // Draw spritesheet.
    painter.image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);

    // Draw grid + tag highlights.
    let cols = vp.state.cols.max(1);
    let rows = vp.state.rows.max(1);
    let cell_w = display_size.x / cols as f32;
    let cell_h = display_size.y / rows as f32;

    // Grid lines.
    let grid_color = egui::Color32::from_rgba_unmultiplied(200, 200, 200, 120);
    for c in 0..=cols {
        let x = rect.min.x + c as f32 * cell_w;
        painter.line_segment(
            [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
            egui::Stroke::new(1.0, grid_color),
        );
    }
    for r in 0..=rows {
        let y = rect.min.y + r as f32 * cell_h;
        painter.line_segment(
            [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
            egui::Stroke::new(1.0, grid_color),
        );
    }

    // Tag frame highlights.
    for (ti, tag) in vp.state.tags.iter().enumerate() {
        let is_selected = vp.state.selected_tag == Some(ti);
        let r = (tag.color >> 16) as u8;
        let g = (tag.color >> 8) as u8;
        let b = tag.color as u8;
        let fill = egui::Color32::from_rgba_unmultiplied(r, g, b, if is_selected { 80 } else { 40 });
        let stroke_color = egui::Color32::from_rgba_unmultiplied(r, g, b, if is_selected { 255 } else { 180 });

        let total_frames = (cols * rows) as usize;
        for fi in tag.from..=tag.to.min(total_frames.saturating_sub(1)) {
            let col = fi % cols as usize;
            let row = fi / cols as usize;
            let x0 = rect.min.x + col as f32 * cell_w;
            let y0 = rect.min.y + row as f32 * cell_h;
            let cell_rect = egui::Rect::from_min_size(egui::pos2(x0, y0), egui::vec2(cell_w, cell_h));
            painter.rect(cell_rect, 0.0, fill, egui::Stroke::new(1.5, stroke_color));
        }
    }

    // Row/Col spinners.
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Rows:");
        let mut rows_val = vp.state.rows;
        if ui.add(egui::DragValue::new(&mut rows_val).range(1_u32..=64)).changed() {
            vp.state.rows = rows_val;
            clamp_tag_frames(&mut vp.state);
            rebuild_preview_sheet(vp);
        }
        ui.label("Cols:");
        let mut cols_val = vp.state.cols;
        if ui.add(egui::DragValue::new(&mut cols_val).range(1_u32..=64)).changed() {
            vp.state.cols = cols_val;
            clamp_tag_frames(&mut vp.state);
            rebuild_preview_sheet(vp);
        }
    });
}

fn clamp_tag_frames(state: &mut SpriteEditorState) {
    let max_frame = (state.rows * state.cols).saturating_sub(1) as usize;
    for tag in &mut state.tags {
        tag.from = tag.from.min(max_frame);
        tag.to = tag.to.min(max_frame);
    }
}
```

- [ ] **Step 6: Implement draw_controls_column**

```rust
fn draw_controls_column(ctx: &egui::Context, ui: &mut egui::Ui, vp: &mut SpriteEditorViewport) {
    ui.heading("Tags");

    // Tag list.
    let mut rebuild = false;
    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
        let n = vp.state.tags.len();
        for i in 0..n {
            let tag = &vp.state.tags[i];
            let is_sel = vp.state.selected_tag == Some(i);
            let r = (tag.color >> 16) as u8;
            let g = (tag.color >> 8) as u8;
            let b = tag.color as u8;
            let (rect, resp) = ui.allocate_at_least(
                egui::vec2(ui.available_width(), 22.0),
                egui::Sense::click(),
            );
            if resp.clicked() {
                vp.state.selected_tag = Some(i);
            }
            let painter = ui.painter_at(rect);
            let bg = if is_sel { egui::Color32::from_gray(60) } else { egui::Color32::TRANSPARENT };
            painter.rect_filled(rect, 0.0, bg);
            // Color swatch.
            let swatch = egui::Rect::from_min_size(rect.min + egui::vec2(4.0, 4.0), egui::vec2(14.0, 14.0));
            painter.rect_filled(swatch, 2.0, egui::Color32::from_rgb(r, g, b));
            // Tag name + range.
            let label = format!("{}  {}-{}", tag.name, tag.from, tag.to);
            painter.text(
                rect.min + egui::vec2(24.0, 11.0),
                egui::Align2::LEFT_CENTER,
                &label,
                egui::FontId::proportional(13.0),
                egui::Color32::WHITE,
            );
            // Behavior combo on same row.
            let behavior = current_behavior_for_tag(&vp.state.tag_map, &tag.name);
            // Use allocate_ui_at_rect instead of child_ui (avoids signature changes in egui 0.33)
            let tag_name = tag.name.clone();
            let combo_rect = egui::Rect::from_min_size(
                egui::pos2(rect.max.x - 115.0, rect.min.y),
                egui::vec2(112.0, rect.height()),
            );
            ui.allocate_ui_at_rect(combo_rect, |ui| {
                egui::ComboBox::from_id_salt(format!("beh_{i}"))
                    .width(108.0)
                    .selected_text(&behavior)
                    .show_ui(ui, |ui| {
                        for slot in BEHAVIOR_SLOTS {
                            if ui.selectable_label(behavior == *slot, *slot).clicked() {
                                set_behavior_mapping(&mut vp.state.tag_map, slot, &tag_name);
                                rebuild = true;
                            }
                        }
                    });
            });
        }
    });

    // Remove button.
    let can_remove = vp.state.selected_tag.is_some();
    if ui.add_enabled(can_remove, egui::Button::new("Remove tag")).clicked() {
        if let Some(i) = vp.state.selected_tag {
            vp.state.tags.remove(i);
            vp.state.selected_tag = if vp.state.tags.is_empty() { None }
                else { Some(i.min(vp.state.tags.len() - 1)) };
            rebuild = true;
        }
    }

    if rebuild {
        rebuild_preview_sheet(vp);
    }

    // Add tag section.
    ui.separator();
    ui.label("Add tag:");
    // Store add-tag form state in vp via a separate helper struct embedded in vp (or use egui memory).
    // For simplicity, use egui persistent memory:
    let form_id = ui.make_persistent_id("add_tag_form");
    let mut form: AddTagForm = ctx.data_mut(|d| d.get_temp(form_id).unwrap_or_default());
    ui.horizontal(|ui| {
        ui.label("Name:");
        ui.text_edit_singleline(&mut form.name);
    });
    ui.horizontal(|ui| {
        let max_frame = (vp.state.rows * vp.state.cols).saturating_sub(1) as usize;
        ui.label("From:");
        ui.add(egui::DragValue::new(&mut form.from).range(0..=max_frame));
        ui.label("To:");
        ui.add(egui::DragValue::new(&mut form.to).range(0..=max_frame));
    });
    ui.horizontal(|ui| {
        ui.label("Direction:");
        egui::ComboBox::from_id_salt("dir_combo")
            .selected_text(form.direction.label())
            .show_ui(ui, |ui| {
                for dir in [TagDirection::Forward, TagDirection::Reverse,
                            TagDirection::PingPong, TagDirection::PingPongReverse] {
                    if ui.selectable_label(form.direction == dir, dir.label()).clicked() {
                        form.direction = dir;
                    }
                }
            });
    });
    if ui.button("Add").clicked() && !form.name.is_empty() {
        let color = TAG_COLORS[vp.state.tags.len() % TAG_COLORS.len()];
        let to = form.to.max(form.from);
        vp.state.tags.push(EditorTag {
            name: form.name.clone(),
            from: form.from,
            to,
            direction: form.direction.clone(),
            color,
        });
        vp.state.selected_tag = Some(vp.state.tags.len() - 1);
        form = AddTagForm::default(); // reset
        rebuild_preview_sheet(vp);
    }
    ctx.data_mut(|d| d.insert_temp(form_id, form));

    // Live preview.
    ui.separator();
    ui.label("Preview:");
    if let Some(sheet) = &vp.preview_sheet {
        if let Some(tex) = &vp.texture {
            let frame_changed = vp.anim.tick(sheet, 16); // approximate; real delta from repaint timing
            let _ = frame_changed;
            let abs = vp.anim.absolute_frame(sheet);
            if let Some(frame) = sheet.frames.get(abs) {
                let img_w = vp.state.image.width() as f32;
                let img_h = vp.state.image.height() as f32;
                let uv_min = egui::pos2(frame.x as f32 / img_w, frame.y as f32 / img_h);
                let uv_max = egui::pos2(
                    (frame.x + frame.w) as f32 / img_w,
                    (frame.y + frame.h) as f32 / img_h,
                );
                let preview_size = egui::vec2(64.0, 64.0);
                let img = egui::Image::new((tex.id(), preview_size))
                    .uv(egui::Rect::from_min_max(uv_min, uv_max));
                ui.add(img);
            }
        }
    } else {
        ui.label("(select a tag to preview)");
    }

    // Save / Export.
    ui.separator();
    let saveable = vp.state.is_saveable();
    if ui.add_enabled(saveable, egui::Button::new("Save")).clicked() {
        let dir = crate::window::sprite_gallery::SpriteGallery::appdata_sprites_dir();
        if let Err(e) = vp.state.save_to_dir(&dir) {
            log::warn!("save failed: {e}");
        } else {
            log::info!("saved to {}", dir.display());
        }
    }
    if !saveable {
        ui.label(
            egui::RichText::new("Assign idle and walk to enable Save")
                .color(egui::Color32::from_rgb(220, 180, 40)),
        );
    }
    if ui.button("Export…").clicked() {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            let clean = vp.state.to_clean_json();
            let stem = vp.state.png_path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("sprite");
            let json_path = dir.join(format!("{stem}.json"));
            let _ = std::fs::write(&json_path, &clean);
            let _ = std::fs::copy(&vp.state.png_path, dir.join(format!("{stem}.png")));
        }
    }
}
```

- [ ] **Step 7: Implement helper functions**

```rust
// ─── Behavior mapping helpers ─────────────────────────────────────────────────

const BEHAVIOR_SLOTS: &[&str] = &[
    "— not set —",
    "idle", "walk", "run", "sit", "sleep", "wake",
    "grabbed", "petted", "react", "fall", "thrown",
];

fn current_behavior_for_tag(tm: &AnimTagMap, tag_name: &str) -> String {
    if tm.idle == tag_name    { return "idle".into(); }
    if tm.walk == tag_name    { return "walk".into(); }
    if tm.run.as_deref()     == Some(tag_name) { return "run".into(); }
    if tm.sit.as_deref()     == Some(tag_name) { return "sit".into(); }
    if tm.sleep.as_deref()   == Some(tag_name) { return "sleep".into(); }
    if tm.wake.as_deref()    == Some(tag_name) { return "wake".into(); }
    if tm.grabbed.as_deref() == Some(tag_name) { return "grabbed".into(); }
    if tm.petted.as_deref()  == Some(tag_name) { return "petted".into(); }
    if tm.react.as_deref()   == Some(tag_name) { return "react".into(); }
    if tm.fall.as_deref()    == Some(tag_name) { return "fall".into(); }
    if tm.thrown.as_deref()  == Some(tag_name) { return "thrown".into(); }
    "— not set —".into()
}

fn set_behavior_mapping(tm: &mut AnimTagMap, behavior: &str, tag_name: &str) {
    // Clear any slot already mapped to tag_name.
    if tm.idle == tag_name  { tm.idle  = String::new(); }
    if tm.walk == tag_name  { tm.walk  = String::new(); }
    for opt in [
        &mut tm.run, &mut tm.sit, &mut tm.sleep, &mut tm.wake,
        &mut tm.grabbed, &mut tm.petted, &mut tm.react,
        &mut tm.fall, &mut tm.thrown,
    ] {
        if opt.as_deref() == Some(tag_name) { *opt = None; }
    }
    // Write the new mapping.
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
        _         => { /* "— not set —": old slot already cleared */ }
    }
}

// ─── Preview sheet ────────────────────────────────────────────────────────────

fn rebuild_preview_sheet(vp: &mut SpriteEditorViewport) {
    if vp.state.tags.is_empty() || vp.state.cols == 0 || vp.state.rows == 0 {
        vp.preview_sheet = None;
        return;
    }
    // Build a minimal JSON and reload via load_embedded to reuse existing parsing.
    let json = vp.state.to_json();
    let png_bytes = {
        let mut buf = std::io::Cursor::new(Vec::new());
        if vp.state.image.write_to(&mut buf, image::ImageFormat::Png).is_err() {
            vp.preview_sheet = None;
            return;
        }
        buf.into_inner()
    };
    vp.preview_sheet = crate::sprite::sheet::load_embedded(&json, &png_bytes).ok();

    // Reset animation to first frame of selected tag.
    if let Some(sheet) = &vp.preview_sheet {
        let tag_name = vp.state.selected_tag
            .and_then(|i| vp.state.tags.get(i))
            .map(|t| t.name.clone())
            .or_else(|| sheet.tags.first().map(|t| t.name.clone()))
            .unwrap_or_default();
        vp.anim = AnimationState::new(tag_name);
    }
}

// ─── Add-tag form state (stored in egui memory) ───────────────────────────────

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct AddTagForm {
    name: String,
    from: usize,
    to: usize,
    direction: TagDirection,
}
```

**Note on AddTagForm serialization:** `TagDirection` derives were added in Task 3, Step 1
(prerequisites — `src/sprite/sheet.rs`). `AddTagForm` stores `TagDirection` and uses
`serde` because egui 0.33's `ctx.data_mut(|d| d.get_temp(...))` / `d.insert_temp(...)` API
requires `serde::Serialize + serde::Deserialize + Clone + 'static`. This is the documented
egui persistent-memory pattern. Do NOT fall back to `Arc<Mutex<>>` — the serde path is correct.

- [ ] **Step 8: Build and fix errors**

```bash
cargo build 2>&1 | grep "^error"
```

Common errors to expect and fix:
- Missing imports — add them one by one.
- `egui::DragValue::range` type mismatches (`usize` vs `u32`) — use explicit casts: e.g. `0_usize..=max_frame` stays `usize` if the field is `usize`; for `u32` fields use `1_u32..=4`.
- `allocate_ui_at_rect` requires `egui::Sense` — check if it takes `Sense::hover()` as a param in egui 0.33 and adjust if needed.

- [ ] **Step 9: Run tests**

```bash
cargo test 2>&1 | tail -10
```

Expected: 130 tests pass.

- [ ] **Step 10: Commit**

```bash
git add src/tray/sprite_editor.rs src/sprite/behavior.rs
git commit -m "feat: implement egui sprite editor"
```

---

## Final verification

- [ ] **Run full test suite one last time**

```bash
cargo test 2>&1 | tail -5
```

Expected: `130 passed`.

- [ ] **Build release to check size**

```bash
cargo build --release 2>&1 | tail -3
```

Expected: builds without errors (size will be larger than before due to wgpu; that's expected).
