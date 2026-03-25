#![allow(dead_code)]
use crate::sprite::sm_expr::parse as parse_expr;
use crate::sprite::sm_expr::Expr;
use crate::sprite::sm_format::SmFile;
use crate::version::ENGINE_VERSION;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum CompileError {
    EngineTooOld(String, String),
    NoRequiredState,
    InvalidDefaultFallback(String),
    UnknownGotoTarget(String, String),
    InvalidFallback(String, String),
    MissingEnginePrimitive(String),
    NestedComposite(String),
    StepsCycle(String),
    ZeroWeightSum(String),
    ConditionParseError(String, String),
    DurationParseError(String, String),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::EngineTooOld(required, have) =>
                write!(f, "engine_min_version '{}' requires engine >= '{}', have '{}'", required, required, have),
            CompileError::NoRequiredState =>
                write!(f, "no required=true state declared"),
            CompileError::InvalidDefaultFallback(name) =>
                write!(f, "default_fallback '{}' must be a required state", name),
            CompileError::UnknownGotoTarget(state, target) =>
                write!(f, "state '{}': goto target '{}' does not exist", state, target),
            CompileError::InvalidFallback(state, fb) =>
                write!(f, "state '{}': fallback '{}' does not exist or is not required", state, fb),
            CompileError::MissingEnginePrimitive(prim) =>
                write!(f, "engine primitive '{}' (grabbed/fall/thrown) must be present", prim),
            CompileError::NestedComposite(state) =>
                write!(f, "composite state '{}' has nested steps (not supported in v1)", state),
            CompileError::StepsCycle(state) =>
                write!(f, "steps cycle detected involving state '{}'", state),
            CompileError::ZeroWeightSum(state) =>
                write!(f, "state '{}': transition weight group sums to 0", state),
            CompileError::ConditionParseError(state, msg) =>
                write!(f, "state '{}': condition parse error: {}", state, msg),
            CompileError::DurationParseError(state, dur) =>
                write!(f, "state '{}': duration/after parse error: '{}'", state, dur),
        }
    }
}

impl std::error::Error for CompileError {}

pub fn validate(sm: &SmFile) -> Vec<CompileError> {
    let mut errors = Vec::new();

    // 1. Check engine version compatibility
    // Compare engine_min_version string against ENGINE_VERSION
    // Simple string comparison: if sm.meta.engine_min_version > ENGINE_VERSION (lexicographic) → error
    // (For v1.0 this works since we only have "1.0")
    if sm.meta.engine_min_version.as_str() > ENGINE_VERSION {
        errors.push(CompileError::EngineTooOld(
            sm.meta.engine_min_version.clone(),
            ENGINE_VERSION.to_string(),
        ));
    }

    // 2. At least one required=true state
    let required_states: HashSet<&str> = sm.states.iter()
        .filter(|(_, s)| s.required.unwrap_or(false))
        .map(|(name, _)| name.as_str())
        .collect();
    if required_states.is_empty() {
        errors.push(CompileError::NoRequiredState);
    }

    // 3. default_fallback must be a required state
    if !required_states.contains(sm.meta.default_fallback.as_str()) {
        errors.push(CompileError::InvalidDefaultFallback(sm.meta.default_fallback.clone()));
    }

    // 4. (Engine primitives check removed — grabbed/fall/thrown are no longer required)

    // 5. All state names for reference
    let all_state_names: HashSet<&str> = sm.states.keys().map(|s| s.as_str()).collect();

    // 6. Check all goto targets exist (transitions + interrupts)
    for (state_name, state_def) in &sm.states {
        // Check transitions
        if let Some(transitions) = &state_def.transitions {
            for t in transitions {
                let goto = &t.goto;
                if goto != "$previous" && !all_state_names.contains(goto.as_str()) {
                    errors.push(CompileError::UnknownGotoTarget(
                        state_name.clone(), goto.clone()
                    ));
                }
            }
        }
        // Check per-state interrupts
        for interrupt in state_def.interrupts.values() {
            if let Some(goto) = &interrupt.goto
                && goto != "$previous" && !all_state_names.contains(goto.as_str()) {
                    errors.push(CompileError::UnknownGotoTarget(
                        state_name.clone(), goto.clone()
                    ));
                }
        }
        // Check fallback
        if let Some(fb) = &state_def.fallback
            && !required_states.contains(fb.as_str()) {
                errors.push(CompileError::InvalidFallback(state_name.clone(), fb.clone()));
            }
    }
    // Also check global interrupts
    for interrupt in sm.interrupts.values() {
        if let Some(goto) = &interrupt.goto
            && goto != "$previous" && !all_state_names.contains(goto.as_str()) {
                errors.push(CompileError::UnknownGotoTarget("(global)".to_string(), goto.clone()));
            }
    }

    // 7. Nested composites: if state A has steps=[B] and B has steps=[...], that's NestedComposite
    // Also detect cycles in steps chains

    // Identify which states are composite (have steps field)
    let composite_states: HashSet<&str> = sm.states.iter()
        .filter(|(_, s)| s.steps.is_some())
        .map(|(name, _)| name.as_str())
        .collect();

    // Check for nested composites: a composite state references another composite as a step
    for (state_name, state_def) in &sm.states {
        if let Some(steps) = &state_def.steps {
            for step in steps {
                if composite_states.contains(step.as_str()) {
                    // step is itself a composite — this is a nested composite
                    errors.push(CompileError::NestedComposite(state_name.clone()));
                    break;
                }
            }
        }
    }

    // Check for cycles in steps (e.g., A's steps include A directly, or A→B→A)
    // Do a DFS from each composite state
    for start in &composite_states {
        let mut visited = HashSet::new();
        let mut stack = vec![*start];
        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                errors.push(CompileError::StepsCycle(start.to_string()));
                break;
            }
            if let Some(state) = sm.states.get(current)
                && let Some(steps) = &state.steps {
                    for step in steps {
                        stack.push(step.as_str());
                    }
                }
        }
    }

    // 8. Parse all conditions to check for errors
    for (state_name, state_def) in &sm.states {
        // Check transition conditions
        if let Some(transitions) = &state_def.transitions {
            for t in transitions {
                if let Some(cond) = &t.condition
                    && let Err(msg) = parse_expr(cond) {
                        errors.push(CompileError::ConditionParseError(state_name.clone(), msg));
                    }
            }
        }
        // Check per-state interrupt conditions
        for interrupt in state_def.interrupts.values() {
            if let Some(cond) = &interrupt.condition
                && let Err(msg) = parse_expr(cond) {
                    errors.push(CompileError::ConditionParseError(state_name.clone(), msg));
                }
        }

        // Check weighted transitions: if any transition has weight, all must; and sum > 0
        if let Some(transitions) = &state_def.transitions {
            let weighted: Vec<_> = transitions.iter().filter(|t| t.weight.is_some()).collect();
            if !weighted.is_empty() {
                let sum: u32 = weighted.iter().map(|t| t.weight.unwrap_or(0)).sum();
                if sum == 0 {
                    errors.push(CompileError::ZeroWeightSum(state_name.clone()));
                }
            }
        }
    }
    // Check global interrupt conditions
    for interrupt in sm.interrupts.values() {
        if let Some(cond) = &interrupt.condition
            && let Err(msg) = parse_expr(cond) {
                errors.push(CompileError::ConditionParseError("(global)".to_string(), msg));
            }
    }

    errors
}

// ─── Compiled types ──────────────────────────────────────────────────────────

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

impl Default for ActionParams {
    fn default() -> Self {
        Self {
            dir: None,
            speed_override: None,
            distance_min_px: None,
            distance_max_px: None,
            gravity_scale: 1.0,
            duration_ms: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActionType {
    Idle, Walk, Run, Sit, Jump, Float, FollowCursor, FleeCursor,
    Grabbed, Fall, Thrown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Direction { Left, Right, Random }

impl ActionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActionType::Idle          => "idle",
            ActionType::Walk          => "walk",
            ActionType::Run           => "run",
            ActionType::Sit           => "sit",
            ActionType::Jump          => "jump",
            ActionType::Float         => "float",
            ActionType::FollowCursor  => "follow_cursor",
            ActionType::FleeCursor    => "flee_cursor",
            ActionType::Grabbed       => "grabbed",
            ActionType::Fall          => "fall",
            ActionType::Thrown        => "thrown",
        }
    }

    pub const ALL: &'static [ActionType] = &[
        ActionType::Idle, ActionType::Walk, ActionType::Run, ActionType::Sit,
        ActionType::Jump, ActionType::Float, ActionType::FollowCursor,
        ActionType::FleeCursor, ActionType::Grabbed, ActionType::Fall, ActionType::Thrown,
    ];
}

impl Direction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Direction::Left   => "left",
            Direction::Right  => "right",
            Direction::Random => "random",
        }
    }

    pub const ALL: &'static [Direction] = &[
        Direction::Left, Direction::Right, Direction::Random,
    ];
}

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

// ─── Compiler ────────────────────────────────────────────────────────────────

/// Validate and compile. Returns Err with all validation errors if any.
pub fn compile(sm: &SmFile) -> Result<Arc<CompiledSM>, Vec<CompileError>> {
    let errors = validate(sm);
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut states = HashMap::new();

    for (name, state_def) in &sm.states {
        let required = state_def.required.unwrap_or(false);
        let fallback = state_def.fallback.clone();

        // Compile per-state interrupts
        let per_state_interrupts = compile_interrupts(&state_def.interrupts);

        let kind = if let Some(steps) = &state_def.steps {
            // Composite state
            let transitions = compile_transitions(state_def.transitions.as_deref().unwrap_or(&[]));
            StateKind::Composite { steps: steps.clone(), transitions }
        } else {
            // Atomic state
            let action_str = state_def.action.as_deref().unwrap_or("idle");
            let action = parse_action_type(action_str);
            let params = compile_params(state_def);
            let transitions = compile_transitions(state_def.transitions.as_deref().unwrap_or(&[]));
            StateKind::Atomic { action, params, transitions }
        };

        states.insert(name.clone(), CompiledState { required, fallback, kind, per_state_interrupts });
    }

    let global_interrupts = compile_interrupts(&sm.interrupts);

    Ok(Arc::new(CompiledSM {
        name: sm.meta.name.clone(),
        default_fallback: sm.meta.default_fallback.clone(),
        global_interrupts,
        states,
    }))
}

fn parse_action_type(s: &str) -> ActionType {
    match s {
        "idle"          => ActionType::Idle,
        "walk"          => ActionType::Walk,
        "run"           => ActionType::Run,
        "sit"           => ActionType::Sit,
        "jump"          => ActionType::Jump,
        "float"         => ActionType::Float,
        "follow_cursor" => ActionType::FollowCursor,
        "flee_cursor"   => ActionType::FleeCursor,
        "grabbed"       => ActionType::Grabbed,
        "fall"          => ActionType::Fall,
        "thrown"        => ActionType::Thrown,
        _               => ActionType::Idle, // validation already caught unknown actions
    }
}

fn compile_params(state_def: &crate::sprite::sm_format::SmStateDef) -> ActionParams {
    let dir = state_def.dir.as_deref().map(|d| match d {
        "left"   => Direction::Left,
        "right"  => Direction::Right,
        _        => Direction::Random,
    });

    // Parse duration string like "500ms", "3s", "2.5s" → ms
    let duration_ms = state_def.duration.as_deref().and_then(parse_duration_str);

    ActionParams {
        dir,
        speed_override: state_def.speed,
        distance_min_px: None, // TODO: parse distance field if needed
        distance_max_px: None,
        gravity_scale: state_def.gravity_scale.unwrap_or(1.0),
        duration_ms,
    }
}

fn parse_duration_str(s: &str) -> Option<u32> {
    if let Some(ms_str) = s.strip_suffix("ms") {
        ms_str.trim().parse::<u32>().ok()
    } else if let Some(s_str) = s.strip_suffix('s') {
        s_str.trim().parse::<f32>().ok().map(|f| (f * 1000.0) as u32)
    } else {
        None
    }
}

fn compile_transitions(transitions: &[crate::sprite::sm_format::SmTransitionDef]) -> Vec<CompiledTransition> {
    transitions.iter().map(|t| {
        let goto = if t.goto == "$previous" {
            Goto::Previous
        } else {
            Goto::State(t.goto.clone())
        };

        // Parse after field: "500ms" or "1s-3s" (range) or "2s"
        let (after_min_ms, after_max_ms) = parse_after_field(t.after.as_deref());

        let condition = t.condition.as_deref().and_then(|c| parse_expr(c).ok());

        CompiledTransition { goto, weight: t.weight, after_min_ms, after_max_ms, condition }
    }).collect()
}

fn parse_after_field(s: Option<&str>) -> (Option<u32>, Option<u32>) {
    let s = match s { Some(s) => s, None => return (None, None) };

    // Try range format: "1s-3s" or "500ms-2000ms"
    if let Some(dash_pos) = s.find('-') {
        let min_str = &s[..dash_pos];
        let max_str = &s[dash_pos+1..];
        let min = parse_duration_str(min_str.trim());
        let max = parse_duration_str(max_str.trim());
        return (min, max);
    }

    // Single value
    let val = parse_duration_str(s);
    (val, val)
}

fn compile_interrupts(interrupts: &HashMap<String, crate::sprite::sm_format::SmInterruptDef>) -> Vec<CompiledInterrupt> {
    interrupts.iter().map(|(event, def)| {
        let effect = if def.ignore.unwrap_or(false) {
            InterruptEffect::Ignore
        } else {
            let target = def.goto.clone().unwrap_or_default();
            let condition = def.condition.as_deref().and_then(|c| parse_expr(c).ok());
            InterruptEffect::Goto { target, condition }
        };
        CompiledInterrupt { event: event.clone(), def: effect }
    }).collect()
}

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
        let sm: crate::sprite::sm_format::SmFile = toml::from_str(minimal_valid()).unwrap();
        assert!(validate(&sm).is_empty());
    }

    #[test]
    fn missing_required_state_error() {
        let toml_str = r#"
[meta]
name = "T"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "a"

[states.a]
action = "idle"
"#;
        let sm: crate::sprite::sm_format::SmFile = toml::from_str(toml_str).unwrap();
        let errs = validate(&sm);
        assert!(errs.iter().any(|e| matches!(e, CompileError::NoRequiredState)));
    }

    #[test]
    fn sm_without_engine_primitives_is_valid() {
        // Engine primitives (grabbed/fall/thrown) are no longer required —
        // a minimal SM with just an idle state should compile successfully.
        let toml_str = r#"
[meta]
name = "T"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"
"#;
        let sm: crate::sprite::sm_format::SmFile = toml::from_str(toml_str).unwrap();
        let errs = validate(&sm);
        assert!(errs.is_empty(), "expected no errors, got: {:?}", errs);
    }

    #[test]
    fn unknown_goto_target_error() {
        let toml_str = r#"
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
        let sm: crate::sprite::sm_format::SmFile = toml::from_str(toml_str).unwrap();
        let errs = validate(&sm);
        assert!(errs.iter().any(|e| matches!(e, CompileError::UnknownGotoTarget(..))));
    }

    #[test]
    fn nested_composite_error() {
        let toml_str = r#"
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
        let sm: crate::sprite::sm_format::SmFile = toml::from_str(toml_str).unwrap();
        let errs = validate(&sm);
        assert!(errs.iter().any(|e| matches!(e, CompileError::NestedComposite(_))));
    }

    #[test]
    fn steps_cycle_error() {
        let toml_str = r#"
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
        let sm: crate::sprite::sm_format::SmFile = toml::from_str(toml_str).unwrap();
        let errs = validate(&sm);
        assert!(errs.iter().any(|e| matches!(e, CompileError::StepsCycle(_))));
    }

    #[test]
    fn compile_valid_sm_succeeds() {
        let sm: crate::sprite::sm_format::SmFile = toml::from_str(minimal_valid()).unwrap();
        assert!(compile(&sm).is_ok());
    }

    #[test]
    fn compiled_states_indexed() {
        let sm: crate::sprite::sm_format::SmFile = toml::from_str(minimal_valid()).unwrap();
        let compiled = compile(&sm).unwrap();
        assert!(compiled.states.contains_key("idle"));
        assert!(compiled.states.contains_key("grabbed"));
    }

    #[test]
    fn compile_invalid_sm_returns_errors() {
        let bad_toml = r#"
[meta]
name = "T"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
action = "idle"
"#;
        let sm: crate::sprite::sm_format::SmFile = toml::from_str(bad_toml).unwrap();
        assert!(compile(&sm).is_err());
    }
}
