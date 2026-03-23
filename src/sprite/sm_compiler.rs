use crate::sprite::sm_expr::parse as parse_expr;
use crate::sprite::sm_format::SmFile;
use crate::version::ENGINE_VERSION;
use std::collections::HashSet;
use std::fmt;

#[derive(Debug)]
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

    // 4. Engine primitives: grabbed, fall, thrown must all be present (as states)
    for prim in &["grabbed", "fall", "thrown"] {
        if !sm.states.contains_key(*prim) {
            errors.push(CompileError::MissingEnginePrimitive(prim.to_string()));
        }
    }

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
        for (_event, interrupt) in &state_def.interrupts {
            if let Some(goto) = &interrupt.goto {
                if goto != "$previous" && !all_state_names.contains(goto.as_str()) {
                    errors.push(CompileError::UnknownGotoTarget(
                        state_name.clone(), goto.clone()
                    ));
                }
            }
        }
        // Check fallback
        if let Some(fb) = &state_def.fallback {
            if !required_states.contains(fb.as_str()) {
                errors.push(CompileError::InvalidFallback(state_name.clone(), fb.clone()));
            }
        }
    }
    // Also check global interrupts
    for (_event, interrupt) in &sm.interrupts {
        if let Some(goto) = &interrupt.goto {
            if goto != "$previous" && !all_state_names.contains(goto.as_str()) {
                errors.push(CompileError::UnknownGotoTarget("(global)".to_string(), goto.clone()));
            }
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
            if let Some(state) = sm.states.get(current) {
                if let Some(steps) = &state.steps {
                    for step in steps {
                        stack.push(step.as_str());
                    }
                }
            }
        }
    }

    // 8. Parse all conditions to check for errors
    for (state_name, state_def) in &sm.states {
        // Check transition conditions
        if let Some(transitions) = &state_def.transitions {
            for t in transitions {
                if let Some(cond) = &t.condition {
                    if let Err(msg) = parse_expr(cond) {
                        errors.push(CompileError::ConditionParseError(state_name.clone(), msg));
                    }
                }
            }
        }
        // Check per-state interrupt conditions
        for (_event, interrupt) in &state_def.interrupts {
            if let Some(cond) = &interrupt.condition {
                if let Err(msg) = parse_expr(cond) {
                    errors.push(CompileError::ConditionParseError(state_name.clone(), msg));
                }
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
    for (_event, interrupt) in &sm.interrupts {
        if let Some(cond) = &interrupt.condition {
            if let Err(msg) = parse_expr(cond) {
                errors.push(CompileError::ConditionParseError("(global)".to_string(), msg));
            }
        }
    }

    errors
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
    fn missing_engine_primitive_error() {
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
        assert!(errs.iter().any(|e| matches!(e, CompileError::MissingEnginePrimitive(_))));
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
}
