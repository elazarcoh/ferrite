# Ferrite Webapp — Issue Backlog

Discovered via interactive browser inspection on 2026-04-07.

---

## 🔴 Critical Bugs

| # | Issue | Root Cause | File(s) |
|---|-------|-----------|---------|
| B-01 | **Selected item text invisible** — active tab, pet list, sprite gallery items all lose their label text when selected | `vis.selection.stroke` is set to indigo (same as `selection.bg_fill`); text color = background color | `crates/ferrite-egui/src/ui_theme.rs` |
| B-02 | **SM code editor is ~30px wide** — text wraps char-by-char vertically, completely unreadable | `SidePanel::right("sm_graph")` has `min_width(240)` and left panel has `min_width(160)`, together consuming all available space at narrow viewports | `crates/ferrite-egui/src/sm_editor.rs` |
| B-03 | **Sprite editor never loads** — clicking eSheep in gallery highlights it but right panel stays "Select a sprite to edit" | `AppWindowState.sprite_editor` is never set from `None` to `Some`; no code path creates `SpriteEditorViewport` on selection | `crates/ferrite-egui/src/app_window.rs`, `crates/ferrite-webapp/src/app.rs` |
| B-04 | **State Graph always "No valid SM selected"** — even after "New SM" or "Copy Built-in Default" loads content | Graph only renders when `selected_sm.is_some()`; "New SM" sets `selected_sm = None` (unsaved) | `crates/ferrite-egui/src/sm_editor.rs` |
| B-05 | **SM list doesn't show newly created SM** | New SM only appears in `list_names()` after save; no visual indication that you need to save first | `crates/ferrite-egui/src/sm_editor.rs` |

---

## 🟠 Layout / UX Bugs

| # | Issue | Root Cause | File(s) |
|---|-------|-----------|---------|
| B-06 | **Pet starts below the floor** — pet is at y=800 (from desktop config) but simulation floor is at `SIM_FLOOR_Y=500` | Simulation initializes `pet.y` from `pet_cfg.y` (desktop value) instead of placing it at the floor | `crates/ferrite-webapp/src/simulation.rs` |
| B-07 | **Floor line appears in middle of empty space** — the grey horizontal line is at y=500 but canvas is 875px tall, giving a large void below the floor | `SIM_FLOOR_Y` is hardcoded to 500px absolute, not relative to panel height | `crates/ferrite-webapp/src/simulation.rs` |
| B-08 | **Pet barely visible at bottom** — consequence of B-06; most of the simulation is wasted empty space | Same as B-06 | `crates/ferrite-webapp/src/simulation.rs` |
| B-09 | **Tab bar overflows at narrow viewports** — "▶ Simulation" text clips at ~474px | No wrapping/overflow handling in tab bar | `crates/ferrite-egui/src/app_window.rs` |
| B-10 | **Import/Export Bundle buttons feel orphaned** — sitting directly under tab bar with no visual grouping | Rendered as a bare `TopBottomPanel` inside the simulation `CentralPanel` | `crates/ferrite-webapp/src/simulation.rs` |
| B-11 | **"✕" close button does nothing in browser** | Desktop close logic has no web equivalent | `crates/ferrite-egui/src/app_window.rs` |

---

## 🟡 Missing / Incomplete Web Adaptation

| # | Issue | Notes |
|---|-------|-------|
| B-12 | **Config not persisted across reloads** | `config_store` uses `localStorage` but may not load on init |
| B-13 | **`embedded://default` shown raw** in State Machine dropdown | Needs a friendly display-name mapping in `WebSmStorage` |
| B-14 | **X/Y fields in Config expose raw simulation coords** | Y=800 makes no sense to users; label should explain or hide |
| B-15 | **No loading indicator** while WASM initializes | Page shows blank until egui starts |
| B-16 | **No favicon** | Browser tab shows generic icon |
| B-17 | **"Edit…" button on pet does nothing visible** | Desktop opens sprite editor window; no web equivalent |
| B-18 | **No simulation controls** — no pause/play, reset, speed slider | |
| B-19 | **Theme toggle icon (✵/□) not intuitive** | Already uses ☀/☾ in source — may be font rendering issue |
| B-20 | **No error feedback for invalid SM TOML** | Save button shows errors but no inline highlighting |
| B-21 | **Toolbar buttons have no tooltips** in the right-side theme/close buttons | |
| B-22 | **Can't import sprite PNG in webapp** | "New from PNG" button hidden on wasm (no file picker); need a web-compatible flow (e.g. `<input type=file>`) |
| B-23 | **Simulation tab visible in desktop app** | `AppTab::Simulation` and the "▶ Simulation" tab should be `#[cfg(target_arch="wasm32")]`-gated |
| B-24 | **Desktop Sprites tab: clicking sprite shows "Select a sprite to edit"** | `sprite_editor` is set to `None` on selection but desktop caller (`src/tray/app_window.rs`) never recreates it |

---

## PRs Planned

| PR | Fixes | Status |
|----|-------|--------|
| PR-A | B-01, B-02, B-06, B-07, B-08, B-11 | planned |
| PR-B | B-03, B-04, B-05, B-10, B-13 | planned |
| PR-C | B-09, B-12, B-14, B-15, B-16 | planned |
| PR-D | B-17, B-19, B-20 + config panel button layout fix | in progress |
