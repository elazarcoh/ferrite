# Ferrite Webapp Design

**Date:** 2026-04-06
**Status:** Approved

## Context

Ferrite is a Windows-native desktop pet simulator. All UI and rendering is Win32-specific. There is no way to run, debug, or test the full feature set without a Windows machine with a running desktop. The goal of this webapp is to provide a browser-based environment where all Ferrite features — sprite editing, state machine authoring, pet simulation, and bundle import/export — can be run and debugged without the Win32 layer. The webapp is standalone (not integrated into the existing `ferrite-web` Dioxus website) for now, with the option to integrate later.

## Crate Structure

```
crates/
  ferrite-core/     ← unchanged: pure portable logic (animation, SM, config schema)
  ferrite-egui/     ← NEW: shared egui panels (no Win32 deps)
  ferrite-webapp/   ← NEW: eframe-wasm entry point + webapp-specific glue
  ferrite-web/      ← unchanged: Dioxus public website
src/                ← desktop: Win32 + eframe; refactored to use ferrite-egui
```

### `ferrite-egui` (new shared UI crate)

Extracted from `src/tray/` — contains all egui panel code with no platform dependencies:

| File | Source |
|------|--------|
| `config_panel.rs` | `src/tray/config_window.rs` |
| `sprite_editor.rs` | `src/tray/sprite_editor.rs` |
| `sm_editor.rs` | `src/tray/sm_editor.rs` |
| `sm_highlighter.rs` | `src/tray/sm_highlighter.rs` |
| `app_window.rs` | `src/tray/app_window.rs` (tab bar shell, minus Win32 viewport management) |
| `ui_theme.rs` | `src/tray/ui_theme.rs` |

Dependencies: `egui`, `ferrite-core`. No `windows-sys`, `muda`, `tray-icon`.

Desktop `src/tray/` becomes thin wrappers: viewport lifecycle + system tray, delegating all panel rendering to `ferrite-egui`.

### `ferrite-webapp` (new wasm crate)

- **Target:** `wasm32-unknown-unknown`
- **Framework:** `eframe` with `glow` backend (WebGL), `accesskit` feature enabled
- **Build tool:** `trunk`
- **Entry point:** `WebApp` struct implementing `eframe::App`
  - No `PetWindow`, no Win32, no tray
  - Tabs: Config | Sprites | State Machine | **Simulation** (new)
  - Config persisted to `localStorage` via `web-sys`
  - Pet state ticked each frame via `eframe`'s `request_repaint()`

## Pet Rendering (Simulation Panel)

Pets render inside the egui viewport using `egui::Painter` + `egui::Image` (texture uploaded from the current animation frame's `RgbaImage`).

The Simulation tab shows a "virtual desktop" surface:
- A fixed floor rect at the bottom of the panel
- User-addable draggable surface boxes (rectangles with a drag handle)
- Each pet rendered at its current position as an `egui::Image`
- Mouse click/drag on a pet sends `Grabbed`/`Released` to `SMRunner` (same events as Win32 wndproc)
- Surface list (floor + boxes) fed into `SMRunner::update_env_vars()` each tick

## Import / Export

Uses `rfd` (which has wasm support) for the file picker:

- **Import:** `rfd::AsyncFileDialog` opens a file picker → reads `.petbundle` bytes → same `bundle.rs` ZIP parsing logic as desktop → extracts sprite/SM into in-memory asset store
- **Export:** same ZIP assembly → `web-sys` `URL.createObjectURL` triggers a browser download

The `bundle.rs` ZIP logic moves from `src/bundle.rs` into `ferrite-core` so both the desktop and webapp share the same import/export implementation.

## Config Storage

- Config serialized as TOML (same schema as desktop via `ferrite-core`)
- Stored in `localStorage` under key `ferrite_config`
- Loaded on startup, saved on every change
- No file watcher needed (no hot-reload in browser context)

## Playwright Integration

Two-layer strategy:

### 1. Accessibility tree (UI interaction)
eframe's `accesskit` feature (already enabled on desktop) exposes ARIA nodes to the DOM for every egui widget. Playwright interacts via:
```js
await page.getByRole('tab', { name: 'Sprites' }).click();
await page.getByLabel('Walk speed').fill('120');
```
Requires egui widgets to have meaningful accessible labels (audit existing panels and add `.on_hover_text()` / `.labelled_by()` where missing).

### 2. JS/wasm bridge (state inspection + simulation driving)
A `#[wasm_bindgen]`-exported struct attached to `window.__ferrite`:

```rust
#[wasm_bindgen]
pub struct FerriteBridge { /* Arc to shared app state */ }

#[wasm_bindgen]
impl FerriteBridge {
    pub fn get_pet_state(&self, id: &str) -> JsValue;   // → JSON
    pub fn get_config(&self) -> JsValue;                // → JSON
    pub fn inject_event(&self, event_json: &str);       // drive simulation programmatically
}
```

Playwright tests use it as:
```js
const state = await page.evaluate(() => window.__ferrite.get_pet_state("pet0"));
expect(state.sm_state).toBe("walk");
```

### 3. Visual regression
Playwright screenshots the simulation canvas element; compared against baseline PNGs using pixelmatch or Playwright's built-in `toHaveScreenshot`.

## Build & Deployment

```bash
# Dev
cd crates/ferrite-webapp
trunk serve   # hot-reload at localhost:8080

# Production
trunk build --release   # outputs dist/ with index.html + .wasm + .js
```

`Trunk.toml` in `crates/ferrite-webapp/` configures wasm-bindgen flags and asset copying (embedded sprites).

Output is a static bundle deployable to GitHub Pages, a CDN, or any static host — separate from the `ferrite-web` Dioxus site.

## Testing Strategy

| Layer | Tool | Coverage |
|-------|------|----------|
| Core logic | Rust unit tests (`ferrite-core`) | Animation, SM, config schema |
| egui panels | `egui_kittest` | Widget-level panel tests without a browser |
| Webapp E2E | Playwright (`tests/webapp/`) | UI interaction, state assertions, import/export, visual regression |

**CI additions:**
- `trunk build --release` step
- `npx playwright test` step (headless browser + `trunk serve`)

**Key Playwright test scenarios:**
1. Add a pet, assign embedded sprite, assert it appears in simulation
2. Edit walk speed, assert `SMRunner` env var updates
3. Import a `.petbundle` fixture, assert sprite and SM loaded
4. Export bundle, download, verify ZIP contents
5. Visual regression: screenshot simulation after 60 frames of walk state
