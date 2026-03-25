# Unified App Window

## Context

The app currently has three separate egui deferred viewports:
- `config_window.rs` → `open_config_viewport` (pet list + per-pet config)
- `sprite_editor.rs` → `open_sprite_editor_viewport` (sprite sheet editor)
- `sm_editor.rs` → `open_sm_editor_viewport` (SM text editor + graph)

The user wants a single unified window with a tab bar replacing all three, opened from the tray icon. The Sprites tab needs a left-panel sprite gallery list (click to select which sprite to edit). The SM and Config tabs keep their existing left-panel + right-panel layout.

## Architecture

Single deferred viewport `"app_window"` in `src/tray/app_window.rs`:

```
┌─────────────────────────────────────────────────┐
│  [Config] [Sprites] [SM]                    [🌙] │  ← tab bar + dark/light toggle
├─────────────────────────────────────────────────┤
│ Left panel    │ Right panel / canvas             │
│ (list)        │                                  │
│               │                                  │
└─────────────────────────────────────────────────┘
```

### Tab: Config
- Left: pet list, each row = selectable label + inline remove ("✕") button
- Right: per-pet settings form (same as current config_window.rs central panel)
- Remove the standalone "Add Pet" bottom button (inline ✕ replaces it for removal)
- Keep "Add Pet" as a button at the bottom of the pet list
- Remove "Edit…" and "New from PNG…" buttons from Config tab (these live in Sprites tab)

### Tab: Sprites
- Left panel: sprite gallery list (scrollable). Each row = selectable label.
  - Selecting a sprite loads it into the editor on the right.
  - Button at bottom: "Import PNG…" to add a new sprite from a PNG file.
- Right panel: full sprite editor (same content as current sprite_editor.rs viewport renders)
  - The editor renders only when a sprite is selected (otherwise shows placeholder text)

### Tab: SM
- Identical to current SM editor (left browser + right text editor + graph)
- No changes needed to SM editor logic

## State Design

### `AppWindowState` (in `app_window.rs`)
```rust
pub struct AppWindowState {
    pub selected_tab: AppTab,
    pub should_close: bool,
    pub dark_mode: bool,
    pub dark_mode_out: Option<bool>,

    // Config tab state
    pub config: Config,
    pub selected_pet_idx: Option<usize>,
    pub config_tx: Sender<AppEvent>,
    loaded_sheet: Option<SpriteSheet>,
    loaded_sheet_path: String,

    // Sprites tab state
    pub gallery: SpriteGallery,          // from window::sprite_gallery
    pub selected_sprite_key: Option<SpriteKey>,
    pub sprite_editor: Option<SpriteEditorViewport>,  // None until sprite selected
    pub pending_png_pick: Option<crossbeam_channel::Receiver<Option<std::path::PathBuf>>>,
    pub saved_json_path: Option<std::path::PathBuf>,  // set after sprite save, consumed by App

    // SM tab state (reuse existing SmEditorViewport fields inline)
    pub sm: SmTabState,
}

pub enum AppTab { Config, Sprites, Sm }

pub struct SmTabState {
    // mirrors SmEditorViewport fields
    pub from_ui: SmEditorFromUi,
    pub from_app: SmEditorFromApp,
    pub selected_sm: Option<String>,
    pub editor_text: String,
    pub is_dirty: bool,
    pub config_dir: PathBuf,
    pub cached_gallery: Option<SmGallery>,
    pub save_errors: Vec<CompileError>,
    pub has_saved_once: bool,
    pub pending_delete: Option<String>,
}
```

## Files to Change

| File | Change |
|------|--------|
| `src/tray/app_window.rs` | **NEW** — unified window state + rendering |
| `src/tray/mod.rs` | Add `pub mod app_window;`, simplify tray menu |
| `src/tray/config_window.rs` | Extract rendering logic into `pub fn render_config_tab(ui, state)` callable from app_window |
| `src/tray/sprite_editor.rs` | Extract rendering into `pub fn render_sprite_editor_panel(ui, state)` |
| `src/tray/sm_editor.rs` | Extract rendering into `pub fn render_sm_tab(ui, state)` |
| `src/app.rs` | Replace three viewport states with `app_window: Option<Arc<Mutex<AppWindowState>>>` |
| `src/event.rs` | Add `TrayOpenWindow`, remove `TrayOpenConfig`, `TrayOpenSmEditor` |

## Tasks

### Task 1: Refactor config_window.rs — extract rendering into a reusable function

**Goal:** Separate state from rendering so the config UI can be called from a tab context.

**Changes in `src/tray/config_window.rs`:**
- Keep `ConfigWindowState` struct as-is (for now; it will be replaced in Task 4)
- Extract rendering into a free function:
  ```rust
  pub fn render_config_panel(ctx: &egui::Context, s: &mut ConfigWindowState)
  ```
  This function contains everything currently inside the `show_viewport_deferred` closure (except the `close_requested` check which belongs to the outer window).
- Keep `open_config_viewport` as a thin wrapper that calls `render_config_panel` (for backwards compat during refactor; will be removed in Task 4).

**No behavior changes.** Tests must still pass.

---

### Task 2: Refactor sprite_editor.rs — extract rendering into a reusable function

**Goal:** Separate state from rendering so the sprite editor UI can be called from a tab context.

**Changes in `src/tray/sprite_editor.rs`:**
- Extract the body of the `show_viewport_deferred` closure into:
  ```rust
  pub fn render_sprite_editor_panel(ctx: &egui::Context, s: &mut SpriteEditorViewport)
  ```
  Caller is responsible for: close_requested check, dark_mode apply. This function does everything else (texture upload, preview sheet rebuild, all panels).
- Keep `open_sprite_editor_viewport` as a thin wrapper that handles close_requested + calls `render_sprite_editor_panel`.

**No behavior changes.** Tests must still pass.

---

### Task 3: Refactor sm_editor.rs — extract rendering into a reusable function

**Goal:** Same pattern as Tasks 1–2.

**Changes in `src/tray/sm_editor.rs`:**
- Extract the rendering body into:
  ```rust
  pub fn render_sm_panel(ctx: &egui::Context, vp: &mut SmEditorViewport)
  ```
  This includes the keyboard shortcut handling, left browser panel, right editor+graph panels, and error bar. Caller handles close_requested.
- Keep `open_sm_editor_viewport` as a thin wrapper.

**No behavior changes.** Tests must still pass.

---

### Task 4: Create `src/tray/app_window.rs` — unified tabbed window

**Goal:** Single deferred viewport with Config / Sprites / SM tabs.

**New file `src/tray/app_window.rs`:**

```rust
pub struct AppWindowState {
    pub selected_tab: AppTab,
    pub should_close: bool,
    pub dark_mode: bool,
    pub dark_mode_out: Option<bool>,

    // ── Config tab ──
    pub config: crate::config::schema::Config,
    pub selected_pet_idx: Option<usize>,
    pub config_tx: crossbeam_channel::Sender<crate::event::AppEvent>,
    loaded_sheet: Option<crate::sprite::sheet::SpriteSheet>,
    loaded_sheet_path: String,

    // ── Sprites tab ──
    pub gallery: crate::window::sprite_gallery::SpriteGallery,
    pub selected_sprite_key: Option<crate::window::sprite_gallery::SpriteKey>,
    pub sprite_editor: Option<crate::tray::sprite_editor::SpriteEditorViewport>,
    pub pending_png_pick: Option<crossbeam_channel::Receiver<Option<std::path::PathBuf>>>,
    pub saved_json_path: Option<std::path::PathBuf>,

    // ── SM tab ──
    pub sm: crate::tray::sm_editor::SmEditorViewport,
}

#[derive(PartialEq)]
pub enum AppTab { Config, Sprites, Sm }
```

`AppWindowState::new(config, tx, dark_mode, config_dir)` initializes all fields.

`open_app_window(ctx, state: Arc<Mutex<AppWindowState>>)` shows one deferred viewport `"app_window"` 1000×640. Inside:
1. Handle close_requested → `s.should_close = true`
2. Apply theme
3. Top bar (using `egui::TopBottomPanel::top`) with tab buttons (Config / Sprites / SM) and dark/light toggle on the right
4. Match `s.selected_tab`:
   - `Config`: render the config panel inline (left pet list + right settings, same as config_window.rs but without a separate viewport). Pet list rows have inline ✕ remove buttons. "Add Pet" button at bottom of list.
   - `Sprites`: render left panel with sprite gallery list + right panel with `render_sprite_editor_panel`. If no sprite selected, right panel shows "Select a sprite to edit." If a PNG pick is pending, poll it and load the new sprite into the editor.
   - `Sm`: call `render_sm_panel(ctx, &mut s.sm)`.

**The Config panel** in the unified window:
- Left `SidePanel` with pet list. Each row: `ui.horizontal(|ui| { selectable_label + ui.small_button("✕").on_click(remove) })`.
- "Add Pet" button at bottom of left panel.
- Right `CentralPanel` with per-pet settings (identical to current config_window.rs right panel).
- Changes to config dispatch `AppEvent::ConfigChanged` via `s.config_tx`.

**The Sprites panel**:
- Left `SidePanel` with gallery entries (selectable labels). Selecting loads the sprite into `s.sprite_editor`.
- Button at bottom: "Import PNG…" — opens file dialog, picks PNG, creates `SpriteEditorViewport` from the chosen PNG via `App::new_editor_state_from_png`.
- Right panel: if `s.sprite_editor.is_some()`, render the sprite editor content via `render_sprite_editor_panel(ctx, editor)`. Otherwise "Select a sprite to edit."

Add `pub mod app_window;` to `src/tray/mod.rs`.

---

### Task 5: Wire unified window into `app.rs` and simplify tray

**Goal:** Replace three separate viewport states with single `app_window` state. Update tray menu.

**Changes in `src/event.rs`:**
- Add `TrayOpenWindow`
- Keep `TrayOpenConfig`, `TrayOpenSmEditor` with `#[allow(dead_code)]` (they can be removed later; keeping avoids merge conflicts)

**Changes in `src/tray/mod.rs`:**
- Replace "Add Pet", "Configure...", "Edit State Machines" menu items with single "Open..." item that sends `TrayOpenWindow`
- Keep "Import Bundle..." and "Quit"

**Changes in `src/app.rs`:**
- Add field `app_window: Option<Arc<Mutex<AppWindowState>>>`
- Initialize as `None`
- Handle `AppEvent::TrayOpenWindow`: if `None`, create `AppWindowState::new(current_config, tx, dark_mode, config_dir)` and wrap in Arc<Mutex>; if already open, send focus command to `"app_window"` viewport.
- In `update()`: replace the three separate viewport open/close blocks with a single block for `app_window`:
  - Push `dark_mode` in
  - Read `dark_mode_out`, `should_close`, `saved_json_path`, SM communication fields
  - Call `open_app_window(ctx, state.clone())` if not closing
  - Handle hot-reload on `saved_json_path`
  - Handle SM debug commands (force_state, step_mode, etc.) from `s.sm.from_ui`
  - Handle SM hot-reload from `s.sm.from_ui.saved_sm_name`
  - Close and set to `None` when `should_close`
- Remove the old `config_window_state`, `sprite_editor_state`, `sm_editor` fields and their associated handling code.
- Remove the `open_editor_request` handling block (the sprite editor is now launched from the Sprites tab directly).
- Keep `TrayAddPet`, `TrayRemovePet` events handling as-is (for wndproc-initiated actions).

**Tests must still pass after this change.**
