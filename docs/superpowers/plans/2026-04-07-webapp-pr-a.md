# Webapp PR-A: Visual & Simulation Fixes

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all "invisible selected text" labels, make the SM editor usable at narrow viewports, and fix the simulation pet starting position and floor rendering.

**Architecture:** Three independent changes: (1) a one-line theme fix in `ui_theme.rs`, (2) panel layout adjustments in `sm_editor.rs`, (3) simulation coordinate fixes in `simulation.rs`. No new files needed. Also hide the close button in the web build.

**Tech Stack:** Rust, egui 0.33, eframe 0.33, wasm32-unknown-unknown

---

## Files

- Modify: `crates/ferrite-egui/src/ui_theme.rs` — fix selection stroke color
- Modify: `crates/ferrite-egui/src/sm_editor.rs` — reduce/collapse right panel at narrow widths
- Modify: `crates/ferrite-webapp/src/simulation.rs` — init pet at floor, dynamic floor y
- Modify: `crates/ferrite-egui/src/app_window.rs` — hide close button on wasm

---

### Task 1: Fix invisible selected-item text (B-01)

**Files:**
- Modify: `crates/ferrite-egui/src/ui_theme.rs`

**Root cause:** `vis.selection.stroke = Stroke::new(1.0, accent)` sets the selected-item text color to indigo — the same color as `selection.bg_fill`. Both are `Color32::from_rgb(99, 102, 241)`, making text invisible.

- [ ] **Step 1: Open `ui_theme.rs` and find the selection stroke line**

```
crates/ferrite-egui/src/ui_theme.rs
```

Look for:
```rust
vis.selection.stroke = Stroke::new(1.0, accent);
```

- [ ] **Step 2: Change it to white**

```rust
// Before
vis.selection.stroke = Stroke::new(1.0, accent);

// After — white text on indigo background gives 5.5:1 contrast ratio
vis.selection.stroke = Stroke::new(1.0, Color32::WHITE);
```

- [ ] **Step 3: Build and verify**

```bash
cargo check -p ferrite-egui
```

Expected: compiles cleanly.

- [ ] **Step 4: Rebuild webapp and verify in browser**

```bash
cd crates/ferrite-webapp && trunk build
```

Open http://localhost:8080. Click each tab — the label text should be visible when selected. Click "eSheep" in Sprites — the label should stay readable. Click a pet in Config — its ID should remain visible.

- [ ] **Step 5: Commit**

```bash
git add crates/ferrite-egui/src/ui_theme.rs
git commit -m "fix(webapp): set selection.stroke to white so selected-item text is visible

selection.bg_fill and selection.stroke were both set to the indigo accent
color, making text invisible on selected tabs, gallery items, and list
entries. White text on indigo gives 5.5:1 contrast ratio."
```

---

### Task 2: Make SM editor usable at narrow viewports (B-02)

**Files:**
- Modify: `crates/ferrite-egui/src/sm_editor.rs`

**Root cause:** At viewport ≤474px, the left panel (min_width=160) + right panel (min_width=240) = 400px minimum, leaving only ~74px for the code editor. With egui's internal margins this gets squeezed to ~30px.

**Fix:** Reduce `sm_graph` min_width to 180, and add a `max_width` cap so it doesn't hog space. The center editor gets whatever remains (at least ~130px at 474px viewport), which is usable.

- [ ] **Step 1: Find the right panel declaration in `sm_editor.rs`**

```
crates/ferrite-egui/src/sm_editor.rs  ~line 310
```

Look for:
```rust
egui::SidePanel::right("sm_graph")
    .resizable(true)
    .min_width(240.0)
```

- [ ] **Step 2: Reduce min_width and add default_width**

```rust
egui::SidePanel::right("sm_graph")
    .resizable(true)
    .min_width(160.0)
    .default_width(220.0)
```

- [ ] **Step 3: Also ensure the center panel has a hard minimum by adding it to the editor panel**

Find the center panel (the `egui::CentralPanel` that contains the code editor) in `render_sm_panel`. It's after the two side panels. It doesn't need changes — egui's CentralPanel takes all remaining space.

Actually also find the left browser panel:
```rust
egui::SidePanel::left("sm_browser")
    .resizable(true)
    .min_width(160.0)
```

Change to:
```rust
egui::SidePanel::left("sm_browser")
    .resizable(true)
    .min_width(140.0)
    .default_width(160.0)
```

- [ ] **Step 4: Build and check**

```bash
cargo check -p ferrite-egui
```

Expected: clean compile.

- [ ] **Step 5: Rebuild webapp and verify**

```bash
cd crates/ferrite-webapp && trunk build
```

Open the SM tab. The code editor should now show at least 3-4 characters per line wide, and the TOML should be readable (wrapping but legible). At wider viewports it should work the same as before.

- [ ] **Step 6: Commit**

```bash
git add crates/ferrite-egui/src/sm_editor.rs
git commit -m "fix(webapp): reduce SM editor panel min widths so editor is usable at narrow viewports

sm_graph min_width 240→160, sm_browser min_width 160→140. At 474px viewport
the code editor now gets ~150px instead of ~30px."
```

---

### Task 3: Fix simulation pet position and floor (B-06, B-07, B-08)

**Files:**
- Modify: `crates/ferrite-webapp/src/simulation.rs`

**Root cause:** Pets initialize from `pet_cfg.y` (desktop value: 800) but simulation floor is at `SIM_FLOOR_Y=500`. Pet ends up 300px below the floor. Also, the floor line and pet rendering use absolute pixel values from panel top, but the panel height varies with viewport.

**Fix:**
1. Initialize each pet's `y` to `SIM_FLOOR_Y - pet_h` (just above the floor) instead of `pet_cfg.y`
2. Make `SIM_FLOOR_Y` dynamic: compute it each frame based on actual panel height (75% down)
3. Pass the runtime floor y into `render()` and use it in `tick()`

Since `tick()` and `render()` are called each frame, we compute `floor_y` once in `WebApp::update()` from the egui context and pass it down. But we can't read the panel height before rendering. Simplest approach: store panel height from the previous frame and use it for the current frame's physics.

Actually, even simpler: use a constant that represents the simulation virtual height (not screen pixels), and scale it when rendering. Keep `SIM_FLOOR_Y=500` as a simulation-space constant, and in `render()` compute a scale factor to map simulation space to panel space.

- [ ] **Step 1: Add a scale factor to the render method**

In `simulation.rs`, change `render` to compute a `y_scale` from the available panel height:

```rust
pub fn render(&self, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::TopBottomPanel::top("sim_toolbar").show_inside(ui, |ui| {
            // ... toolbar unchanged ...
        });

        let panel_rect = ui.available_rect_before_wrap();
        let panel_h = panel_rect.height();
        let panel_w = panel_rect.width();

        // Scale simulation coords to panel:
        // simulation runs in SIM_SCREEN_W x (SIM_FLOOR_Y + margin) space
        // map SIM_FLOOR_Y to 85% of panel height
        let y_scale = (panel_h * 0.85) / SIM_FLOOR_Y as f32;
        let x_scale = panel_w / SIM_SCREEN_W as f32;

        // Draw floor line at scaled position
        let floor_y = panel_rect.top() + SIM_FLOOR_Y as f32 * y_scale;
        ui.painter().line_segment(
            [
                egui::pos2(panel_rect.left(), floor_y),
                egui::pos2(panel_rect.right(), floor_y),
            ],
            egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 100, 130)),
        );

        // Draw each pet scaled
        for pet in &self.pets {
            let abs_frame = pet.anim.absolute_frame(&pet.sheet);
            let frame = pet.sheet.frames.get(abs_frame);

            let (frame_w, frame_h) = if let Some(f) = frame {
                (f.w as f32 * pet.scale * x_scale, f.h as f32 * pet.scale * y_scale)
            } else {
                (32.0 * x_scale, 32.0 * y_scale)
            };

            let px = panel_rect.left() + pet.x as f32 * x_scale;
            let py = panel_rect.top() + pet.y as f32 * y_scale;
            let rect = egui::Rect::from_min_size(egui::pos2(px, py), egui::vec2(frame_w, frame_h));

            // ... texture upload and draw unchanged, just use `rect` ...

            ui.painter().text(
                egui::pos2(px + frame_w / 2.0, py + frame_h),
                egui::Align2::CENTER_TOP,
                format!("{} [{}]", pet.id, pet.sm.current_state_name()),
                egui::FontId::proportional(10.0),
                egui::Color32::DARK_GRAY,
            );
        }
    });
}
```

- [ ] **Step 2: Initialize pet y at the floor, not from config**

In `SimulationState::new`, after creating each `PetSimState`, override `y` to `SIM_FLOOR_Y - pet_h`:

```rust
let (_, init_h) = if let Some(frame) = sheet.frames.first() {
    (
        (frame.w as f32 * pet_cfg.scale) as i32,
        (frame.h as f32 * pet_cfg.scale) as i32,
    )
} else {
    (32, 32)
};

pets.push(PetSimState {
    id: pet_cfg.id.clone(),
    x: SIM_SCREEN_W / 4,          // start at 25% of sim width
    y: SIM_FLOOR_Y - init_h,       // just above the floor
    scale: pet_cfg.scale,
    sheet,
    sm,
    anim,
});
```

- [ ] **Step 3: Build and check**

```bash
cargo check -p ferrite-webapp
```

Expected: clean compile.

- [ ] **Step 4: Rebuild webapp and verify**

```bash
cd crates/ferrite-webapp && trunk build
```

Open Simulation tab. The pet should be visible near the middle of the screen, sitting on a floor line. As the browser window resizes, the floor and pet should stay proportionally positioned.

- [ ] **Step 5: Commit**

```bash
git add crates/ferrite-webapp/src/simulation.rs
git commit -m "fix(webapp): scale simulation rendering to panel size and init pet at floor

Previously pet initialized at y=800 (desktop config) while simulation floor
is at SIM_FLOOR_Y=500, putting pet 300px below the floor. Now:
- Pet initializes at SIM_FLOOR_Y - pet_height (just above floor)
- Rendering scales simulation coords to panel dimensions so floor always
  appears at 85% of panel height regardless of viewport size."
```

---

### Task 4: Hide close button in wasm build (B-11)

**Files:**
- Modify: `crates/ferrite-egui/src/app_window.rs`

**Root cause:** The ✕ close button sets `s.should_close = true` which has no effect in the web build (no window to close).

- [ ] **Step 1: Find the close button in `render_app_tab_bar`**

```
crates/ferrite-egui/src/app_window.rs
```

Look for:
```rust
if ui.button("✕").clicked() {
    s.should_close = true;
}
```

- [ ] **Step 2: Gate it behind `#[cfg(not(target_arch = "wasm32"))]`**

```rust
#[cfg(not(target_arch = "wasm32"))]
if ui.button("✕").clicked() {
    s.should_close = true;
}
```

- [ ] **Step 3: Build both targets**

```bash
cargo check -p ferrite-egui
cargo check -p ferrite-webapp --target wasm32-unknown-unknown
```

Expected: both clean.

- [ ] **Step 4: Commit**

```bash
git add crates/ferrite-egui/src/app_window.rs
git commit -m "fix(webapp): hide close button on wasm (no window to close in browser)"
```

---

### Task 5: Open PR

- [ ] **Step 1: Push branch**

```bash
git push -u origin HEAD
```

- [ ] **Step 2: Create PR**

```bash
gh pr create \
  --title "fix(webapp): selected text visibility, SM editor width, simulation floor" \
  --body "$(cat <<'EOF'
## Summary

- Fix invisible selected-item text on tabs, pet list, sprite gallery (selection.stroke was same color as background)
- Reduce SM editor panel min widths so the code editor is usable at narrow viewports
- Fix simulation: pet now initializes above the floor instead of 300px below it; rendering scales to panel size
- Hide ✕ close button on wasm builds (has no effect in browser)

## Test plan
- [ ] Click each tab — label text visible when selected
- [ ] Click eSheep in Sprites — item stays readable
- [ ] SM tab at narrow viewport — code editor shows multiple chars per line
- [ ] Simulation tab — pet visible near center of screen, sitting on floor line

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```
