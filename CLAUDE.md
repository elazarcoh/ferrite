# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build
cargo build
cargo build --release

# Run
cargo run
RUST_LOG=debug cargo run

# Test
cargo test
cargo test --test integration   # integration tests only

# Lint (strict — warnings are errors, dead_code allowed)
cargo clippy -- -D warnings -A dead-code

# Benchmarks
cargo bench
```

To run a single test: `cargo test <test_name>` (e.g., `cargo test test_animation`).

## Commit Convention

Commits must follow Conventional Commits format, enforced by pre-commit hook and CI:
```
type(scope): description
# e.g.: feat(window): add drag, fix(config): handle invalid paths
```

## Architecture

Ferrite is a Windows-native desktop pet simulator. Animated pets live on the desktop; users interact via system tray. Config is stored at `%LOCALAPPDATA%\ferrite\config.toml`.

**Crate structure:**

```
crates/
  ferrite-core/     pure portable logic (animation, SM, config schema, bundle ZIP)
  ferrite-egui/     shared egui panels (no Win32 deps) — used by desktop + webapp
  ferrite-webapp/   eframe-wasm browser app (trunk build, wasm32-unknown-unknown)
  ferrite-web/      Dioxus public website (unchanged)
src/                desktop: Win32 + eframe; thin wrappers over ferrite-egui panels
```

**Entry point flow:**
1. `main.rs` → initializes logger, creates `App`
2. `app.rs::App::new()` → loads config, spawns `PetInstance`s, starts file watcher
3. eframe event loop calls `App::update()` each frame
4. Each `PetInstance::tick()` advances animation/state machine, triggers render

**Key modules:**

| Module | Responsibility |
|--------|----------------|
| `app.rs` | Runtime loop, `PetInstance` coordination, event dispatch |
| `sprite/` | Spritesheet loading, `AnimationState` (frame timing), state machine compiler + runner (`SMRunner`), TOML DSL (`SmFile`), sprite/SM galleries |
| `config/` | `PetConfig`/`Config` load/save, hot-reload via file watcher |
| `tray/` | System tray menus, thin wrappers delegating to `ferrite-egui` panels |
| `window/` | HWND creation, per-pixel hit testing (transparent areas click-through), desktop surface detection, bitmap blending |
| `event.rs` | `AppEvent` channel between components |
| `ferrite-core::bundle` | `.petbundle` ZIP import/export (shared with webapp) |
| `ferrite-egui` | All egui panel logic: `app_window`, `config_panel`, `sprite_editor`, `sm_editor`, `sm_highlighter`, `ui_theme`; platform I/O via `SmStorage` / `SheetLoader` traits |
| `ferrite-webapp` | Browser DevTools: `WebApp` (eframe), `WebSmStorage`, `SimulationPanel`, `FerriteBridge` (`window.__ferrite` JS API) |

**Sprite/State machine pipeline:**
```
PetConfig.sheet_path → SpriteSheet (Aseprite JSON/PNG)
                     → AnimationState.tick() → current frame
                     → PetWindow.render() → blit to screen

SmFile (TOML) → sm_compiler → CompiledSM → SMRunner.tick() → drives AnimationState
```

**Asset references:** `embedded://esheep` (bundled via `rust-embed`) or absolute filesystem paths.

**Windows API surface:** `windows-sys` for HWND management, alpha blending (`UpdateLayeredWindow`), per-pixel regions (`SetWindowRgn`), monitor/surface enumeration.

**UI:** `egui`/`eframe` with WGPU renderer. The main window is hidden; all UI surfaces are egui windows or the system tray.

**Webapp (browser DevTools):**

```bash
# Dev server (requires trunk ≥ 0.21 + wasm32 target)
rustup target add wasm32-unknown-unknown
cargo install trunk
cd crates/ferrite-webapp && trunk serve   # → http://localhost:8080

# Production build
cd crates/ferrite-webapp && trunk build --release   # outputs dist/

# Fallback build (if trunk unavailable — e.g. no clang)
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version $(grep -A1 'name = "wasm-bindgen"' Cargo.lock | grep version | cut -d'"' -f2)
cd crates/ferrite-webapp && bash build.sh          # dev build → dist/
cd crates/ferrite-webapp && bash build.sh release  # release build → dist/
python -m http.server 8080 --directory dist        # serve locally

# Playwright E2E tests (requires a pre-built dist/)
cd tests/webapp && npm ci && npx playwright install chromium
npx playwright test
```

The webapp exposes `window.__ferrite` in the browser console:
```js
window.__ferrite.get_state()         // → {pets: [...], dark_mode: true}
window.__ferrite.get_pet_state("id") // → {id, x, y, sm_state, animation_tag}
window.__ferrite.inject_event('{"type":"grab","pet_id":"esheep"}')
```

## Testing

- Unit tests: inline in modules
- Integration tests: `tests/integration/` — cover animation, behavior/SM transitions, config roundtrip, sprite parsing, hot-reload, window creation, drag/interaction E2E
- No mocking of Windows APIs — integration tests that require a real HWND run on Windows only
