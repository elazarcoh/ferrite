# User-Defined State Machines

**Date:** 2026-03-23
**Status:** Draft

---

## Context

The pet's behavior is currently a hardcoded Rust state machine in `behavior.rs` — fixed states (Idle, Walk, Run, Sit, Sleep, …), fixed probabilities, fixed timers. Users cannot change how their pet behaves without editing source code.

This design replaces the hardcoded behavior with a **data-driven state machine system**: behavior is defined in text files (`.petstate`), editable in-app, shareable with other users, and compiled to an efficient runtime representation at load time. The physics engine and Win32 rendering are untouched; only the behavior orchestration layer changes.

---

## Goals

- Users define custom pet behaviors without writing Rust
- State machines are text files — human-readable, diffable, safely inspectable
- Share SMs as standalone `.petstate` files or bundled with a sprite as `.petbundle`
- Any SM can be paired with any sprite sheet (via an alignment/alias layer)
- Broken SMs never crash a running pet; errors are caught at save time
- Runtime performance is equivalent to or better than the current hardcoded logic

## Non-Goals

- Arbitrary scripting / code execution (no Lua, WASM, eval)
- Online marketplace (file-based sharing only, v1)
- Cross-pet interaction (states reacting to other pets on screen)
- Audio or visual effects per state

---

## Architecture Overview

```
.petstate file  ──► [SM Parser + Validator] ──► [SM Compiler] ──► CompiledSM (in memory)
                                                                         │
                                                              SMRunner::tick() replaces BehaviorAi::tick()

.petbundle ──► [Bundle Importer] ──► sprite gallery entry
                                 └─► SM collection entry
                                 └─► "recommended SM" association stored with sprite
```

**What changes:**
- `BehaviorAi` in `behavior.rs` is replaced by `SMRunner` (new file: `sprite/sm_runner.rs`)
- `PetConfig.tag_map: AnimTagMap` is replaced by `PetConfig.state_machine: String` (path or `"embedded://default"`)
- `AnimTagMap` struct is removed; tag resolution moves to `smMappings` in the sprite JSON (see §Sprite Alignment)
- `AppEvent` gains: `SMImported`, `SMChanged`, `BundleImported`
- Built-in behavior becomes `embedded://default` — a read-only `.petstate` embedded in the binary, copyable as a starting point
- `myPetTagMap` in sprite JSON is **deprecated and removed**; `smMappings` replaces it (see §Migration Notes)

**What does not change:**
- Win32 window, GDI rendering, surface detection — untouched
- Sprite sheet JSON/PNG format (extended with `smMappings`, see §Sprite Alignment)
- Config hot-reload via file watcher
- Physics engine (gravity, throw, surface snap) — action primitives call into it

**`ActiveState` replaces `BehaviorState`:**
```rust
pub enum ActiveState {
    Named(String),                         // executing a data-defined state
    Fall { vy: f32 },                      // engine physics primitive
    Thrown { vx: f32, vy: f32 },           // engine physics primitive
    Grabbed { cursor_offset: (i32, i32) }, // engine physics primitive
}
```

---

## Storage Paths

All user-managed assets live under `%LOCALAPPDATA%\my-pet\`:

| Path | Contents |
|---|---|
| `%LOCALAPPDATA%\my-pet\config.toml` | App config (existing) |
| `%LOCALAPPDATA%\my-pet\state_machines\` | Live `.petstate` files |
| `%LOCALAPPDATA%\my-pet\state_machines\drafts\` | `.draft.petstate` files |
| `%LOCALAPPDATA%\my-pet\sprites\` | Imported sprite JSON + PNG pairs |
| `%LOCALAPPDATA%\my-pet\sprites\gallery.toml` | Sprite gallery metadata (recommended SM associations) |

`gallery.toml` stores per-sprite metadata:
```toml
[[sprites]]
id             = "lazy-cat"
json_path      = "lazy-cat.json"
png_path       = "lazy-cat.png"
recommended_sm = "lazy-cat-behavior"   # SM name (not path); omitted if none
```

---

## Engine Version

`src/version.rs` (new file):
```rust
pub const ENGINE_VERSION: &str = "1.0";
```

Used by the SM compiler to validate `engine_min_version` in `[meta]`.

---

## The `.petstate` File Format

TOML text file. The SM author defines states, transitions, and interrupt handling.

### Full annotated example

```toml
[meta]
name               = "Default Pet"
version            = "1.0"
engine_min_version = "1.0"       # rejected at load if engine is older
default_fallback   = "idle"      # fallback for optional states with no explicit fallback
                                  # must itself be a required state

# ── Global interrupt table ──────────────────────────────────────────────────
# Fires from ANY state unless a per-state override blocks or redirects it.
[interrupts]
grabbed     = { goto = "grabbed" }
petted      = { goto = "petted" }
cursor_near = { goto = "alert", condition = "cursor_dist < 80" }
# built-in events also available: "released" (fired by engine after grabbed ends)
# → typically goes to "fall" or "thrown" via engine logic, not the SM

# ── Atomic states ────────────────────────────────────────────────────────────

[states.idle]
required = true
action   = "idle"
transitions = [
  { goto = "walk",  weight = 45, after = "1s..3s" },
  { goto = "sit",   weight = 20, after = "1s..3s" },
  { goto = "sleep", weight = 15, after = "15s"    },
  # weights are unnormalized relative values; sum must be > 0
  # a single-transition state with no weight always fires when `after` elapses
]

[states.walk]
required = true
action   = "walk"
dir      = "random"
distance = "200px..800px"
# if distance is exhausted before screen edge: fire transitions
# if screen edge is hit mid-walk: stop early, fire transitions immediately
transitions = [{ goto = "idle" }]

[states.moonwalk]
required      = false
fallback      = "walk"
action        = "walk"
dir           = "random"
speed         = 60
gravity_scale = 0.2            # floaty feel; multiplies engine gravity
transitions   = [{ goto = "idle", after = "2s..4s" }]

[states.sleep]
required = false
action   = "idle"
interrupts.cursor_near = { ignore = true }     # block this global interrupt
interrupts.petted      = { goto = "startled" } # override global for this state
transitions = [{ goto = "wake_up", condition = "state_time > 20s" }]

# ── Composite state (sequence of steps) ─────────────────────────────────────
# Steps are private sub-states: not addressable by `goto` from outside the composite.
# External interrupts can still fire during any sub-step.
# Last step's completion triggers the composite's own transitions.

[states.wake_up]
required = false
steps    = ["stir", "yawn", "stretch"]  # references top-level states; can be reused elsewhere
transitions = [{ goto = "idle" }]

[states.stir]
action   = "idle"
duration = "400ms"

[states.yawn]
action   = "idle"
duration = "700ms"

[states.stretch]
action   = "idle"
duration = "600ms"

# ── Engine-owned physics states ───────────────────────────────────────────────
# Must be present; action type is fixed; transitions out are user-defined.

[states.grabbed]
required = true
action   = "grabbed"    # engine: follow cursor, accumulate release velocity
transitions = []        # engine fires "released" event → engine transitions to fall/thrown

[states.fall]
required = true
action   = "fall"       # engine: gravity integration
transitions = [{ goto = "idle", condition = "on_surface" }]

[states.thrown]
required = true
action   = "thrown"     # engine: ballistic trajectory
transitions = [{ goto = "fall", condition = "on_surface" }]

# ── Interrupt-return states ───────────────────────────────────────────────────

[states.petted]
required = false
fallback = "idle"
action   = "idle"
duration = "600ms"
transitions = [{ goto = "$previous" }]
# $previous = the Named state active when the interrupt fired
# if no previous state exists (edge case: interrupt fires at startup) → default_fallback

[states.alert]
required = false
fallback = "idle"
action   = "idle"
duration = "800ms"
transitions = [{ goto = "$previous" }]
```

### Action primitives

| Name | Physics | Parameters |
|---|---|---|
| `idle` | gravity + surface snap, no movement | — |
| `walk` | gravity + surface snap, horizontal movement | `dir` (left/right/random), `speed`, `distance` |
| `run` | same as walk, default speed = `2× walk_speed` if no `speed` override | same as walk |
| `sit` | alias for idle (different animation tag) | — |
| `jump` | upward velocity then fall | `vy` |
| `float` | no gravity, free 2D movement; clamps at screen edges | `vx`, `vy` |
| `follow_cursor` | moves toward cursor, gravity off | `speed` |
| `flee_cursor` | moves away from cursor, gravity off | `speed` |
| `grabbed` | engine-owned: follow cursor, accumulate velocity | — |
| `fall` | engine-owned: gravity integration | — |
| `thrown` | engine-owned: ballistic trajectory | — |

**`float` boundary:** clamps position at screen edges (no bounce). `vx`/`vy` continue to apply; the pet stays at the edge until a condition or timer fires a transition.

**`walk` + screen edge:** if a screen edge is hit before the declared `distance` is consumed, the walk stops immediately and transitions fire. No auto-reverse (current behavior) — the SM can add a reverse-walk transition explicitly.

**Physics overrides available on all primitives:** `gravity_scale` (float, multiplier), `speed` (px/s override).

**Weight semantics:** transition weights are unnormalized relative values. `{ weight=45 }` and `{ weight=20 }` mean 45/65 and 20/65 probability respectively. Sum must be > 0 (compiler error if all weights are 0 in a weighted group). A transition with no `weight` is a deterministic "always" transition, valid only when it is the sole transition in the list.

### Condition expression language

Simple infix expressions evaluated against a context snapshot. Parsed to AST at compile time; no string evaluation at runtime. The AST node types are: `Literal`, `Variable`, `QualifiedVariable`, `FunctionCall`, `BinaryOp`, `UnaryOp` — future variables and functions slot into existing node types without grammar changes.

**Available variables (v1):**

| Variable | Type | Description |
|---|---|---|
| `cursor_dist` | float (px) | Distance from pet center to cursor |
| `state_time` | duration | Time spent in the current state (resets on every state entry) |
| `on_surface` | bool | Pet is resting on a surface |
| `pet.x`, `pet.y` | float (px) | Pet position |
| `pet.vx`, `pet.vy` | float (px/s) | Pet velocity components (positive = right/down) |
| `pet.v` | float (px/s) | Pet velocity magnitude (`sqrt(vx²+vy²)`) |
| `screen.w`, `screen.h` | float (px) | Screen dimensions |
| `time.hour` | int (0–23) | Current hour |
| `input.focused_app` | string | Title of focused window |

**`near_edge` — parameterized qualifier:**

`near_edge` is a qualified variable, not a plain bool. The qualifier specifies axis and/or threshold:

| Expression | Meaning |
|---|---|
| `near_edge` | within 20px of any screen edge |
| `near_edge.80px` | within 80px of any edge |
| `near_edge.x` | within 20px of left or right edge |
| `near_edge.y` | within 20px of top or bottom edge |
| `near_edge.x.70px` | within 70px of left or right edge |

The parser reads the dotted suffix as qualifier tokens; unrecognised qualifiers are a compile-time error (future qualifiers can be added without breaking existing SMs that don't use them).

**Functions (v1 — whitelist enforced at compile time):**

| Function | Description |
|---|---|
| `abs(x)` | Absolute value |
| `min(a, b)` | Minimum of two values |
| `max(a, b)` | Maximum of two values |

The AST supports `FunctionCall` nodes today. Future functions (`sin`, `cos`, `clamp`, …) are added to the whitelist without grammar changes. Unknown function names are compile-time errors, so SM authors see clear messages rather than silent wrong results.

**Operators:** `<`, `>`, `<=`, `>=`, `==`, `!=`, `and`, `or`, `not`
**Literals:** numbers, durations (`10s`, `500ms`), strings (`"code.exe"`)

### Composite states and step references

A composite state's `steps` array lists **any declared state by name** — there is no private/public distinction. Any state can be both independently transitioned into and used as a step inside a composite routine.

```toml
# A normal state, independently reachable
[states.running]
required = true
action   = "run"
dir      = "random"
distance = "200px..600px"
transitions = [{ goto = "idle" }]   # normal outbound transition

# A routine that reuses existing states as steps
[states.morning_routine]
required = false
steps    = ["running", "sleep", "eat"]  # references existing top-level states
transitions = [{ goto = "idle" }]       # fires after "eat" completes
```

**Step execution semantics:** when a state runs as a step inside a composite, its normal outbound `transitions` are suppressed. The composite advances to the next step when the state reaches natural completion (timer, distance, or condition met). The last step's completion fires the composite's own `transitions`.

**Interrupts during a step** fire normally and interrupt the whole composite — `$previous` is set to the composite's name.

**Constraints (v1):**
- No nested composites (`steps` may not reference a state that itself has `steps`)
- Cycle detection: compiler errors if a `steps` chain references itself directly or transitively

### Required / optional states

- SM must declare at least one `required = true` state (compiler error if none)
- A sprite+SM pairing is valid only if all `required = true` states resolve to a sprite tag
- Optional states with no resolution are silently replaced by their `fallback` at compile time — zero runtime cost
- `default_fallback` in `[meta]` applies to any optional state that omits an explicit `fallback`; it must be a `required = true` state

### External trigger events

The SM interrupt table supports these event names:

| Event | Fired by |
|---|---|
| `grabbed` | User starts dragging the pet |
| `released` | User releases the pet (engine handles → fall/thrown; SM may also react) |
| `petted` | User clicks an opaque pixel |
| `wake` | Previously: `ai.wake()` — now a first-class interrupt event |
| `react` | Previously: `ai.react()` — now a first-class interrupt event |
| `cursor_near` | Evaluated each tick via `condition` field |
| `cursor_far` | Evaluated each tick via `condition` field |
| `app_focused` | Evaluated each tick via `condition = "input.focused_app == \"code.exe\""` |
| `time_of_day` | Evaluated each tick via `condition = "time.hour >= 22"` |

`wake` and `react` replace the old `BehaviorAi::wake()` / `BehaviorAi::react()` methods. Call sites in `app.rs` that previously called `ai.wake()` will instead send an interrupt event through `SMRunner::interrupt("wake")`.

### Versioning

- `engine_min_version` compared against `ENGINE_VERSION` constant in `src/version.rs`
- Load rejected if SM requires a newer engine, with a clear error message
- No automatic migration in v1

### SM name uniqueness

The `name` field in `[meta]` is used as the key for `smMappings` in sprite JSONs. The SM collection must enforce uniqueness: if an SM is saved or imported with a `name` that already exists in the collection (under a different filename), the app warns: *"An SM named 'X' already exists. Rename, replace, or cancel?"* This prevents ambiguous `smMappings` lookups at runtime.

---

## `SMRunner` Interface

`SMRunner` lives in `src/sprite/sm_runner.rs` and replaces `BehaviorAi`.

```rust
pub struct SMRunner {
    sm: Arc<CompiledSM>,
    active: ActiveState,
    previous_named: Option<String>, // last Named state before an interrupt; used by $previous
    state_time_ms: u32,             // ms in current state; reset on every state entry
    rng: u64,                       // same LCG as current BehaviorAi
}

impl SMRunner {
    /// Called once per frame from PetInstance::tick().
    /// Returns the sprite tag name to display (resolved via smMappings).
    pub fn tick(
        &mut self,
        delta_ms: u32,
        x: &mut i32,
        y: &mut i32,
        screen_w: i32,
        pet_w: i32,
        pet_h: i32,
        floor_y: i32,
        sheet: &SpriteSheet,       // replaces tag_map param; smMappings read from here
    ) -> &str;                     // resolved sprite tag name

    /// Returns the current facing direction for compute_flip() in app.rs.
    pub fn current_facing(&self) -> Option<Facing>;
    // Returns Some(facing) when active state uses walk/run/follow_cursor/flee_cursor;
    // None otherwise (caller treats as no-flip).
    // Facing for walk/run: determined by the `dir` field resolved this frame (left/right/random).
    // Facing for follow_cursor: sign of (cursor_x - pet_x) this frame.
    // Facing for flee_cursor: opposite sign of (cursor_x - pet_x) this frame.
    // When mid-step inside a composite state, facing is determined by the active sub-step's action.

    /// Fire an interrupt event by name (e.g., "grabbed", "petted", "wake", "react").
    pub fn interrupt(&mut self, event: &str, cursor_offset: Option<(i32, i32)>);

    /// Force the runner into a named state (debug tool, SM editor only).
    pub fn force_state(&mut self, state: &str);

    /// Release a forced state; resume normal SM execution.
    pub fn release_force(&mut self);

    /// Grab / release (maps to engine physics states directly).
    /// These are the SOLE entry points for physics-state transitions.
    /// The interrupt table entries "grabbed" and "released" are SM-layer aliases:
    /// grab() internally transitions the SM via interrupt("grabbed") AND sets
    /// ActiveState::Grabbed. Do NOT call interrupt("grabbed") directly from app.rs.
    pub fn grab(&mut self, cursor_offset: (i32, i32));
    pub fn release(&mut self, velocity: (f32, f32));
}
```

**`compute_flip()` in `app.rs`** is updated to call `runner.current_facing()` instead of pattern-matching on `BehaviorState` variants. No other rendering code changes.

**`$previous` resolution:** when an interrupt fires on a `Named` active state, `previous_named` is set to that state's name before transitioning. If the runner is mid-step inside a composite state when the interrupt fires, `previous_named` is set to the **composite state's name** (not the sub-step name — sub-steps are private). If the interrupt fires on a physics state (`Fall`, `Thrown`, `Grabbed`), `previous_named` is unchanged (the last Named state is preserved). When `$previous` is evaluated and `previous_named` is `None` (startup edge case), `default_fallback` is used.

---

## SM Compiler

Runs once at pet load and at SM editor save time. Result is a `CompiledSM` held in memory.

**Compilation steps:**
1. Parse TOML → schema validation
2. Build `HashMap<String, CompiledState>` (O(1) state lookup at runtime)
3. Verify `engine_min_version` ≤ `ENGINE_VERSION` → error with message if not
4. Verify at least one `required = true` state exists → error
5. Verify `default_fallback` names a `required = true` state → error
6. Verify all `goto` targets exist → error
7. Resolve `steps` chains to index arrays; detect cycles and nested composites → error
8. Verify all three engine primitives (`grabbed`, `fall`, `thrown`) are present → error
9. Verify all `fallback` fields name existing required states → error
10. Parse all condition expressions → expression AST; validate variable names → error on unknown variable
11. Pre-sort weighted transition tables; verify no weighted group has sum = 0 → error
12. Resolve `smMappings` for the paired sprite; verify all required states resolve → error if not

**Runtime tick does only:**
- One `HashMap` lookup for current state
- One `u32` timer comparison
- One pre-compiled AST expression eval (no string parsing)
- One weighted random pick from a pre-sorted array

---

## Draft System & Save-Time Validation

The same validation pipeline runs at **save time in the SM editor**, not only at load.

### Save behavior

| Validation result | Outcome |
|---|---|
| ✓ All checks pass | Saved to `state_machines/<name>.petstate`; live in SM collection |
| ✗ Any check fails | Saved to `state_machines/drafts/<name>.draft.petstate`; not loadable |

**Drafts:**
- Not shown in SM selection list or bundle export dialog
- Visible only in SM editor's draft section (dimmed, draft icon)
- Fully editable; saving a passing draft promotes it to `state_machines/` and removes the draft file

### Hot-reload (external edit)
- Re-validate on disk change (same `notify` watcher as config)
- Valid → recompile and swap `SMRunner`'s `Arc<CompiledSM>` live
- Invalid → toast error; keep previous valid SM running

---

## The `.petbundle` Format

A ZIP file. Text files inside remain human-readable with any ZIP tool.

```
my-lazy-cat.petbundle   (ZIP)
├── bundle.toml
├── sprite.json
├── sprite.png
└── behavior.petstate   (optional — sprite-only bundles omit this)
```

```toml
# bundle.toml
[bundle]
name           = "Lazy Cat"
author         = "someone"
version        = "1.0"
recommended_sm = "behavior.petstate"  # filename within the bundle; omitted if sprite-only
```

**Use cases:**

| Bundle contents | Use case |
|---|---|
| sprite.json + sprite.png | Sprite-only distribution (replaces two-file sharing) |
| sprite.json + sprite.png + behavior.petstate | Full personality package |

**On import:**
1. Extract `sprite.json` + `sprite.png` → save to `sprites/`; add to gallery
2. If `behavior.petstate` present: validate → save to `state_machines/` (or `drafts/` if invalid)
3. Store `recommended_sm` in `sprites/gallery.toml` entry for this sprite
4. Show import summary: *"Imported: Lazy Cat sprite + Lazy Cat behavior (recommended pairing)"*

**Recommended SM UX:**
- In the config dialog, when a sprite with a recommended SM is selected: `★ Recommended SM: Lazy Cat Behavior [Use it]`
- One-click applies it; user is free to pick any other SM
- Association lives in `gallery.toml`, not in the sprite JSON

**Name collision on import:** if an SM or sprite with the same name already exists, prompt: *Rename / Replace / Cancel*.

**Bundle export:**
- "Export Bundle" button in the sprite editor toolbar
- User selects an SM from the collection (or skips for sprite-only)
- Produces a `.petbundle` via native save dialog

---

## Sprite Sheet Alignment (`smMappings`)

How a sprite with tags `walk_cycle`, `snooze` declares compatibility with an SM using state names `patrol`, `sleep`.

### Storage in sprite JSON

```json
{
  "meta": {
    "smMappings": {
      "Default Pet": {
        "idle":  "idle_cycle",
        "walk":  "walk_cycle",
        "sleep": "snooze"
      },
      "Patrol Guard": {
        "stand":  "idle_cycle",
        "patrol": "walk_cycle"
      }
    }
  }
}
```

**Auto-matching:** tag name == state name → matches without an explicit entry.

**Runtime resolution order (per state name):**
1. `smMappings[sm_name][state_name]` (explicit alias)
2. Tag with name equal to `state_name` (auto-match)
3. `fallback` declared in the SM for that state
4. `default_fallback` from SM `[meta]`

### Editing in the sprite editor

When an SM is selected in the SM switcher dropdown:

- **Coverage panel** shows per-state status:
  - `✓ auto` — direct name match
  - `✓ walk_cycle → patrol` — explicit alias
  - `⚠ tag "walk_cycle" no longer exists` — dangling alias
  - `✗ missing — required` — blocks valid save for this sprite+SM pairing
  - `○ missing — uses fallback "stand"` — graceful degradation
- Clicking an unmapped state shows a tag picker dropdown
- Changes set the sprite dirty flag

**SM switching mid-edit:**
- Full `smMappings` dict is in memory; switching SM changes only the viewed key
- No data loss on switch; dirty flag reflects unsaved changes
- On sprite save: dangling alias entries removed with a warning

---

## In-App SM Editor

A new egui viewport. Threading model mirrors the sprite editor:

```rust
pub struct SMEditorViewport {
    // written by egui thread, read by app thread each frame:
    pub saved_sm_path: Option<String>,   // path of newly saved live SM (triggers hot-reload)
    pub force_state: Option<String>,     // debug: force pet into this state
    pub step_mode: bool,                 // debug: freeze auto-transitions
    pub step_advance: bool,              // debug: advance one transition (consumed each frame)
    pub should_close: bool,

    // written by app thread, read by egui thread each frame:
    pub active_state_snapshot: Option<String>,     // currently active state name
    pub variable_snapshot: Option<ConditionVars>,  // live condition variable values
    pub transition_log: Vec<TransitionLogEntry>,   // last 10 transitions
}
// Shared as Arc<Mutex<SMEditorViewport>> between app thread and egui viewport thread.
```

### Layout

**Left panel — SM browser:**
- List of installed (valid) SMs
- List of drafts (dimmed) — editable only
- "Import .petstate" / "Import .petbundle" / "New SM" / "Copy built-in default" buttons

**Center panel — text editor:**
- Full `.petstate` TOML source, editable
- Inline validation highlights with line/column
- Save → full validation → live SM or draft
- Status: `✓ Valid` / `✗ 3 errors`

**Right panel — live state graph:**
- States + transitions rendered from last valid compiled SM
- Composite states shown as expandable groups
- Active state highlighted in real-time (from `active_state_snapshot`)
- **▶ Force** button on each state node (writes `force_state`)

**Bottom bar — error list:**
- Clickable rows → jump to offending line
- Typo suggestions: `unknown variable 'cusror_dist' — did you mean 'cursor_dist'?`

### Debug Tools (SM editor only)

**Force state:** writes `force_state` field → `SMRunner::force_state()` called next frame. Banner: `FORCED: [name]` with Release button. Transitions and timers frozen; interrupts still fire. An interrupt during force-state implicitly releases the force (the forced-state banner clears automatically). Forcing a composite state enters its first sub-step and proceeds through the sequence normally (timers on sub-steps still run; only the composite's outbound transitions are frozen until released).

**Step mode:** sets `step_mode = true` → SMRunner skips auto-transitions. `→ Next` button sets `step_advance = true` (consumed once per frame). Interrupts (grabbed, petted, etc.) still fire during step mode — same as force state.

**Variable inspector:** displays `variable_snapshot` — live values of all condition variables.

**Transition log:** displays `transition_log` — last 10 entries with state names and reason.

---

## Config Changes

`PetConfig` in `config/schema.rs`:

```rust
pub struct PetConfig {
    pub id: String,
    pub sheet_path: String,
    pub state_machine: String,   // path to .petstate, or "embedded://default"
    pub x: i32,
    pub y: i32,
    pub scale: u32,
    pub walk_speed: f32,
    // tag_map: AnimTagMap  ← REMOVED; resolved from smMappings in sprite JSON
}
```

---

## Code Deletion Checklist

All of the following are deleted outright — no compatibility shims:

**Deleted structs / functions:**
- `AnimTagMap` (all fields, serde derives, `tag_for()` method)
- `BehaviorAi` (entire struct and impl)
- `sheet::load_with_tag_map()`, `sheet::parse_my_pet_tag_map()`
- `meta.myPetTagMap` read/write in `sprite_editor.rs`

**Updated call sites in `app.rs`:**
- `PetInstance.ai: BehaviorAi` → `runner: SMRunner`
- `ai.tick(…, tag_map)` → `runner.tick(…, sheet)`
- `ai.wake()` → `runner.interrupt("wake", None)`
- `ai.react()` → `runner.interrupt("react", None)`
- `ai.grab(offset)` → `runner.grab(offset)`
- `ai.release(vel)` → `runner.release(vel)`
- `compute_flip()` pattern-match on `BehaviorState` → `runner.current_facing()`

**Updated config:**
- `PetConfig.tag_map: AnimTagMap` removed; `PetConfig.state_machine: String` added
- Config TOML from older sessions will fail to deserialize — acceptable, not production

---

## Edge Cases

| Situation | Handling |
|---|---|
| SM references a state with no sprite tag (required) | Compile error; pairing blocked |
| SM references a state with no sprite tag (optional) | Replaced by fallback at compile time |
| Sprite tag renamed while mapping points to it | Coverage panel shows `⚠`; removed on sprite save with warning |
| SM name collision on import | Prompt: Rename / Replace / Cancel |
| `engine_min_version` newer than running engine | Load rejected with clear message |
| Composite state with cyclic steps | Compiler detects cycle → error |
| `steps` references a state that itself has `steps` (nested composite) | Compiler error |
| `$previous` with no prior Named state | Resolves to `default_fallback` |
| Transition `goto` targets non-existent state | Compiler error |
| `.petstate` modified externally while in use | Re-validate; keep last valid SM if broken |
| `.petbundle` contains invalid `.petstate` | SM goes to drafts; sprite imports successfully |
| All transition weights are 0 | Compiler error |
| Interrupt fires during a composite step | Interrupts the whole composite; `$previous` = composite state name |
| `walk` hits screen edge before distance is consumed | Walk stops early; transitions fire immediately |
| `float` reaches screen boundary | Position clamps at edge; motion continues if conditions allow transition |
