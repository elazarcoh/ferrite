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
| `tray/` | System tray menus, unified app window with Config/Sprites/SM tabs, sprite editor, SM editor |
| `window/` | HWND creation, per-pixel hit testing (transparent areas click-through), desktop surface detection, bitmap blending |
| `bundle.rs` | `.petbundle` ZIP import/export |
| `event.rs` | `AppEvent` channel between components |

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

## Testing

- Unit tests: inline in modules
- Integration tests: `tests/integration/` — cover animation, behavior/SM transitions, config roundtrip, sprite parsing, hot-reload, window creation, drag/interaction E2E
- No mocking of Windows APIs — integration tests that require a real HWND run on Windows only
