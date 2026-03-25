# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build --release        # Optimized binary (strips debug info, LTO)
cargo build                  # Debug build (console visible)
cargo run --release          # Run release build
cargo run                    # Run with debug console

cargo test                   # All tests (unit + integration)
cargo test --test integration  # Integration tests only
cargo test <name>            # Single test by name

cargo bench                  # All Criterion benchmarks
cargo bench surfaces         # Single benchmark suite
```

## Architecture

This is a Windows desktop pet application — a transparent, always-on-top window with an animated sprite that walks around the screen, sits on windows, and reacts to user interaction.

### Execution Flow

```
main.rs → App::new() → eframe event loop → App::update() (per frame)
```

- `App` manages multiple `PetInstance`s, config, system tray, and file watcher
- Each `PetInstance` owns its Win32 window, sprite sheet, animation state, and behavior AI
- Events flow through a `crossbeam-channel` (`AppEvent` enum in `event.rs`)

### Key Design Points

**Two rendering layers:**
- `eframe/egui` runs a hidden viewport for UI dialogs (config dialog, sprite editor)
- Each pet lives in its own transparent Win32 layered window rendered via GDI DIB — completely separate from egui

**Physics and behavior** (`sprite/behavior.rs`):
- `BehaviorAi` is a state machine: Idle → Walk/Run → Sit/Sleep, plus Fall/Thrown/Grabbed/Petted
- Gravity = 980 px/s². Surface detection scans actual pixels of other windows
- Facing direction is `Left`/`Right`; per-tag `flip_h` in the sprite sheet determines which direction the sprite natively faces (standard = right, `flip_h=true` means the sheet art faces left and must be flipped to walk right)

**Sprite format** (`sprite/sheet.rs`):
- Aseprite JSON (hash or array format), paired with PNG
- Tags: `idle` (required), `walk`, `run`, `sit`, `sleep`, `wake`, `grabbed`, `petted`, `react`, `fall`, `thrown`
- Tag fields: `direction` (Forward/Reverse/PingPong/PingPongReverse), `flipH` (bool)
- Embedded sprites use `embedded://esheep` path; external sprites use absolute path

**Config** (`config/`):
- Stored at `%LOCALAPPDATA%\ferrite\config.toml`
- `config/watcher.rs` hot-reloads on file change → `AppEvent::ConfigChanged`
- `config/schema.rs`: `Config` (app-level) + `PetConfig` (per-pet, including `tag_map`)

**Win32 window** (`window/`):
- `pet_window.rs`: Layered window with per-pixel alpha via `UpdateLayeredWindow`
- `wndproc.rs`: Message proc — handles drag, throw velocity, click-through hit testing
- `blender.rs`: BGRA premultiplied alpha blending
- `surfaces.rs`: Scans screen pixels to find solid surfaces for landing

**UI** (`tray/`):
- `config_window.rs`: egui dialog for per-pet settings (live-apply via channel)
- `sprite_editor.rs`: Grid/tag editor for defining animations on a spritesheet
- `ui_theme.rs`: Dark/light theme helpers for egui

### Module Map

| Path | Responsibility |
|------|---------------|
| `src/app.rs` | App state, per-frame tick, event dispatch |
| `src/event.rs` | `AppEvent` enum (all inter-thread events) |
| `src/sprite/behavior.rs` | Pet AI state machine + physics |
| `src/sprite/animation.rs` | Frame sequencing, ping-pong |
| `src/sprite/sheet.rs` | Aseprite JSON parser, `SpriteSheet` |
| `src/window/pet_window.rs` | Transparent Win32 window, GDI rendering |
| `src/window/wndproc.rs` | Win32 message handler, drag/throw |
| `src/window/surfaces.rs` | Screen surface collision detection |
| `src/config/schema.rs` | `Config`, `PetConfig` data structures |
| `src/tray/config_window.rs` | egui config dialog |
| `src/tray/sprite_editor.rs` | egui sprite editor |
