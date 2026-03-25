# SM Core Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hardcoded `BehaviorAi` state machine with a data-driven `SMRunner` that loads, compiles, and executes `.petstate` TOML files at runtime.

**Architecture:** Raw TOML is deserialized into `SmFile` structs, compiled into a `CompiledSM` (indexed, expression ASTs pre-built), then executed each frame by `SMRunner::tick()` which replaces `BehaviorAi::tick()`. `AnimTagMap` and `BehaviorAi` are deleted entirely — no shims.

**Tech Stack:** Rust, `serde`/`toml` (existing), `rust-embed` (existing for embedded default SM asset)

**Spec:** `docs/superpowers/specs/2026-03-23-user-defined-state-machines.md`

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src/version.rs` | **Create** | `ENGINE_VERSION` constant |
| `src/sprite/sm_format.rs` | **Create** | Raw serde structs matching `.petstate` TOML schema |
| `src/sprite/sm_expr.rs` | **Create** | Expression AST types, parser, evaluator, `ConditionVars` |
| `src/sprite/sm_compiler.rs` | **Create** | Validates `SmFile` → `CompiledSM`; all error reporting |
| `src/sprite/sm_runner.rs` | **Create** | `SMRunner`, `ActiveState`, `CompiledSM` + runtime execution |
| `assets/default.petstate` | **Create** | Default SM (current hardcoded behavior as TOML) |
| `src/sprite/behavior.rs` | **Delete** | Replaced by `sm_runner.rs` |
| `src/config/schema.rs` | **Modify** | Remove `AnimTagMap`; add `state_machine: String` to `PetConfig` |
| `src/sprite/sheet.rs` | **Modify** | Remove `load_with_tag_map`, `parse_my_pet_tag_map`, `AnimTagMap` export |
| `src/app.rs` | **Modify** | Replace `BehaviorAi` with `SMRunner`; update all call sites |
| `src/event.rs` | **Modify** | Add `SMImported`, `SMChanged` events |
| `src/main.rs` | **Modify** | Add `mod version;` |
| `src/sprite/mod.rs` | **Modify** | Add new module declarations |
| `Cargo.toml` | **Modify** | Confirm `toml`, `serde` features cover all needed derives |

---

## Task 1: Engine version constant

**Files:**
- Create: `src/version.rs`
- Modify: `src/main.rs`

- [ ] Create `src/version.rs`:

```rust
pub const ENGINE_VERSION: &str = "1.0";
```

- [ ] Add `mod version;` to `src/main.rs` (after existing mod declarations).

- [ ] Build to confirm it compiles:

```
cargo build 2>&1 | head -20
```
Expected: no errors.

- [ ] Commit:
```
git add src/version.rs src/main.rs
git commit -m "feat: add ENGINE_VERSION constant"
```

---

## Task 2: SM format types (raw TOML structs)

**Files:**
- Create: `src/sprite/sm_format.rs`
- Modify: `src/sprite/mod.rs`

- [ ] Create `src/sprite/sm_format.rs`:

```rust
use std::collections::HashMap;
use serde::Deserialize;

/// Top-level structure of a `.petstate` TOML file.
#[derive(Deserialize, Debug)]
pub struct SmFile {
    pub meta: SmMeta,
    #[serde(default)]
    pub interrupts: HashMap<String, SmInterruptDef>,
    #[serde(default)]
    pub states: HashMap<String, SmStateDef>,
}

#[derive(Deserialize, Debug)]
pub struct SmMeta {
    pub name: String,
    pub version: String,
    pub engine_min_version: String,
    pub default_fallback: String,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct SmInterruptDef {
    pub goto: Option<String>,
    pub condition: Option<String>,
    pub ignore: Option<bool>,
}

/// Covers both atomic and composite states; validation distinguishes them.
#[derive(Deserialize, Debug, Default)]
pub struct SmStateDef {
    pub required: Option<bool>,
    pub fallback: Option<String>,

    // Atomic state fields
    pub action: Option<String>,
    pub duration: Option<String>,
    pub dir: Option<String>,
    pub speed: Option<f32>,
    pub distance: Option<String>,
    pub gravity_scale: Option<f32>,
    pub transitions: Option<Vec<SmTransitionDef>>,
    #[serde(default)]
    pub interrupts: HashMap<String, SmInterruptDef>,

    // Composite state fields
    pub steps: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SmTransitionDef {
    pub goto: String,
    pub weight: Option<u32>,
    pub after: Option<String>,
    pub condition: Option<String>,
}
```

- [ ] Add `pub mod sm_format;` to `src/sprite/mod.rs`.

- [ ] Write a unit test in `sm_format.rs` to verify basic TOML deserialization:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_minimal_sm() {
        let toml = r#"
[meta]
name = "Test"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"
"#;
        let sm: SmFile = toml::from_str(toml).unwrap();
        assert_eq!(sm.meta.name, "Test");
        assert!(sm.states.contains_key("idle"));
    }

    #[test]
    fn deserialize_composite_state() {
        let toml = r#"
[meta]
name = "Test"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"

[states.routine]
steps = ["idle", "idle"]
"#;
        let sm: SmFile = toml::from_str(toml).unwrap();
        let routine = &sm.states["routine"];
        assert_eq!(routine.steps.as_ref().unwrap().len(), 2);
    }
}
```

- [ ] Run tests:
```
cargo test sm_format
```
Expected: 2 tests pass.

- [ ] Commit:
```
git add src/sprite/sm_format.rs src/sprite/mod.rs
git commit -m "feat: add SmFile raw TOML deserialization structs"
```

---

## Task 3: Expression language — AST and parser

**Files:**
- Create: `src/sprite/sm_expr.rs`
- Modify: `src/sprite/mod.rs`

- [ ] Create `src/sprite/sm_expr.rs` with AST types, `ConditionVars`, and parser:

```rust
/// All variables available to condition expressions.
#[derive(Debug, Clone, Default)]
pub struct ConditionVars {
    pub cursor_dist: f32,
    pub state_time_ms: u32,
    pub on_surface: bool,
    pub pet_x: f32,
    pub pet_y: f32,
    pub pet_vx: f32,
    pub pet_vy: f32,
    pub pet_v: f32,   // pre-computed: sqrt(vx²+vy²)
    pub pet_w: f32,
    pub pet_h: f32,
    pub screen_w: f32,
    pub screen_h: f32,
    pub hour: u32,
    pub focused_app: String,
}

/// A compiled expression node.
#[derive(Debug, Clone)]
pub enum Expr {
    Literal(Literal),
    Var(Var),
    BinOp { op: BinOp, left: Box<Expr>, right: Box<Expr> },
    UnaryNot(Box<Expr>),
    Call { name: String, args: Vec<Expr> },
}

#[derive(Debug, Clone)]
pub enum Literal {
    Number(f32),
    DurationMs(u32),
    Bool(bool),
    Str(String),
}

#[derive(Debug, Clone)]
pub enum Var {
    CursorDist,
    StateTime,
    OnSurface,
    PetX, PetY, PetVx, PetVy, PetV,
    ScreenW, ScreenH,
    Hour,
    FocusedApp,
    NearEdge { axis: Option<Axis>, threshold_px: f32 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Axis { X, Y }

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp { Lt, Gt, Le, Ge, Eq, Ne, And, Or }

/// Parse an expression string into an AST. Returns Err with a message on failure.
pub fn parse(src: &str) -> Result<Expr, String> { ... }

/// Evaluate a compiled expression against a context snapshot.
pub fn eval(expr: &Expr, vars: &ConditionVars) -> Result<bool, String> { ... }
```

- [ ] Implement the **lexer** (private `tokenize(src)` function returning `Vec<Token>`):

Tokens: `Number(f32)`, `DurationMs(u32)`, `Str(String)`, `Ident(String)`, `NearEdge { axis, threshold_px }`, `Op(BinOp)`, `Not`, `LParen`, `RParen`, `Comma`, `Dot`.

Key rule: when lexer sees `near_edge`, scan ahead for `.x`/`.y` and/or `.\d+px` suffixes and emit a single `NearEdge` token.

Duration literals: `"500ms"` → `DurationMs(500)`, `"1s"` → `DurationMs(1000)`, `"1.5s"` → `DurationMs(1500)`.

- [ ] Implement the **recursive-descent parser** (`parse_or → parse_and → parse_cmp → parse_unary → parse_primary`).

- [ ] Add to `src/sprite/mod.rs`: `pub mod sm_expr;`

- [ ] Write parser unit tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_comparison() {
        assert!(parse("cursor_dist < 150").is_ok());
    }

    #[test]
    fn parse_duration_comparison() {
        assert!(parse("state_time > 3s").is_ok());
    }

    #[test]
    fn parse_and_expression() {
        assert!(parse("on_surface and cursor_dist < 50").is_ok());
    }

    #[test]
    fn parse_not() {
        assert!(parse("not on_surface").is_ok());
    }

    #[test]
    fn parse_near_edge_plain() {
        assert!(parse("near_edge").is_ok());
    }

    #[test]
    fn parse_near_edge_parameterized() {
        assert!(parse("near_edge.x.70px").is_ok());
        assert!(parse("near_edge.80px").is_ok());
        assert!(parse("near_edge.y").is_ok());
    }

    #[test]
    fn parse_function_call() {
        assert!(parse("abs(pet.vx) > 50").is_ok());
    }

    #[test]
    fn parse_unknown_variable_fails() {
        assert!(parse("typo_var < 5").is_err());
    }

    #[test]
    fn parse_unknown_function_fails() {
        assert!(parse("eval(1)").is_err());
    }

    #[test]
    fn parse_string_literal() {
        assert!(parse(r#"input.focused_app == "code.exe""#).is_ok());
    }
}
```

- [ ] Run:
```
cargo test sm_expr::tests
```
Expected: all tests pass.

- [ ] Commit:
```
git add src/sprite/sm_expr.rs src/sprite/mod.rs
git commit -m "feat: add expression AST and parser for SM conditions"
```

---

## Task 4: Expression evaluator

**Files:**
- Modify: `src/sprite/sm_expr.rs`

- [ ] Implement `eval(expr, vars) -> Result<bool, String>`:
  - `NearEdge { axis: None, threshold_px }` → `pet.x < t || pet.x + pet.w > screen.w - t || pet.y < t || pet.y + pet.h > screen.h - t`
  - `NearEdge { axis: Some(X), threshold_px }` → left or right edge only
  - `NearEdge { axis: Some(Y), threshold_px }` → top or bottom edge only
  - `StateTime` compares against `vars.state_time_ms` (convert duration literals to ms at parse time)
  - `PetV` evaluates `vars.pet_v` directly
  - Whitelisted functions: `abs(x)`, `min(a,b)`, `max(a,b)` — unknown name → `Err`

- [ ] Write evaluator unit tests:

```rust
#[test]
fn eval_cursor_near() {
    let expr = parse("cursor_dist < 150").unwrap();
    let mut v = ConditionVars::default();
    v.cursor_dist = 100.0;
    assert!(eval(&expr, &v).unwrap());
    v.cursor_dist = 200.0;
    assert!(!eval(&expr, &v).unwrap());
}

#[test]
fn eval_state_time_duration() {
    let expr = parse("state_time > 3s").unwrap();
    let mut v = ConditionVars::default();
    v.state_time_ms = 5000;
    assert!(eval(&expr, &v).unwrap());
    v.state_time_ms = 2000;
    assert!(!eval(&expr, &v).unwrap());
}

#[test]
fn eval_near_edge_x() {
    let expr = parse("near_edge.x.70px").unwrap();
    let mut v = ConditionVars { pet_x: 30.0, screen_w: 1920.0, pet_w: 32.0, ..Default::default() };
    assert!(eval(&expr, &v).unwrap()); // 30 < 70
    v.pet_x = 500.0;
    assert!(!eval(&expr, &v).unwrap());
}

#[test]
fn eval_pet_velocity() {
    let expr = parse("pet.v > 100").unwrap();
    let mut v = ConditionVars::default();
    v.pet_v = 150.0;
    assert!(eval(&expr, &v).unwrap());
}

#[test]
fn eval_abs_function() {
    let expr = parse("abs(pet.vx) > 50").unwrap();
    let mut v = ConditionVars::default();
    v.pet_vx = -80.0;
    assert!(eval(&expr, &v).unwrap());
}

#[test]
fn eval_focused_app() {
    let expr = parse(r#"input.focused_app == "code.exe""#).unwrap();
    let mut v = ConditionVars::default();
    v.focused_app = "code.exe".to_string();
    assert!(eval(&expr, &v).unwrap());
    v.focused_app = "other.exe".to_string();
    assert!(!eval(&expr, &v).unwrap());
}
```

- [ ] Run:
```
cargo test sm_expr
```
Expected: all tests pass.

- [ ] Commit:
```
git add src/sprite/sm_expr.rs
git commit -m "feat: implement expression evaluator with ConditionVars"
```

---

## Task 5: SM compiler — validation

**Files:**
- Create: `src/sprite/sm_compiler.rs`
- Modify: `src/sprite/mod.rs`

- [ ] Create `src/sprite/sm_compiler.rs` with `CompileError` type and validation logic:

```rust
use crate::sprite::sm_format::SmFile;
use crate::sprite::sm_expr::{parse as parse_expr, Expr};
use crate::version::ENGINE_VERSION;
use std::collections::{HashMap, HashSet};

#[derive(Debug, thiserror::Error)]  // or just use String for errors
pub enum CompileError {
    #[error("engine_min_version '{0}' requires engine >= '{0}', have '{1}'")]
    EngineTooOld(String, String),
    #[error("no required=true state declared")]
    NoRequiredState,
    #[error("default_fallback '{0}' must be a required state")]
    InvalidDefaultFallback(String),
    #[error("state '{0}': goto target '{1}' does not exist")]
    UnknownGotoTarget(String, String),
    #[error("state '{0}': fallback '{1}' does not exist or is not required")]
    InvalidFallback(String, String),
    #[error("engine primitive '{0}' (grabbed/fall/thrown) must be present")]
    MissingEnginePrimitive(String),
    #[error("composite state '{0}' has nested steps (not supported in v1)")]
    NestedComposite(String),
    #[error("steps cycle detected involving state '{0}'")]
    StepsCycle(String),
    #[error("state '{0}': transition weight group sums to 0")]
    ZeroWeightSum(String),
    #[error("state '{0}': condition parse error: {1}")]
    ConditionParseError(String, String),
    #[error("state '{0}': duration/after parse error: '{1}'")]
    DurationParseError(String, String),
}

/// Validate a parsed SmFile without compiling it to a runtime structure.
/// Returns a list of all errors (not just the first one).
pub fn validate(sm: &SmFile) -> Vec<CompileError> { ... }
```

Implement `validate()` with all checks from the spec (§SM Compiler steps 3–11, except expression AST build which is part of compilation).

- [ ] Add `pub mod sm_compiler;` to `src/sprite/mod.rs`.

- [ ] Write validation tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_valid() -> &'static str {
        r#"
[meta]
name = "Test"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"
transitions = []

[states.grabbed]
required = true
action = "grabbed"
transitions = []

[states.fall]
required = true
action = "fall"
transitions = [{ goto = "idle", condition = "on_surface" }]

[states.thrown]
required = true
action = "thrown"
transitions = [{ goto = "fall", condition = "on_surface" }]
"#
    }

    #[test]
    fn valid_sm_has_no_errors() {
        let sm: SmFile = toml::from_str(minimal_valid()).unwrap();
        assert!(validate(&sm).is_empty());
    }

    #[test]
    fn missing_required_state_error() {
        let toml = r#"
[meta]
name = "T" version = "1.0" engine_min_version = "1.0" default_fallback = "a"
[states.a]
action = "idle"
"#;
        let sm: SmFile = toml::from_str(toml).unwrap();
        let errs = validate(&sm);
        assert!(errs.iter().any(|e| matches!(e, CompileError::NoRequiredState)));
    }

    #[test]
    fn missing_engine_primitive_error() {
        let toml = r#"
[meta]
name = "T" version = "1.0" engine_min_version = "1.0" default_fallback = "idle"
[states.idle]
required = true
action = "idle"
"#;
        let sm: SmFile = toml::from_str(toml).unwrap();
        let errs = validate(&sm);
        assert!(errs.iter().any(|e| matches!(e, CompileError::MissingEnginePrimitive(_))));
    }

    #[test]
    fn unknown_goto_target_error() {
        let toml = r#"
[meta]
name = "T"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"
transitions = [{ goto = "nonexistent" }]

[states.grabbed]
required = true
action = "grabbed"
transitions = []

[states.fall]
required = true
action = "fall"
transitions = [{ goto = "idle", condition = "on_surface" }]

[states.thrown]
required = true
action = "thrown"
transitions = [{ goto = "fall", condition = "on_surface" }]
"#;
        let sm: SmFile = toml::from_str(toml).unwrap();
        let errs = validate(&sm);
        assert!(errs.iter().any(|e| matches!(e, CompileError::UnknownGotoTarget(..))));
    }

    #[test]
    fn nested_composite_error() {
        let toml = r#"
[meta]
name = "T"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"

[states.grabbed]
required = true
action = "grabbed"
transitions = []

[states.fall]
required = true
action = "fall"
transitions = []

[states.thrown]
required = true
action = "thrown"
transitions = []

[states.outer]
steps = ["inner"]

[states.inner]
steps = ["idle"]
"#;
        let sm: SmFile = toml::from_str(toml).unwrap();
        let errs = validate(&sm);
        assert!(errs.iter().any(|e| matches!(e, CompileError::NestedComposite(_))));
    }

    #[test]
    fn steps_cycle_error() {
        let toml = r#"
[meta]
name = "T"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"

[states.grabbed]
required = true
action = "grabbed"
transitions = []

[states.fall]
required = true
action = "fall"
transitions = []

[states.thrown]
required = true
action = "thrown"
transitions = []

[states.a]
steps = ["b"]

[states.b]
steps = ["a"]
"#;
        let sm: SmFile = toml::from_str(toml).unwrap();
        let errs = validate(&sm);
        assert!(errs.iter().any(|e| matches!(e, CompileError::StepsCycle(_))));
    }
}
```

- [ ] Run:
```
cargo test sm_compiler
```
Expected: passing tests for each error case.

- [ ] Commit:
```
git add src/sprite/sm_compiler.rs src/sprite/mod.rs
git commit -m "feat: SM compiler validation with all error cases"
```

---

## Task 6: SM compiler — compile to `CompiledSM`

**Files:**
- Modify: `src/sprite/sm_compiler.rs`

- [ ] Add `CompiledSM` and related types to `sm_compiler.rs`:

```rust
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CompiledSM {
    pub name: String,
    pub default_fallback: String,
    pub global_interrupts: Vec<CompiledInterrupt>,
    pub states: HashMap<String, CompiledState>,
}

#[derive(Debug, Clone)]
pub struct CompiledState {
    pub required: bool,
    pub fallback: Option<String>,
    pub kind: StateKind,
    pub per_state_interrupts: Vec<CompiledInterrupt>,
}

#[derive(Debug, Clone)]
pub enum StateKind {
    Atomic {
        action: ActionType,
        params: ActionParams,
        transitions: Vec<CompiledTransition>,
    },
    Composite {
        steps: Vec<String>,
        transitions: Vec<CompiledTransition>,
    },
}

#[derive(Debug, Clone)]
pub struct ActionParams {
    pub dir: Option<Direction>,
    pub speed_override: Option<f32>,
    pub distance_min_px: Option<f32>,
    pub distance_max_px: Option<f32>,
    pub gravity_scale: f32,
    pub duration_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActionType {
    Idle, Walk, Run, Sit, Jump, Float, FollowCursor, FleeCursor,
    Grabbed, Fall, Thrown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Direction { Left, Right, Random }

#[derive(Debug, Clone)]
pub struct CompiledTransition {
    pub goto: Goto,
    pub weight: Option<u32>,
    pub after_min_ms: Option<u32>,
    pub after_max_ms: Option<u32>,
    pub condition: Option<Expr>,
}

#[derive(Debug, Clone)]
pub enum Goto { State(String), Previous }

#[derive(Debug, Clone)]
pub struct CompiledInterrupt {
    pub event: String,
    pub def: InterruptEffect,
}

#[derive(Debug, Clone)]
pub enum InterruptEffect {
    Goto { target: String, condition: Option<Expr> },
    Ignore,
}

/// Validate and compile. Returns Err with all validation errors if any.
pub fn compile(sm: &SmFile) -> Result<Arc<CompiledSM>, Vec<CompileError>> { ... }
```

- [ ] Implement `compile()`: call `validate()` first, then build `CompiledSM`.

- [ ] Write compilation tests:

```rust
#[test]
fn compile_valid_sm_succeeds() {
    let sm: SmFile = toml::from_str(minimal_valid()).unwrap();
    assert!(compile(&sm).is_ok());
}

#[test]
fn compiled_states_indexed() {
    let sm: SmFile = toml::from_str(minimal_valid()).unwrap();
    let compiled = compile(&sm).unwrap();
    assert!(compiled.states.contains_key("idle"));
    assert!(compiled.states.contains_key("grabbed"));
}

#[test]
fn compile_invalid_sm_returns_errors() {
    let bad_toml = r#"
[meta]
name = "T" version = "1.0" engine_min_version = "1.0" default_fallback = "idle"
[states.idle]
action = "idle"
"#; // missing required = true
    let sm: SmFile = toml::from_str(bad_toml).unwrap();
    assert!(compile(&sm).is_err());
}
```

- [ ] Run:
```
cargo test sm_compiler
```
Expected: all tests pass.

- [ ] Commit:
```
git add src/sprite/sm_compiler.rs
git commit -m "feat: compile SmFile to CompiledSM runtime structure"
```

---

## Task 7: `ActiveState` and `SMRunner` struct

**Files:**
- Create: `src/sprite/sm_runner.rs`
- Modify: `src/sprite/mod.rs`

- [ ] Create `src/sprite/sm_runner.rs` with data structures only (no tick logic yet):

```rust
use std::sync::Arc;
use crate::sprite::sm_compiler::{CompiledSM, ActionType, Direction};
use crate::sprite::sm_expr::ConditionVars;
use crate::sprite::sheet::SpriteSheet;

#[derive(Debug, Clone)]
pub enum ActiveState {
    Named(String),
    Fall { vy: f32 },
    Thrown { vx: f32, vy: f32 },
    Grabbed { cursor_offset: (i32, i32) },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Facing { Left, Right }

pub struct SMRunner {
    pub sm: Arc<CompiledSM>,
    active: ActiveState,
    previous_named: Option<String>,
    state_time_ms: u32,
    step_index: usize,       // composite step progress
    walk_remaining_px: f32,
    facing: Facing,
    walk_speed: f32,         // from PetConfig; used as default speed for walk/run actions
    rng: u64,
    next_transition_ms: u32, // pre-computed timer threshold
    // Debug tools (written by SM editor viewport each frame)
    pub force_state: Option<String>,
    pub release_force: bool,   // separate sentinel — avoids magic strings in force_state
    pub step_mode: bool,
    pub step_advance: bool,
}

impl SMRunner {
    pub fn new(sm: Arc<CompiledSM>, walk_speed: f32) -> Self {
        let mut runner = Self {
            sm,
            active: ActiveState::Named("idle".to_string()),
            previous_named: None,
            state_time_ms: 0,
            step_index: 0,
            walk_remaining_px: 0.0,
            facing: Facing::Right,
            rng: 12345,
            next_transition_ms: 0,
            force_state: None,
            step_mode: false,
            step_advance: false,
        };
        runner.enter_state("idle");
        runner
    }

    pub fn current_facing(&self) -> Option<Facing> { ... }
    /// Returns the name of the current Named state, or None for physics states.
    pub fn current_state_name(&self) -> Option<&str> { ... }
    /// Returns the last captured condition variables (updated every tick).
    pub fn last_condition_vars(&self) -> &ConditionVars { ... }
    /// Returns the last 10 transition log entries (oldest first).
    pub fn transition_log(&self) -> &[TransitionLogEntry] { ... }
    pub fn interrupt(&mut self, event: &str, cursor_offset: Option<(i32, i32)>) { ... }
    pub fn grab(&mut self, cursor_offset: (i32, i32)) { ... }
    pub fn release(&mut self, velocity: (f32, f32)) { ... }

    fn enter_state(&mut self, name: &str) { ... }
    fn lcg_rand(&mut self) -> u64 { ... }
    fn rand_range(&mut self, min: u32, max: u32) -> u32 { ... }
}
```

- [ ] Implement `lcg_rand()` (same LCG as current `behavior.rs`):
```rust
fn lcg_rand(&mut self) -> u64 {
    self.rng = self.rng
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    self.rng >> 33
}
```

- [ ] Add `pub mod sm_runner;` to `src/sprite/mod.rs`.

- [ ] Build to confirm struct definitions compile:
```
cargo build 2>&1 | head -30
```

- [ ] Commit:
```
git add src/sprite/sm_runner.rs src/sprite/mod.rs
git commit -m "feat: add SMRunner and ActiveState structs"
```

---

## Task 8: `SMRunner::tick()` — atomic states

**Files:**
- Modify: `src/sprite/sm_runner.rs`

- [ ] Implement `tick()` for atomic states. Note: `walk_speed` is stored on `SMRunner` (set in `new()`), NOT a tick parameter. This matches the spec signature:

```rust
pub fn tick(
    &mut self,
    delta_ms: u32,
    x: &mut i32,
    y: &mut i32,
    screen_w: i32,
    pet_w: i32,
    pet_h: i32,
    floor_y: i32,
    sheet: &SpriteSheet,
) -> &str {
    // 1. Handle debug force
    if let Some(name) = self.force_state.take() {
        self.set_previous_from_current();
        self.enter_named(name);
    }

    // 2. Accumulate time
    self.state_time_ms += delta_ms;

    // 3. Execute action primitive
    self.execute_action(delta_ms, x, y, screen_w, pet_w, pet_h, floor_y, walk_speed);

    // 4. Evaluate transitions (unless step_mode and not step_advance)
    if !self.step_mode || self.step_advance {
        self.step_advance = false;
        self.try_transition(screen_w, pet_w, pet_h, floor_y, walk_speed);
    }

    // 5. Resolve tag
    self.resolve_tag(sheet)
}
```

- [ ] Implement `execute_action()` for atomic states — physics for each `ActionType`:
  - `Idle`/`Sit`: no horizontal movement, surface snap (use existing gravity logic pattern)
  - `Walk`: `*x += speed * dir_sign * (delta_ms as f32 / 1000.0)`, decrement `walk_remaining_px`
  - `Run`: same as walk with default `2 × walk_speed`
  - `Jump`: apply upward `vy`, then fall (transition to `Fall` state)
  - `Float`: `*x += vx * dt`, `*y += vy * dt`, clamp at screen edges
  - `FollowCursor` / `FleeCursor`: move toward/away from cursor (cursor position from ConditionVars)
  - `Fall` / `Thrown` / `Grabbed`: delegated to existing physics helpers (same as `behavior.rs`)

- [ ] Implement `resolve_tag(sheet)` as a temporary placeholder (Plan 2 Task 1 will replace this with `sheet.resolve_tag()`):

```rust
fn resolve_tag<'a>(&self, sheet: &'a SpriteSheet, state_name: &str) -> &'a str {
    // TODO(Plan-2-Task-1): replace with sheet.resolve_tag(sm_name, state_name)
    // Temporary: auto-match by tag name, fall back to default_fallback tag
    if sheet.tags.iter().any(|t| t.name == state_name) {
        return state_name; // lifetime: sheet outlives this call
    }
    // fall back to default_fallback state's tag
    self.sm.default_fallback.as_str()
}
```

- [ ] Write tick unit tests using a mock sheet:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_runner() -> SMRunner {
        let sm_toml = include_str!("../../assets/default.petstate");
        let file: SmFile = toml::from_str(sm_toml).unwrap();
        let compiled = compile(&file).unwrap();
        SMRunner::new(compiled, 80.0)
    }

    #[test]
    fn starts_in_idle() {
        let r = make_runner();
        assert!(matches!(&r.active, ActiveState::Named(n) if n == "idle"));
    }

    #[test]
    fn grab_transitions_to_grabbed() {
        let mut r = make_runner();
        r.grab((0, 0));
        assert!(matches!(&r.active, ActiveState::Grabbed { .. }));
    }

    #[test]
    fn release_at_high_velocity_transitions_to_thrown() {
        let mut r = make_runner();
        r.grab((0, 0));
        r.release((200.0, -100.0));
        assert!(matches!(&r.active, ActiveState::Thrown { .. }));
    }

    #[test]
    fn interrupt_petted_transitions_to_petted_state() {
        let mut r = make_runner();
        r.interrupt("petted", None);
        assert!(matches!(&r.active, ActiveState::Named(n) if n == "petted"));
    }
}
```

- [ ] Run:
```
cargo test sm_runner
```
Expected: all tests pass.

- [ ] Commit:
```
git add src/sprite/sm_runner.rs
git commit -m "feat: SMRunner tick for atomic states and interrupt handling"
```

---

## Task 9: `SMRunner::tick()` — composite states and `$previous`

**Files:**
- Modify: `src/sprite/sm_runner.rs`

- [ ] Extend `tick()` and `enter_state()` to handle composite states:
  - `enter_named(composite_name)`: set `step_index = 0`, enter first step's action
  - When a step's action completes (timer/distance/condition), advance `step_index`; if last step → fire composite's `transitions`
  - Interrupts during a step: `previous_named = composite_name` (not sub-step name)

- [ ] Implement `$previous` resolution in `enter_state()`: when `Goto::Previous`, look up `previous_named`, fall back to `default_fallback`.

- [ ] Write composite state tests:

```rust
#[test]
fn composite_runs_steps_in_order() {
    // Build a minimal SM with composite state, tick through it
    let toml = r#"
...composite state with steps = ["stir", "yawn"]...
"#;
    // verify step_index advances
}

#[test]
fn previous_returns_to_interrupted_state() {
    let mut r = make_runner();
    // Enter idle, then trigger petted interrupt
    r.interrupt("petted", None);
    // After petted duration, should return to idle via $previous
    // Advance time past petted duration
    let sheet = mock_sheet();
    let mut x = 0; let mut y = 800;
    for _ in 0..100 { r.tick(10, &mut x, &mut y, 1920, 32, 32, 800, 80.0, &sheet); }
    assert!(matches!(&r.active, ActiveState::Named(n) if n == "idle"));
}
```

- [ ] Run:
```
cargo test sm_runner
```

- [ ] Commit:
```
git add src/sprite/sm_runner.rs
git commit -m "feat: composite state step sequencing and \$previous resolution"
```

---

## Task 10: Default SM asset

**Files:**
- Create: `assets/default.petstate`
- Modify: `src/sprite/sm_runner.rs` or a new `src/embedded.rs`

- [ ] Create `assets/default.petstate` encoding exactly the current hardcoded behavior:

```toml
[meta]
name               = "Default Pet"
version            = "1.0"
engine_min_version = "1.0"
default_fallback   = "idle"

[interrupts]
grabbed = { goto = "grabbed" }
petted  = { goto = "petted" }

[states.idle]
required = true
action   = "idle"
transitions = [
  { goto = "walk",  weight = 45, after = "1s..3s" },
  { goto = "sit",   weight = 20, after = "1s..3s" },
  { goto = "sleep", weight = 15, after = "15s" },
]

[states.walk]
required = true
action   = "walk"
dir      = "random"
distance = "200px..800px"
transitions = [{ goto = "idle" }]

[states.run]
required = false
fallback = "walk"
action   = "run"
dir      = "random"
distance = "200px..600px"
transitions = [{ goto = "idle" }]

[states.sit]
required = false
fallback = "idle"
action   = "sit"
transitions = [{ goto = "idle", after = "1.5s..4s" }]

[states.sleep]
required = false
fallback = "idle"
action   = "idle"
transitions = [{ goto = "wake", condition = "state_time > 15s" }]
interrupts.petted = { goto = "petted" }

[states.wake]
required = false
fallback = "idle"
action   = "idle"
duration = "800ms"
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

[states.petted]
required = false
fallback = "idle"
action   = "idle"
duration = "600ms"
transitions = [{ goto = "$previous" }]

[states.react]
required = false
fallback = "idle"
action   = "idle"
duration = "600ms"
transitions = [{ goto = "$previous" }]
```

- [ ] Add `include_str!` loading. In `sm_runner.rs` or a new `src/embedded.rs`:

```rust
pub const DEFAULT_SM_TOML: &str = include_str!("../../assets/default.petstate");

pub fn load_default_sm() -> Arc<CompiledSM> {
    let file: SmFile = toml::from_str(DEFAULT_SM_TOML)
        .expect("default.petstate must parse");
    compile(&file).expect("default.petstate must compile")
}
```

- [ ] Write a test confirming the default SM compiles without errors:

```rust
#[test]
fn default_sm_compiles() {
    let _ = load_default_sm(); // panics if invalid
}
```

- [ ] Run:
```
cargo test default_sm_compiles
```

- [ ] Commit:
```
git add assets/default.petstate src/sprite/sm_runner.rs
git commit -m "feat: embed default.petstate encoding current hardcoded behavior"
```

---

## Task 11: Config schema — remove `AnimTagMap`, add `state_machine`

**Files:**
- Modify: `src/config/schema.rs`
- Modify: `src/app.rs` (stub out broken call sites)
- Modify: `src/tray/config_window.rs` (stub out broken call sites)
- Modify: `src/tray/sprite_editor.rs` (stub out broken call sites)

- [ ] Open `src/config/schema.rs`. Delete `AnimTagMap` struct entirely (all fields and impls).

- [ ] Update `PetConfig`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PetConfig {
    pub id: String,
    pub sheet_path: String,
    pub state_machine: String,  // path or "embedded://default"
    pub x: i32,
    pub y: i32,
    pub scale: u32,
    pub walk_speed: f32,
}

impl Default for PetConfig {
    fn default() -> Self {
        Self {
            id: "esheep".to_string(),
            sheet_path: "embedded://esheep".to_string(),
            state_machine: "embedded://default".to_string(),
            x: 100,
            y: 800,
            scale: 2,
            walk_speed: 80.0,
        }
    }
}
```

- [ ] Fix every compiler error from `AnimTagMap` removal. Known break points:

| File | What breaks | Stub fix |
|---|---|---|
| `src/app.rs` ~line 61 | `cfg.tag_map.fall.as_deref()` | `Some("fall")` — hardcoded until Task 13 wires SMRunner |
| `src/app.rs` `compute_flip()` | pattern-match on `BehaviorState` | return `false` until Task 13 adds `runner.current_facing()` |
| `src/app.rs` `PetInstance.ai` | field type | replace with `runner: Option<SMRunner>` initialized to `None` |
| `src/tray/config_window.rs` | tag map UI section | replace entire tag map section with `ui.label("SM selector (TODO)")` |
| `src/tray/sprite_editor.rs` | `myPetTagMap` write | delete the write call entirely |

- [ ] Build with no errors:
```
cargo build
```
Expected: clean build (no warnings about unused imports are fine, no errors).

- [ ] Commit:
```
git add src/config/schema.rs src/app.rs src/tray/config_window.rs src/tray/sprite_editor.rs
git commit -m "refactor: remove AnimTagMap, stub broken sites — app still runs without SM"
```

---

## Task 12: Remove dead code from `sheet.rs`

**Files:**
- Modify: `src/sprite/sheet.rs`

- [ ] Delete `load_with_tag_map()`, `parse_my_pet_tag_map()`, and any `AnimTagMap`-related parsing from `sheet.rs`.

- [ ] The sprite sheet loading API should now just be `SpriteSheet::load(json, png)` with no tag map involvement.

- [ ] Build and fix any compilation errors:
```
cargo build
```

- [ ] Commit:
```
git add src/sprite/sheet.rs
git commit -m "refactor: remove myPetTagMap and AnimTagMap from sheet loader"
```

---

## Task 13: App integration — wire `SMRunner` into `PetInstance`

**Files:**
- Modify: `src/app.rs`
- Delete: `src/sprite/behavior.rs`

- [ ] In `src/app.rs`, replace `pub ai: BehaviorAi` with `pub runner: SMRunner` in `PetInstance`.

- [ ] Update `PetInstance::new()`: load SM from `cfg.state_machine`:
  - `"embedded://default"` → call `load_default_sm()`
  - Otherwise → read file from `state_machines/` directory, parse, compile

- [ ] Replace all `ai.*` call sites in `app.rs`:

| Old | New |
|---|---|
| `ai.tick(…, tag_map)` | `runner.tick(…, &sheet)` |
| `ai.grab(offset)` | `runner.grab(offset)` |
| `ai.release(vel)` | `runner.release(vel)` |
| `ai.wake()` | `runner.interrupt("wake", None)` |
| `ai.react()` | `runner.interrupt("react", None)` |
| `pet.ai.pet()` | `runner.interrupt("petted", None)` |

- [ ] Update `compute_flip()`:

```rust
fn compute_flip(runner: &SMRunner, sheet: &SpriteSheet) -> bool {
    let Some(facing) = runner.current_facing() else { return false };
    let tag = runner.current_tag(); // or pass tag name separately
    let flip_h = sheet.tags.iter()
        .find(|t| t.name == tag)
        .map(|t| t.flip_h)
        .unwrap_or(false);
    match facing {
        Facing::Right => flip_h,
        Facing::Left  => !flip_h,
    }
}
```

- [ ] Delete `src/sprite/behavior.rs` and remove its `mod` declaration.

- [ ] Build:
```
cargo build
```
Expected: clean build (no `behavior.rs` references).

- [ ] Run all tests:
```
cargo test
```
Expected: all tests pass (or test failures caused by config TOML format change, which is expected and acceptable).

- [ ] Run the app and verify the pet walks, sits, sleeps, and responds to grab/drag/release:
```
cargo run
```

- [ ] Commit:
```
git add src/app.rs src/sprite/sm_runner.rs
git rm src/sprite/behavior.rs
git commit -m "feat: replace BehaviorAi with SMRunner in PetInstance — app runs on data-driven SM"
```

---

## Verification

After all tasks complete:

1. `cargo test` — all tests pass
2. `cargo run` — pet behaves identically to before (same probabilities, same reactions)
3. Manually edit `assets/default.petstate` to change walk weight to 90 — rebuild — pet walks much more often
4. Delete `grabbed` from `default.petstate` — compiler error shown at startup
5. Write a condition with `typo_var < 5` — startup error shows "unknown variable"

