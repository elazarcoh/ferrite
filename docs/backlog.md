# Ferrite Backlog

Non-asset-related feature backlog, ordered by impact.

---

## 1. SM Import & Per-Pet SM Switching *(in progress)*

**What:** Wire up the unhandled `SMImported`, `SMChanged`, and `SMCollectionChanged`
events. Make SM switching a hot-swap (keep position, no window rebuild) instead of a
full `PetInstance` teardown. Bundle imports auto-assign the bundled SM to the matching
pet. Open config windows refresh their SM dropdown without restart.

**Why it matters:** Without this, importing a bundle with a custom SM requires manually
selecting it from the dropdown (the pet doesn't auto-update), and even then the pet
window flickers and resets position.

**Files:** `crates/ferrite-core/src/sprite/sm_runner.rs`, `src/app.rs`,
`src/tray/app_window.rs`, `src/tray/config_window.rs`

---

## 2. Sprite Tag Mapping UI (Task-13)

**What:** The old tag-map panel in `sprite_editor.rs` was deleted and never replaced.
Custom sprite sheets need a way to map their Aseprite animation tag names to SM state
names. The SM compiler already has fallback-chain logic; this is mostly a UI/wiring task.

**Why it matters:** Without tag mapping, custom sprite sheets with non-standard tag names
(anything other than the built-in set) won't animate correctly under user-defined SMs.

**Files:** `src/tray/sprite_editor.rs`, `crates/ferrite-core/src/sprite/sm_compiler.rs`

---

## 3. Pet-to-Pet Collision / Interaction

**What:** Multiple pets currently ignore each other — no bumping, avoidance, or
interaction. Add AABB overlap detection across `PetInstance`s in `App::update()` and
introduce a `Collide` SM action/interrupt that fires when two pets touch.

**Why it matters:** Multi-pet setups are a core differentiator. Pets that interact with
each other feel alive; pets that phase through each other feel broken.

**Files:** `src/app.rs`, `crates/ferrite-core/src/sprite/sm_runner.rs`,
`crates/ferrite-core/src/sprite/sm_format.rs`

---

## 4. SM Expression Enhancements

**What:** Extend `sm_expr.rs` with new runtime variables:
- `pet_count` — number of active pets on screen
- `other_pet_dist` — pixel distance to nearest other pet
- `on_surface` extensions (surface width, surface label)

Also implement the `distance` field parsing in `sm_compiler.rs:403` (currently marked
`// TODO: not implemented`).

**Why it matters:** These variables unlock richer reactive behaviors (crowd-shy pets,
pets that walk toward each other) without requiring new SM actions.

**Files:** `crates/ferrite-core/src/sprite/sm_expr.rs`,
`crates/ferrite-core/src/sprite/sm_compiler.rs`

---

## 5. SM Debugging UI

**What:** `SMRunner` already has `force_state`, `step_mode`, `step_advance`, and a
10-entry `transition_log`, but none of it is surfaced in the tray UI. Add a debug
panel to the SM tab in the app window showing: current state name, live variable values
(`cursor_dist`, `state_time_ms`, `on_surface`, etc.), and the last 10 transitions with
timestamps.

**Why it matters:** SM authoring is currently a blind process — you write TOML, reload,
and hope. A live debug panel would make iteration dramatically faster for anyone building
custom behaviors.

**Files:** `src/tray/sm_editor.rs`, `src/tray/app_window.rs`
