# SM Editor UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the in-app SM editor: a text editor for `.petstate` files with save-time validation, a live state graph, and debug tools (force state, step mode, variable inspector, transition log).

**Architecture:** `SMEditorViewport` follows the exact threading pattern of `SpriteEditorViewport` — an `Arc<Mutex<SMEditorViewport>>` shared between the egui viewport thread and the app main thread. The app polls it each frame for debug commands and pushes back live state snapshots. The SM editor does NOT parse or execute TOML itself; it calls `SmGallery::save()` for validation and delegates all SM logic to the existing compiler/runner.

**Tech Stack:** Rust, `egui` / `eframe` (existing), SM compiler and runner from Core Engine plan

**Prerequisite:** Both Core Engine and Asset Pipeline plans must be complete.

**Spec:** `docs/superpowers/specs/2026-03-23-user-defined-state-machines.md`

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src/tray/sm_editor.rs` | **Create** | SM editor egui viewport — all UI and shared state |
| `src/app.rs` | **Modify** | Open SM editor from tray; poll viewport; push debug state; apply force/step commands |
| `src/sprite/sm_runner.rs` | **Modify** | Consume `force_state`, `step_mode`, `step_advance` from editor; expose `transition_log` |
| `src/event.rs` | **Modify** | Add `TrayOpenSmEditor` |
| `src/tray/mod.rs` | **Modify** | Add "Edit State Machines" tray menu item |

---

## Task 1: `SMEditorViewport` shared state struct

**Files:**
- Create: `src/tray/sm_editor.rs`
- Modify: `src/tray/mod.rs`

- [ ] Create `src/tray/sm_editor.rs` with the shared state struct (no UI yet):

```rust
use std::sync::{Arc, Mutex};
use crate::sprite::sm_compiler::CompileError;

/// A single entry in the transition log.
#[derive(Clone, Debug)]
pub struct TransitionLogEntry {
    pub from: String,
    pub to: String,
    pub reason: String,  // "weight 45, timer 2.3s" or "interrupt: petted"
}

/// Live snapshot of condition variables for the inspector.
#[derive(Clone, Debug, Default)]
pub struct VarSnapshot {
    pub cursor_dist: f32,
    pub state_time_ms: u32,
    pub on_surface: bool,
    pub near_edge: bool,
    pub pet_x: f32,
    pub pet_y: f32,
    pub pet_vx: f32,
    pub pet_vy: f32,
    pub pet_v: f32,
    pub hour: u32,
    pub focused_app: String,
}

/// Fields written by the egui thread, read by the app thread each frame.
pub struct SmEditorFromUi {
    pub saved_sm_name: Option<String>,   // name of SM just saved (triggers hot-reload)
    pub force_state: Option<String>,     // debug: force pet into this named state
    pub release_force: bool,             // debug: release forced state (separate from force_state)
    pub step_mode: bool,
    pub step_advance: bool,              // consumed once per frame
    pub should_close: bool,
}

/// Fields written by the app thread, read by the egui thread each frame.
pub struct SmEditorFromApp {
    pub active_state: Option<String>,   // from runner.current_state_name()
    pub is_forced: bool,                // from runner.force_state.is_some()
    pub var_snapshot: VarSnapshot,
    pub transition_log: Vec<TransitionLogEntry>,
    pub validation_errors: Vec<CompileError>,   // set after save attempt
}

pub struct SmEditorViewport {
    pub from_ui: SmEditorFromUi,
    pub from_app: SmEditorFromApp,

    // Internal egui state (only touched by egui thread):
    pub selected_sm: Option<String>,
    pub editor_text: String,
    pub is_dirty: bool,
    pub dark_mode: bool,
}

impl SmEditorViewport {
    pub fn new(dark_mode: bool) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            from_ui: SmEditorFromUi {
                saved_sm_name: None,
                force_state: None,
                step_mode: false,
                step_advance: false,
                should_close: false,
            },
            from_app: SmEditorFromApp {
                active_state: None,
                var_snapshot: VarSnapshot::default(),
                transition_log: Vec::new(),
                validation_errors: Vec::new(),
            },
            selected_sm: None,
            editor_text: String::new(),
            is_dirty: false,
            dark_mode,
        }))
    }
}
```

- [ ] Add `pub mod sm_editor;` to `src/tray/mod.rs`.

- [ ] Build to confirm struct definitions compile:
```
cargo build
```

- [ ] Commit:
```
git add src/tray/sm_editor.rs src/tray/mod.rs
git commit -m "feat: SMEditorViewport shared state struct"
```

---

## Task 2: App thread integration — open, poll, push

**Files:**
- Modify: `src/app.rs`
- Modify: `src/event.rs`
- Modify: `src/tray/mod.rs` (tray menu)

- [ ] Add to `src/event.rs`:
```rust
TrayOpenSmEditor,
```

- [ ] Add "Edit State Machines" to tray menu in `tray/mod.rs`, sending `TrayOpenSmEditor`.

- [ ] In `App` struct, add:
```rust
sm_editor: Option<Arc<Mutex<SmEditorViewport>>>,
```

- [ ] Handle `TrayOpenSmEditor`: open a new egui viewport (same pattern as `TrayOpenConfig`), store in `sm_editor`.

- [ ] In `App::update()`, if `sm_editor` is `Some`, each frame:

```rust
if let Some(viewport) = &self.sm_editor {
    let mut vp = viewport.lock().unwrap();

    // Push live state to editor
    if let Some(pet) = self.pets.first() {
        vp.from_app.active_state = pet.runner.current_state_name().map(String::from);
        vp.from_app.var_snapshot = pet.runner.last_condition_vars().into();
        vp.from_app.transition_log = pet.runner.transition_log().to_vec();
    }

    // Consume debug commands from editor
    if let Some(state_name) = vp.from_ui.force_state.take() {
        if let Some(pet) = self.pets.first_mut() {
            pet.runner.force_state = Some(state_name);
        }
    }
    if vp.from_ui.release_force {
        vp.from_ui.release_force = false;
        if let Some(pet) = self.pets.first_mut() {
            pet.runner.release_force = true;
        }
    }
    vp.from_app.is_forced = self.pets.first().map(|p| p.runner.force_state.is_some()).unwrap_or(false);
    if vp.from_ui.step_mode != self.pets.first().map(|p| p.runner.step_mode).unwrap_or(false) {
        if let Some(pet) = self.pets.first_mut() {
            pet.runner.step_mode = vp.from_ui.step_mode;
        }
    }
    if vp.from_ui.step_advance {
        vp.from_ui.step_advance = false;
        if let Some(pet) = self.pets.first_mut() {
            pet.runner.step_advance = true;
        }
    }

    // Close viewport
    if vp.from_ui.should_close {
        self.sm_editor = None;
    }
}
```

- [ ] Add `current_state_name()`, `last_condition_vars()`, `transition_log()` accessors to `SMRunner`.

- [ ] Add `transition_log: VecDeque<TransitionLogEntry>` (max 10) to `SMRunner`; push to it on every state transition in `enter_named()`.

- [ ] Build and open the SM editor from the tray (window appears, even if empty):
```
cargo run
```

- [ ] Commit:
```
git add src/app.rs src/event.rs src/tray/mod.rs src/sprite/sm_runner.rs
git commit -m "feat: app thread integration for SM editor — open, poll, push live state"
```

---

## Task 3: SM editor layout — browser panel and text editor

**Files:**
- Modify: `src/tray/sm_editor.rs`

- [ ] Implement the egui `update()` function for the SM editor viewport with the three-panel layout (left browser, center text editor, bottom error bar):

```rust
pub fn update(ctx: &egui::Context, state: &Arc<Mutex<SmEditorViewport>>, gallery: &Arc<Mutex<SmGallery>>) {
    let mut vp = state.lock().unwrap();
    let gallery = gallery.lock().unwrap();

    egui::SidePanel::left("sm_browser").show(ctx, |ui| {
        ui.heading("State Machines");
        ui.separator();

        // Valid SMs
        for name in gallery.valid_names() {
            let selected = vp.selected_sm.as_deref() == Some(name);
            if ui.selectable_label(selected, name).clicked() {
                vp.selected_sm = Some(name.to_string());
                vp.editor_text = gallery.source(name).unwrap_or_default();
                vp.is_dirty = false;
            }
        }

        ui.separator();
        ui.label(egui::RichText::new("Drafts").color(egui::Color32::GRAY));
        for name in gallery.draft_names() {
            let selected = vp.selected_sm.as_deref() == Some(name);
            if ui.selectable_label(selected, egui::RichText::new(name).color(egui::Color32::GRAY)).clicked() {
                vp.selected_sm = Some(name.to_string());
                vp.editor_text = gallery.draft_source(name).unwrap_or_default();
                vp.is_dirty = false;
            }
        }

        ui.separator();
        if ui.button("📄 New SM").clicked() {
            vp.editor_text = MINIMAL_TEMPLATE.to_string();
            vp.selected_sm = None;
            vp.is_dirty = true;
        }
        if ui.button("📋 Copy Built-in Default").clicked() {
            vp.editor_text = crate::sprite::sm_runner::DEFAULT_SM_TOML.to_string();
            vp.selected_sm = None;
            vp.is_dirty = true;
        }
    });

    egui::TopBottomPanel::bottom("sm_errors").show(ctx, |ui| {
        // Error list rendered in Task 5
    });

    egui::CentralPanel::default().show(ctx, |ui| {
        // Text editor + save button
        let save_btn = ui.button(if vp.is_dirty { "💾 Save*" } else { "💾 Save" });
        // Text editor rendered in Task 4
    });
}

const MINIMAL_TEMPLATE: &str = r#"[meta]
name = "My SM"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[interrupts]
grabbed = { goto = "grabbed" }
petted  = { goto = "petted" }

[states.idle]
required = true
action   = "idle"
transitions = [
  { goto = "walk", weight = 50, after = "1s..3s" },
]

[states.walk]
required = true
action   = "walk"
dir      = "random"
distance = "200px..600px"
transitions = [{ goto = "idle" }]

[states.grabbed]
required = true
action   = "grabbed"
transitions = []

[states.fall]
required = true
action   = "fall"
transitions = [{ goto = "idle", condition = "on_surface" }]

[states.thrown]
required = true
action   = "thrown"
transitions = [{ goto = "fall", condition = "on_surface" }]
"#;
```

- [ ] Run app, open SM editor, verify browser panel lists SMs and clicking one loads its source.

- [ ] Commit:
```
git add src/tray/sm_editor.rs
git commit -m "feat: SM editor browser panel with SM list and new/copy-default buttons"
```

---

## Task 4: Text editor with dirty tracking

**Files:**
- Modify: `src/tray/sm_editor.rs`

- [ ] In the center panel, render a multi-line text editor using `egui::TextEdit::multiline`:

```rust
let response = egui::TextEdit::multiline(&mut vp.editor_text)
    .font(egui::TextStyle::Monospace)
    .desired_width(f32::INFINITY)
    .desired_rows(40)
    .show(ui);

if response.response.changed() {
    vp.is_dirty = true;
}
```

- [ ] Status bar above the text area showing `✓ Valid` (green) or `✗ N error(s)` (red) based on `vp.from_app.validation_errors`.

- [ ] Run app, type in the editor, verify dirty flag appears on the Save button.

- [ ] Commit:
```
git add src/tray/sm_editor.rs
git commit -m "feat: monospace text editor with dirty tracking in SM editor"
```

---

## Task 5: Save button — validation and draft promotion

**Files:**
- Modify: `src/tray/sm_editor.rs`
- Modify: `src/app.rs`

- [ ] On Save button click in `sm_editor.rs`:
  1. Extract SM name from TOML `[meta].name` (parse just the meta section, or full parse)
  2. Call `gallery.save(name, &vp.editor_text)` — returns `true` (live) or `false` (draft)
  3. If live: set `vp.from_ui.saved_sm_name = Some(name)` to notify app thread; set `is_dirty = false`
  4. If draft: leave `is_dirty = true`; errors already in `from_app.validation_errors`

- [ ] In `app.rs`, when `from_ui.saved_sm_name` is `Some`:
  - Hot-reload: find any pet using that SM by name, recompile + swap `runner.sm`
  - Send `AppEvent::SMCollectionChanged` so config window can refresh its picker

- [ ] Error list in bottom panel: render `vp.from_app.validation_errors`, each as a clickable row. On click, highlight the relevant line in the text editor (use `egui::TextEdit::cursor_at_begin` or scroll to line).

- [ ] Promotion: if user saves a draft that now passes validation, show a toast: *"'My SM' promoted from draft to live SM"*. Implement as a `Option<String>` field on the viewport that app thread reads and shows.

- [ ] Test: type a valid SM, save — appears in SM list. Type an invalid SM, save — appears in drafts. Fix the error, save — promoted to live.

- [ ] Commit:
```
git add src/tray/sm_editor.rs src/app.rs
git commit -m "feat: SM editor save with validation, draft/live routing, and hot-reload"
```

---

## Task 6: Live state graph

**Files:**
- Modify: `src/tray/sm_editor.rs`

- [ ] In the right panel, render a state graph from the last valid compiled SM. Use `egui::Painter` for custom drawing (no external graph library needed):

Strategy: simple **grid layout** — states arranged in a grid, arrows between them for transitions.

```rust
fn draw_state_graph(ui: &mut egui::Ui, sm: &CompiledSM, active_state: Option<&str>) {
    let (response, painter) = ui.allocate_painter(
        ui.available_size(),
        egui::Sense::hover(),
    );
    let rect = response.rect;

    let state_names: Vec<&str> = sm.states.keys().map(String::as_str).collect();
    let cols = (state_names.len() as f32).sqrt().ceil() as usize;
    let node_w = 100.0f32;
    let node_h = 30.0f32;
    let gap_x = 40.0f32;
    let gap_y = 50.0f32;

    // Compute positions
    let positions: HashMap<&str, egui::Pos2> = state_names.iter().enumerate().map(|(i, &name)| {
        let col = (i % cols) as f32;
        let row = (i / cols) as f32;
        let x = rect.left() + col * (node_w + gap_x) + node_w / 2.0;
        let y = rect.top() + row * (node_h + gap_y) + node_h / 2.0;
        (name, egui::pos2(x, y))
    }).collect();

    // Draw arrows first (behind nodes)
    for (state_name, state) in &sm.states {
        let from = positions[state_name.as_str()];
        let transitions = state.kind.transitions();
        for t in transitions {
            if let Goto::State(to_name) = &t.goto {
                if let Some(&to) = positions.get(to_name.as_str()) {
                    painter.arrow(from, to - from, egui::Stroke::new(1.0, egui::Color32::GRAY));
                }
            }
        }
    }

    // Draw nodes
    for (name, &center) in &positions {
        let node_rect = egui::Rect::from_center_size(center, egui::vec2(node_w, node_h));
        let is_active = active_state == Some(name);
        let bg = if is_active { egui::Color32::from_rgb(60, 120, 200) } else { egui::Color32::from_gray(50) };
        painter.rect_filled(node_rect, 4.0, bg);
        painter.text(center, egui::Align2::CENTER_CENTER, name, egui::FontId::monospace(11.0), egui::Color32::WHITE);
    }
}
```

- [ ] Show the ▶ Force button on hover over each state node. On click, set `vp.from_ui.force_state = Some(name.to_string())`.

- [ ] Show composite states as a slightly different shape (rounded, different border).

- [ ] Run app, open SM editor, verify state graph renders and active state highlights in real-time.

- [ ] Commit:
```
git add src/tray/sm_editor.rs
git commit -m "feat: live state graph with active state highlight and force-state buttons"
```

---

## Task 7: Force state and step mode debug tools

**Files:**
- Modify: `src/tray/sm_editor.rs`

- [ ] Add `is_forced: bool` to `SmEditorFromApp` (app thread sets this to `runner.force_state.is_some()` each frame). When `is_forced` is true:
  - Show banner at top of right panel: `⏸ FORCED: [state]` in amber
  - Show `[▶ Release]` button: on click, sets `vp.from_ui.release_force = true` (consumed by app thread → calls `runner.release_force = true`, which SMRunner checks at top of tick)

- [ ] Step mode toggle:
  - Checkbox in right panel: `[ ] Step mode`
  - Bound to `vp.from_ui.step_mode`
  - When checked: show `[→ Next transition]` button, wired to `step_advance = true`

- [ ] Interrupts fire during both force and step mode (already handled in SMRunner — just document the behavior in the UI with a tooltip).

- [ ] Run app, force a state, verify pet freezes in that state but can still be grabbed.

- [ ] Commit:
```
git add src/tray/sm_editor.rs
git commit -m "feat: force state banner and step mode toggle in SM editor"
```

---

## Task 8: Variable inspector and transition log

**Files:**
- Modify: `src/tray/sm_editor.rs`

- [ ] Below the state graph in the right panel, add a collapsible **Variable Inspector** section:

```rust
egui::CollapsingHeader::new("🔍 Variables").show(ui, |ui| {
    let v = &vp.from_app.var_snapshot;
    egui::Grid::new("vars").striped(true).show(ui, |ui| {
        ui.label("cursor_dist"); ui.label(format!("{:.0}px", v.cursor_dist)); ui.end_row();
        ui.label("state_time");  ui.label(format!("{:.1}s", v.state_time_ms as f32 / 1000.0)); ui.end_row();
        ui.label("on_surface");  ui.label(if v.on_surface { "true" } else { "false" }); ui.end_row();
        ui.label("near_edge");   ui.label(if v.near_edge { "true" } else { "false" }); ui.end_row();
        ui.label("pet.x/y");     ui.label(format!("{:.0}, {:.0}", v.pet_x, v.pet_y)); ui.end_row();
        ui.label("pet.vx/vy/v"); ui.label(format!("{:.0}, {:.0}, {:.0}", v.pet_vx, v.pet_vy, v.pet_v)); ui.end_row();
        ui.label("time.hour");   ui.label(format!("{}", v.hour)); ui.end_row();
        ui.label("focused_app"); ui.label(&v.focused_app); ui.end_row();
    });
});
```

- [ ] Add a collapsible **Transition Log** section:

```rust
egui::CollapsingHeader::new("📋 Transitions").default_open(true).show(ui, |ui| {
    for entry in vp.from_app.transition_log.iter().rev() {
        ui.label(format!("{} → {}  ({})", entry.from, entry.to, entry.reason));
    }
});
```

- [ ] In `SMRunner::enter_named()`, push to `transition_log` whenever state changes, capping at 10 entries (`VecDeque`).

- [ ] Run app, force some state changes, verify log updates in real time.

- [ ] Commit:
```
git add src/tray/sm_editor.rs src/sprite/sm_runner.rs
git commit -m "feat: variable inspector and transition log in SM editor"
```

---

## Task 9: Import `.petstate` from file

**Files:**
- Modify: `src/tray/sm_editor.rs`

- [ ] Add "Import .petstate" button in the left browser panel:

```rust
if ui.button("📂 Import .petstate").clicked() {
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("Pet State Machine", &["petstate"])
        .pick_file()
    {
        if let Ok(source) = std::fs::read_to_string(&path) {
            // Name collision check: parse meta.name from source
            // If collision: show inline conflict UI (rename/replace/cancel)
            // For simplicity: auto-append " (imported)" on collision
            vp.editor_text = source;
            vp.is_dirty = true;
            vp.selected_sm = None;
        }
    }
}
```

- [ ] Add "Import .petbundle" button calling `bundle::import()` inline (same as Plan 2 Task 5 but triggered from SM editor).

- [ ] Test: drag a `.petstate` file into a dialog, import it, verify it appears in the browser.

- [ ] Commit:
```
git add src/tray/sm_editor.rs
git commit -m "feat: import .petstate and .petbundle from SM editor"
```

---

## Task 10: Tray entry and final wiring

**Files:**
- Modify: `src/tray/mod.rs`
- Modify: `src/app.rs`

- [ ] Ensure "Edit State Machines" is in the tray right-click menu (verify from Task 2, add if missing).

- [ ] Ensure only one SM editor viewport opens at a time (guard in `App::handle_event`).

- [ ] Smoke test all features end-to-end:
  1. Open SM editor from tray
  2. Select "Default Pet" SM — source loads in text editor
  3. Change a walk weight to 90 — save — pet immediately walks more often (hot-reload)
  4. Add a typo in a condition (`typo_var < 5`) — save — saved as draft, error shown
  5. Fix the typo — save — promoted to live, pet reloads
  6. Force "sleep" state — pet goes to sleep animation, can still be grabbed
  7. Release force — pet resumes normal SM

- [ ] Run full test suite:
```
cargo test
```

- [ ] Commit:
```
git add src/tray/mod.rs src/app.rs
git commit -m "feat: SM editor fully wired — tray entry, hot-reload, debug tools complete"
```

---

## Verification

After all tasks complete:

1. `cargo test` — all tests pass
2. Open SM editor from system tray — window appears with browser, text editor, state graph
3. Edit a SM and save — hot-reload visible immediately on running pet
4. Invalid SM saves as draft — does not affect running pet
5. Force a state — pet freezes in that state; grab still works; release resumes SM
6. Step mode — pet freezes between transitions; advance one step at a time
7. Variable inspector shows live values updating every frame
8. Transition log shows last 10 state changes in real time
