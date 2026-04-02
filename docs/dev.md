# Ferrite — Developer Guide

## Prerequisites

- Rust stable (edition 2024)
- Windows (the desktop crate uses Win32 APIs — `windows-sys`, DX12/Vulkan)
- Git with pre-commit hooks (installed automatically via `cargo-husky` on first `cargo test`)

For the web crate only:
- `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- `dioxus-cli`: `cargo install dioxus-cli --version "^0.6" --locked`
- Node.js 20+ (for Tailwind CSS build)

---

## Build

```bash
cargo build                    # debug — target/debug/ferrite.exe
cargo build --release          # release — target/release/ferrite.exe

cargo build --package ferrite-core   # core library only (cross-platform)
```

Run directly:
```bash
cargo run
RUST_LOG=debug cargo run       # with logging
```

---

## Test

```bash
cargo test                                    # all tests
cargo test --package ferrite                  # desktop crate
cargo test --package ferrite-core             # core library
cargo test --test integration                 # integration suite only
cargo test --test e2e                         # e2e suite only
cargo test test_animation                     # single test by name
```

**Suites:**

| Suite | Path | What it covers |
|-------|------|----------------|
| Unit | inline in each module | Pure logic, no I/O |
| Integration | `tests/integration/` | Windows APIs, config I/O, hot-reload, sprite editor, stress |
| E2E | `tests/e2e/` | Full pet lifecycle, drag/click, surface detection, config reload |
| ferrite-core | `crates/ferrite-core/src/` | Animation, SM compiler, sheet parsing |

---

## Lint

```bash
cargo clippy --package ferrite -- -D warnings -A dead-code
cargo clippy --package ferrite-core -- -D warnings -A dead-code
```

Warnings are errors in CI. Dead code is explicitly allowed.

---

## Release

Releases are automated via two workflows:

1. **`release-please.yml`** — Runs on every push to `main`. Opens a release PR that bumps `Cargo.toml` version and updates `CHANGELOG.md` based on commit history.
2. **`release-artifacts.yml`** — Runs when a `v*` tag is pushed. Builds `--release`, zips to `ferrite-<version>-windows.zip`, uploads to GitHub Release.

**To ship a release:** merge the release PR created by release-please. The tag is created automatically and artifacts are built.

---

## Commit Convention

Enforced by CI (`commit-lint.yml`):

```
type(scope): description
```

Types: `feat` `fix` `docs` `refactor` `test` `perf` `build` `ci` `chore`

```
feat(window): add drag support
fix(config): handle missing path gracefully
docs: add dev guide
```

Scope is optional but encouraged for larger codebases.

---

## Code Structure

```
src/
  main.rs            — entry point: init logger, create App
  app.rs             — runtime loop, PetInstance coord, event dispatch
  event.rs           — AppEvent enum (channel between components)
  version.rs         — ENGINE_VERSION constant
  bundle.rs          — .petbundle ZIP import/export
  assets/            — rust-embed setup (Assets struct)
  config/
    mod.rs           — load/save/config_path(), hot-reload watcher
    schema.rs        — Config, PetConfig structs (re-exported from ferrite-core)
    watcher.rs       — notify-based file watcher → ConfigReloaded event
  sprite/
    mod.rs           — re-exports from ferrite-core + desktop-only mods
    editor_state.rs  — mutable sprite state for the in-app editor
    sm_gallery.rs    — list/load/save state machine files from appdata
    sprite_gallery.rs — list/load embedded + installed spritesheets
  tray/
    app_window.rs    — unified egui window (Pets / Config / Sprites / SM tabs)
    config_window.rs — config tab UI
    sprite_editor.rs — frame/tag editor, animation preview, export
    sm_editor.rs     — SM source editor with live debug overlay
    sm_highlighter.rs — syntax highlighting for .petstate files
    ui_theme.rs      — shared egui style helpers
  window/
    pet_window.rs    — HWND creation, UpdateLayeredWindow render loop
    blender.rs       — blit_frame(): RGBA→BGRA, scale, flip, premultiply
    surfaces.rs      — desktop surface / floor detection
    wndproc.rs       — per-pixel hit testing (click-through transparent areas)

crates/ferrite-core/src/
  sprite/
    animation.rs     — AnimationState: frame timing, ping-pong, tag switching
    sheet.rs         — SpriteSheet: Aseprite JSON/PNG parser, tag/frame lookup
    sm_compiler.rs   — SmFile → CompiledSM, validation, error types
    sm_runner.rs     — SMRunner: tick(), physics (fall/throw/grab), transitions
    sm_expr.rs       — condition expression parser + evaluator
    sm_format.rs     — TOML deserialization structs for .petstate files
  config/schema.rs   — Config, PetConfig (source of truth)
  version.rs         — ENGINE_VERSION

crates/ferrite-web/src/
  app.rs             — Dioxus Router setup
  pages/             — Home, GuideIndex, GuidePage
  components/        — NavLayout, Footer, PetCanvas (canvas animation)
  pet/               — WASM RAF loop, renderer, state struct

assets/
  esheep.json/png    — built-in sheep sprite (embedded in binary)
  default.petstate   — built-in state machine (embedded in binary)
  test_pet.*         — minimal sprite used by tests
  README.md          — Aseprite export instructions, tag names

.github/workflows/
  ci.yml             — clippy + tests on Windows
  pages.yml          — build + deploy ferrite-web to GitHub Pages
  release-please.yml — auto release PR + changelog
  release-artifacts.yml — build zip on tag push
  commit-lint.yml    — enforce semantic commit format on PRs
```

---

## Editing Defaults

### Default pet behavior (`assets/default.petstate`)

The built-in state machine. Controls idle/walk/sit/sleep/petted/grabbed transitions.

```toml
[states.idle]
action = "idle"
transitions = [
  { goto = "walk",  weight = 45, after = "1s-3s" },
  { goto = "sit",   weight = 20, after = "1s-3s" },
  ...
]
```

State machine language reference — key fields:
- `action` — one of: `idle walk run sit jump float follow_cursor flee_cursor grabbed fall thrown`
- `dir` — `left` / `right` / `random` (walk/run only)
- `after` — delay before transition fires: `"2s"`, `"1s-3s"` (random range)
- `condition` — expression: `cursor_dist < 100`, `state_time > 5s`, `on_surface`
- `weight` — relative probability for weighted-random transitions
- `steps` — composite state: list of sub-states to run in sequence
- `required = true` — state must exist (grabbed, fall, thrown, idle)
- `fallback` — tag to use for animation if no matching tag in spritesheet

Interrupt events (`[interrupts]` / `[states.X.interrupts]`):
- `grabbed`, `petted`, `wake` — fired by engine or user interaction

### Default pet config

Defined in `crates/ferrite-core/src/config/schema.rs` (`PetConfig::default()`):

```toml
id           = "esheep"
sheet_path   = "embedded://esheep"
state_machine = "embedded://default"
x = 100
y = 800
scale = 2.0          # fractional values supported (e.g. 1.5)
walk_speed = 80.0    # pixels per second
```

The live config file is `%LOCALAPPDATA%\ferrite\config.toml`. Editing it while the app runs triggers hot-reload.

### Adding a new embedded sprite

1. Add `yourpet.json` + `yourpet.png` to `assets/`
2. Reference as `sheet_path = "embedded://yourpet"` in config or tests
3. `rust-embed` picks it up automatically at compile time

---

## The .petbundle Format

ZIP archive containing:
```
bundle.toml        — name, author, version, recommended_sm
sprite.json        — Aseprite spritesheet
sprite.png         — texture
behavior.petstate  — optional state machine
```

Import/export via `src/bundle.rs` (`import()` / `export()`). The sprite editor UI exposes both.

---

## Hot-Reload

Config changes are detected automatically:

1. `spawn_watcher(path, tx)` starts a `notify` watcher on `config.toml`
2. Modify/Create events trigger `AppEvent::ConfigReloaded(Config)` on the channel
3. `App::update()` receives the event and re-initialises `PetInstance`s in place

To test: edit `%LOCALAPPDATA%\ferrite\config.toml` while the app is running.

---

## Web / GitHub Pages

The landing page is in `crates/ferrite-web/`. It uses Dioxus 0.6 + Tailwind and compiles to WASM.

```bash
cd crates/ferrite-web
dx serve                        # local dev server
dx build --release --platform web   # production build → dist/
```

GitHub Pages deploys automatically on push to `main` via `.github/workflows/pages.yml`.
Enable once in repo Settings → Pages → Source: **GitHub Actions**.
