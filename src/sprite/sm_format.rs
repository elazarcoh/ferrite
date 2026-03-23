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
