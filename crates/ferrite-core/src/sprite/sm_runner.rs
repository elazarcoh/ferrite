#![allow(dead_code)]
use std::sync::Arc;
use crate::sprite::sm_compiler::{CompiledSM, ActionType, Direction};
use crate::sprite::sm_expr::ConditionVars;
use crate::sprite::sheet::SpriteSheet;
use crate::sprite::sm_format::SmFile;
use crate::sprite::sm_compiler::compile;

const GRAVITY: f32 = 980.0;

pub const DEFAULT_SM_TOML: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/default.petstate"));

pub fn load_default_sm() -> Arc<CompiledSM> {
    let file: SmFile = toml::from_str(DEFAULT_SM_TOML)
        .expect("default.petstate must parse");
    compile(&file).expect("default.petstate must compile")
}

#[derive(Debug, Clone)]
pub enum ActiveState {
    Named(String),
    Fall { vy: f32 },
    Thrown { vx: f32, vy: f32 },
    Grabbed { cursor_offset: (i32, i32) },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Facing { Left, Right }

#[derive(Debug, Clone)]
pub struct TransitionLogEntry {
    pub from_state: String,
    pub to_state: String,
    pub reason: String,
}

pub struct SMRunner {
    pub sm: Arc<CompiledSM>,
    pub active: ActiveState,
    pub previous_named: Option<String>,
    state_time_ms: u32,
    pub step_index: usize,
    walk_remaining_px: f32,
    pub facing: Facing,
    walk_speed: f32,
    rng: u64,
    next_transition_ms: u32,
    last_vars: ConditionVars,
    transition_log: Vec<TransitionLogEntry>,
    current_tag: String,
    // Debug tools
    pub force_state: Option<String>,
    pub release_force: bool,
    pub step_mode: bool,
    pub step_advance: bool,
}

impl SMRunner {
    pub fn new(sm: Arc<CompiledSM>, walk_speed: f32) -> Self {
        let initial_state = sm.default_fallback.clone();
        let mut runner = Self {
            sm: sm.clone(),
            active: ActiveState::Named(initial_state.clone()),
            previous_named: None,
            state_time_ms: 0,
            step_index: 0,
            walk_remaining_px: 0.0,
            facing: Facing::Right,
            walk_speed,
            rng: 12345,
            next_transition_ms: 0,
            last_vars: ConditionVars::default(),
            transition_log: Vec::new(),
            current_tag: initial_state.clone(),
            force_state: None,
            release_force: false,
            step_mode: false,
            step_advance: false,
        };
        runner.enter_state(&initial_state);
        runner
    }

    pub fn current_facing(&self) -> Facing {
        self.facing.clone()
    }

    /// Returns the name of the current Named state, or the physics state name.
    pub fn current_state_name(&self) -> &str {
        match &self.active {
            ActiveState::Named(name) => name.as_str(),
            ActiveState::Fall { .. } => "fall",
            ActiveState::Thrown { .. } => "thrown",
            ActiveState::Grabbed { .. } => "grabbed",
        }
    }

    /// For a composite state, returns the name of the current step.
    /// For atomic or physics states, returns the same as current_state_name().
    fn active_display_state_name(&self) -> &str {
        if let ActiveState::Named(composite_name) = &self.active {
            if let Some(state) = self.sm.states.get(composite_name.as_str()) {
                use crate::sprite::sm_compiler::StateKind;
                if let StateKind::Composite { steps, .. } = &state.kind
                    && let Some(step_name) = steps.get(self.step_index) {
                        return step_name.as_str();
                    }
            }
            composite_name.as_str()
        } else {
            self.current_state_name()
        }
    }

    /// Returns the last captured condition variables (updated every tick).
    pub fn last_condition_vars(&self) -> &ConditionVars {
        &self.last_vars
    }

    /// Returns the last up-to-10 transition log entries (oldest first).
    pub fn transition_log(&self) -> &[TransitionLogEntry] {
        &self.transition_log
    }

    /// Handle a named interrupt event (e.g. "grabbed", "petted").
    pub fn interrupt(&mut self, event: &str, cursor_offset: Option<(i32, i32)>) {
        // Check global interrupts first
        if let Some(interrupt) = self.sm.global_interrupts.iter()
            .find(|i| i.event == event)
            .cloned()
        {
            use crate::sprite::sm_compiler::InterruptEffect;
            match interrupt.def {
                InterruptEffect::Ignore => return,
                InterruptEffect::Goto { target, condition } => {
                    let ok = if let Some(cond) = &condition {
                        crate::sprite::sm_expr::eval(cond, &self.last_vars).unwrap_or(false)
                    } else {
                        true
                    };
                    if ok {
                        if event == "grabbed" {
                            let offset = cursor_offset.unwrap_or((0, 0));
                            self.grab(offset);
                            return;
                        }
                        // set_previous_from_current records the composite name (not the step)
                        self.set_previous_from_current();
                        let from = self.current_state_name().to_string();
                        self.enter_state(&target.clone());
                        self.log_transition(&from, &target, "interrupt");
                    }
                }
            }
            return;
        }

        // Fallback for grabbed
        if event == "grabbed" {
            let offset = cursor_offset.unwrap_or((0, 0));
            self.grab(offset);
        }
    }

    pub fn grab(&mut self, cursor_offset: (i32, i32)) {
        self.set_previous_from_current();
        self.active = ActiveState::Grabbed { cursor_offset };
        self.state_time_ms = 0;
    }

    pub fn release(&mut self, velocity: (f32, f32)) {
        let (vx, vy) = velocity;
        if vx.abs() > 10.0 || vy.abs() > 10.0 {
            self.active = ActiveState::Thrown { vx, vy };
        } else {
            self.active = ActiveState::Fall { vy: 0.0 };
        }
        self.state_time_ms = 0;
    }

    /// Transition from any Named state into a free-fall.
    /// Called by the web renderer when the pet walks off the edge of a DOM surface.
    pub fn start_fall(&mut self) {
        self.set_previous_from_current();
        self.active = ActiveState::Fall { vy: 0.0 };
        self.state_time_ms = 0;
    }

    fn enter_state(&mut self, name: &str) {
        self.state_time_ms = 0;
        self.step_index = 0;
        self.next_transition_ms = 0;
        self.active = ActiveState::Named(name.to_string());
    }

    fn set_previous_from_current(&mut self) {
        // Always records the composite state name (not the sub-step), since
        // active is Named(composite_name) during composite execution.
        if let ActiveState::Named(name) = &self.active {
            self.previous_named = Some(name.clone());
        }
    }

    fn log_transition(&mut self, from: &str, to: &str, reason: &str) {
        if self.transition_log.len() >= 10 {
            self.transition_log.remove(0);
        }
        self.transition_log.push(TransitionLogEntry {
            from_state: from.to_string(),
            to_state: to.to_string(),
            reason: reason.to_string(),
        });
    }

    fn lcg_rand(&mut self) -> u64 {
        self.rng = self.rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.rng >> 33
    }

    fn rand_range(&mut self, min: u32, max: u32) -> u32 {
        if min >= max { return min; }
        let range = (max - min) as u64;
        min + (self.lcg_rand() % range) as u32
    }

    /// Tick the state machine for one frame.
    /// Returns the tag name to use for the current animation frame.
    #[allow(clippy::too_many_arguments)]
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
        let dt = delta_ms as f32 / 1000.0;

        // 1. Handle release_force
        if self.release_force {
            self.release_force = false;
            self.force_state = None;
        }

        // 2. Handle debug force
        if let Some(name) = self.force_state.take() {
            self.set_previous_from_current();
            let old = self.current_state_name().to_string();
            self.enter_state(&name.clone());
            self.log_transition(&old, &name, "forced");
        }

        // 3. Accumulate time
        self.state_time_ms = self.state_time_ms.saturating_add(delta_ms);

        // 4. Execute action physics
        self.execute_action(dt, x, y, screen_w, pet_w, pet_h, floor_y);

        // 5. Evaluate transitions (unless step_mode without advance)
        if !self.step_mode || self.step_advance {
            self.step_advance = false;
            self.try_transitions(screen_w, pet_w, pet_h, floor_y);
        }

        // 6. Resolve and store tag name, then return reference to stored field
        self.resolve_tag(sheet)
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_action(&mut self, dt: f32, x: &mut i32, y: &mut i32, screen_w: i32, pet_w: i32, _pet_h: i32, floor_y: i32) {
        match self.active.clone() {
            ActiveState::Named(name) => {
                // Determine the effective state name to use for action lookup.
                // For a composite state, we execute the current step's action.
                let effective_name = {
                    use crate::sprite::sm_compiler::StateKind;
                    if let Some(state) = self.sm.states.get(name.as_str()) {
                        if let StateKind::Composite { steps, .. } = &state.kind {
                            steps.get(self.step_index).cloned().unwrap_or(name.clone())
                        } else {
                            name.clone()
                        }
                    } else {
                        name.clone()
                    }
                };

                let state = match self.sm.states.get(effective_name.as_str()) {
                    Some(s) => s.clone(),
                    None => return,
                };
                use crate::sprite::sm_compiler::StateKind;
                if let StateKind::Atomic { action, params, .. } = &state.kind {
                    let speed = params.speed_override.unwrap_or(self.walk_speed);
                    match action {
                        ActionType::Walk | ActionType::Run => {
                            let eff_speed = if *action == ActionType::Run { speed * 2.0 } else { speed };
                            let dx = eff_speed * dt;
                            self.walk_remaining_px -= dx;

                            let dir_sign: i32 = match &self.facing {
                                Facing::Right => 1,
                                Facing::Left => -1,
                            };
                            let new_x = *x + (dx as i32) * dir_sign;

                            if new_x <= 0 {
                                *x = 0;
                                self.facing = Facing::Right;
                            } else if new_x + pet_w >= screen_w {
                                *x = screen_w - pet_w;
                                self.facing = Facing::Left;
                            } else {
                                *x = new_x;
                            }

                            if self.walk_remaining_px <= 0.0 {
                                self.walk_remaining_px = 0.0;
                                // Transition to idle (walk distance done)
                                let fallback = self.sm.default_fallback.clone();
                                self.transition_to(&fallback, "walk_done");
                            }
                        }
                        ActionType::Fall => {
                            // Named state with action=fall: treat same as ActiveState::Fall
                            // Normally handled via ActiveState::Fall variant
                        }
                        ActionType::Grabbed | ActionType::Thrown => {
                            // These are handled via ActiveState variants
                        }
                        _ => {
                            // Idle, Sit, Sleep, Float, etc. — no position change in action
                        }
                    }
                }
            }

            ActiveState::Fall { vy } => {
                let mut vy = vy;
                vy += GRAVITY * dt;
                let new_y = *y + (vy * dt) as i32;
                if new_y >= floor_y {
                    *y = floor_y;
                    self.last_vars.on_surface = true;
                    let fallback = self.sm.default_fallback.clone();
                    self.transition_to(&fallback, "landed");
                } else {
                    *y = new_y;
                    self.active = ActiveState::Fall { vy };
                    self.last_vars.on_surface = false;
                }
            }

            ActiveState::Thrown { vx, vy } => {
                let mut vx = vx;
                let mut vy = vy;
                vy += GRAVITY * dt;
                let new_x = *x + (vx * dt) as i32;
                let new_y = *y + (vy * dt) as i32;

                // Horizontal bounce
                let (clamped_x, new_vx) = if new_x <= 0 {
                    (0, vx.abs())
                } else if new_x + pet_w >= screen_w {
                    (screen_w - pet_w, -vx.abs())
                } else {
                    (new_x, vx)
                };
                vx = new_vx;

                if new_y >= floor_y {
                    *x = clamped_x;
                    *y = floor_y;
                    self.last_vars.on_surface = true;
                    // Transition to fall state (land from thrown)
                    self.active = ActiveState::Fall { vy: 0.0 };
                    self.state_time_ms = 0;
                } else {
                    *x = clamped_x;
                    *y = new_y;
                    self.active = ActiveState::Thrown { vx, vy };
                    self.last_vars.on_surface = false;
                }
            }

            ActiveState::Grabbed { .. } => {
                // Position driven externally; no physics here
            }
        }
    }

    fn try_transitions(&mut self, screen_w: i32, pet_w: i32, pet_h: i32, floor_y: i32) {
        // Only Named states have data-driven transitions
        let state_name = match &self.active {
            ActiveState::Named(n) => n.clone(),
            _ => return, // physics states transition via execute_action
        };

        let state = match self.sm.states.get(&state_name) {
            Some(s) => s.clone(),
            None => return,
        };

        use crate::sprite::sm_compiler::StateKind;
        match &state.kind {
            StateKind::Composite { steps, transitions } => {
                let steps = steps.clone();
                let composite_transitions = transitions.clone();

                // Get the current step
                if let Some(step_name) = steps.get(self.step_index) {
                    let step_name = step_name.clone();
                    let step = match self.sm.states.get(step_name.as_str()) {
                        Some(s) => s.clone(),
                        None => {
                            // Step doesn't exist, advance to next step
                            self.step_index += 1;
                            self.state_time_ms = 0;
                            self.next_transition_ms = 0;
                            return;
                        }
                    };

                    // Check if the step's duration has elapsed
                    let step_done = if let StateKind::Atomic { params, .. } = &step.kind {
                        if let Some(dur) = params.duration_ms {
                            self.state_time_ms >= dur
                        } else {
                            false // no duration — step runs until composite is interrupted
                        }
                    } else {
                        false
                    };

                    if step_done {
                        self.step_index += 1;
                        self.state_time_ms = 0;
                        self.next_transition_ms = 0;

                        if self.step_index >= steps.len() {
                            // All steps done — fire composite's own transitions
                            for t in &composite_transitions {
                                let after_ok = match (t.after_min_ms, t.after_max_ms) {
                                    (None, None) => true,
                                    (Some(min), _) => self.state_time_ms >= min,
                                    (None, Some(max)) => self.state_time_ms >= max,
                                };
                                if !after_ok { continue; }
                                let cond_ok = if let Some(cond) = &t.condition {
                                    crate::sprite::sm_expr::eval(cond, &self.last_vars).unwrap_or(false)
                                } else {
                                    true
                                };
                                if cond_ok {
                                    let goto = self.resolve_goto(&t.goto);
                                    self.transition_to(&goto, "composite_done");
                                    return;
                                }
                            }
                            // No transition fired — go to default_fallback
                            let fallback = self.sm.default_fallback.clone();
                            self.transition_to(&fallback, "composite_done_fallback");
                        }
                        // else: step advanced, continue in next tick
                    }
                } else {
                    // step_index out of range — fire composite transitions immediately
                    for t in &composite_transitions {
                        let cond_ok = if let Some(cond) = &t.condition {
                            crate::sprite::sm_expr::eval(cond, &self.last_vars).unwrap_or(false)
                        } else {
                            true
                        };
                        if cond_ok {
                            let goto = self.resolve_goto(&t.goto);
                            self.transition_to(&goto, "composite_done");
                            return;
                        }
                    }
                    let fallback = self.sm.default_fallback.clone();
                    self.transition_to(&fallback, "composite_done_fallback");
                }
            }

            StateKind::Atomic { action, transitions, .. } => {
                // Don't evaluate data-driven transitions mid-walk; walk completion
                // is handled by execute_action calling transition_to(fallback, "walk_done").
                if (*action == ActionType::Walk || *action == ActionType::Run)
                    && self.walk_remaining_px > 0.0
                {
                    return;
                }
                let transitions = transitions.clone();
                self.try_atomic_transitions(&transitions);
            }
        }

        let _ = (screen_w, pet_w, pet_h, floor_y); // suppress unused warnings
    }

    /// Process sequential or weighted transitions for an atomic state.
    fn try_atomic_transitions(&mut self, transitions: &[crate::sprite::sm_compiler::CompiledTransition]) {
        if transitions.is_empty() { return; }

        // Separate weighted and unweighted transitions
        let has_weights = transitions.iter().any(|t| t.weight.is_some());

        if has_weights {
            // Weighted random: pick one whose after/condition is satisfied
            let eligible: Vec<_> = transitions.iter().filter(|t| {
                let after_ok = match (t.after_min_ms, t.after_max_ms) {
                    (None, None) => true,
                    (Some(min), _) => self.state_time_ms >= min,
                    (None, Some(max)) => self.state_time_ms >= max,
                };
                if !after_ok { return false; }
                if let Some(cond) = &t.condition {
                    crate::sprite::sm_expr::eval(cond, &self.last_vars).unwrap_or(false)
                } else {
                    true
                }
            }).collect();

            if eligible.is_empty() { return; }

            // Compute threshold for this state (randomize once when entering)
            if self.next_transition_ms == 0 {
                if let (Some(min), Some(max)) = (eligible[0].after_min_ms, eligible[0].after_max_ms) {
                    self.next_transition_ms = self.rand_range(min, max);
                } else {
                    self.next_transition_ms = 1;
                }
            }

            if self.state_time_ms < self.next_transition_ms { return; }

            // Weighted pick
            let total_weight: u32 = eligible.iter().map(|t| t.weight.unwrap_or(1)).sum();
            let mut r = (self.lcg_rand() as u32) % total_weight;
            for t in &eligible {
                let w = t.weight.unwrap_or(1);
                if r < w {
                    let goto = self.resolve_goto(&t.goto);
                    self.transition_to(&goto, "weighted");
                    return;
                }
                r -= w;
            }
        } else {
            // Sequential: first satisfied transition wins
            for t in transitions {
                let after_ok = match (t.after_min_ms, t.after_max_ms) {
                    (None, None) => true,
                    (Some(min), _) => {
                        if self.next_transition_ms == 0 {
                            self.next_transition_ms = if let Some(max) = t.after_max_ms {
                                self.rand_range(min, max)
                            } else {
                                min
                            };
                        }
                        self.state_time_ms >= self.next_transition_ms
                    }
                    (None, Some(max)) => self.state_time_ms >= max,
                };
                if !after_ok { continue; }

                let cond_ok = if let Some(cond) = &t.condition {
                    crate::sprite::sm_expr::eval(cond, &self.last_vars).unwrap_or(false)
                } else {
                    true
                };

                if cond_ok {
                    let goto = self.resolve_goto(&t.goto);
                    self.transition_to(&goto, "condition");
                    return;
                }
            }
        }
    }

    fn resolve_goto(&self, goto: &crate::sprite::sm_compiler::Goto) -> String {
        match goto {
            crate::sprite::sm_compiler::Goto::State(name) => name.clone(),
            crate::sprite::sm_compiler::Goto::Previous => {
                self.previous_named.clone()
                    .unwrap_or_else(|| self.sm.default_fallback.clone())
            }
        }
    }

    fn transition_to(&mut self, target: &str, reason: &str) {
        let from = self.current_state_name().to_string();

        // Determine direction for walk/run states
        if let Some(state) = self.sm.states.get(target).cloned() {
            use crate::sprite::sm_compiler::StateKind;
            if let StateKind::Atomic { action, params, .. } = &state.kind
                && (*action == ActionType::Walk || *action == ActionType::Run) {
                    self.facing = match params.dir {
                        Some(Direction::Left) => Facing::Left,
                        Some(Direction::Right) => Facing::Right,
                        _ => {
                            // Random direction
                            if self.lcg_rand().is_multiple_of(2) { Facing::Right } else { Facing::Left }
                        }
                    };
                    // Set walk distance
                    let dist = match (params.distance_min_px, params.distance_max_px) {
                        (Some(min), Some(max)) => self.rand_range(min as u32, max as u32) as f32,
                        (Some(min), None) => min,
                        _ => 400.0, // default
                    };
                    self.walk_remaining_px = dist;
                }
        }

        self.log_transition(&from, target, reason);
        self.set_previous_from_current();
        self.enter_state(target);
    }

    /// Replace the running state machine with a new one, resetting all transient state.
    /// Preserved across the swap: `facing`, `walk_speed`, `rng`, `last_vars`.
    pub fn replace_sm(&mut self, new_sm: Arc<CompiledSM>) {
        let default = new_sm.default_fallback.clone();
        self.sm = new_sm;
        self.active = ActiveState::Named(default.clone());
        self.previous_named = None;
        self.state_time_ms = 0;
        self.step_index = 0;
        self.walk_remaining_px = 0.0;
        self.next_transition_ms = 0;
        self.force_state = None;
        self.release_force = false;
        self.step_mode = false;
        self.step_advance = false;
        self.transition_log.clear();
        self.current_tag = default;
        // Preserved: facing, walk_speed, rng, last_vars
    }

    fn resolve_tag(&mut self, sheet: &SpriteSheet) -> &str {
        let sm_name = self.sm.name.clone();
        let display_name = self.active_display_state_name().to_string();

        let mut candidate = display_name;
        let resolved = loop {
            if let Some(tag) = sheet.resolve_tag(&sm_name, &candidate) {
                break tag.to_string();
            }
            let fallback_opt = self.sm.states.get(&candidate)
                .and_then(|s| s.fallback.clone());
            if let Some(fb) = fallback_opt {
                candidate = fb;
                continue;
            }
            let default_fb = self.sm.default_fallback.clone();
            if let Some(tag) = sheet.resolve_tag(&sm_name, &default_fb) {
                break tag.to_string();
            }
            break default_fb;
        };

        self.current_tag = resolved;
        self.current_tag.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprite::sm_compiler::compile;
    use crate::sprite::sm_format::SmFile;
    use crate::sprite::sheet::{SpriteSheet, Frame, FrameTag, TagDirection};
    use image::RgbaImage;

    fn make_runner() -> SMRunner {
        let sm_toml = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/default.petstate"));
        let file: SmFile = toml::from_str(sm_toml).unwrap();
        let compiled = compile(&file).unwrap();
        SMRunner::new(compiled, 80.0)
    }

    fn mock_sheet() -> SpriteSheet {
        let image = RgbaImage::new(32, 32);
        let frames = vec![
            Frame { x: 0, y: 0, w: 32, h: 32, duration_ms: 100 },
        ];
        let tags = vec![
            FrameTag { name: "idle".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
            FrameTag { name: "walk".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
            FrameTag { name: "sit".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
            FrameTag { name: "grabbed".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
            FrameTag { name: "petted".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        ];
        SpriteSheet { image, frames, tags, sm_mappings: std::collections::HashMap::new(), chromakey: crate::sprite::sheet::ChromakeyConfig::default() }
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
    fn start_fall_transitions_named_to_fall() {
        let mut r = make_runner();
        assert!(matches!(&r.active, ActiveState::Named(_)));
        r.start_fall();
        assert!(matches!(&r.active, ActiveState::Fall { vy } if *vy == 0.0));
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

    #[test]
    fn composite_runs_steps_in_order() {
        let toml_str = r#"
[meta]
name = "T"
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
transitions = []

[states.thrown]
required = true
action = "thrown"
transitions = []

[states.step_a]
action = "idle"
duration = "100ms"

[states.step_b]
action = "sit"
duration = "100ms"

[states.routine]
steps = ["step_a", "step_b"]
transitions = [{ goto = "idle" }]
"#;
        let file: SmFile = toml::from_str(toml_str).unwrap();
        let compiled = compile(&file).unwrap();
        let mut r = SMRunner::new(compiled, 80.0);

        // Force into routine
        r.force_state = Some("routine".to_string());
        let sheet = mock_sheet();
        let mut x = 0;
        let mut y = 800;

        // Tick once to process force (1ms delta — not enough to expire step)
        r.tick(1, &mut x, &mut y, 1920, 32, 32, 800, &sheet);
        assert!(matches!(&r.active, ActiveState::Named(n) if n == "routine"), "should be in routine");
        assert_eq!(r.step_index, 0, "should be on step 0");

        // Tick past step_a duration (100ms)
        r.tick(110, &mut x, &mut y, 1920, 32, 32, 800, &sheet);
        assert_eq!(r.step_index, 1, "should advance to step 1");

        // Tick past step_b duration (100ms)
        r.tick(110, &mut x, &mut y, 1920, 32, 32, 800, &sheet);
        // After all steps done, should go to idle
        assert!(matches!(&r.active, ActiveState::Named(n) if n == "idle"), "should return to idle");
    }

    #[test]
    fn default_sm_compiles() {
        let _ = load_default_sm(); // panics if invalid
    }

    #[test]
    fn previous_returns_to_interrupted_named_state() {
        let mut r = make_runner();
        // Start in idle
        assert!(matches!(&r.active, ActiveState::Named(n) if n == "idle"));
        // Trigger petted interrupt
        r.interrupt("petted", None);
        assert!(matches!(&r.active, ActiveState::Named(n) if n == "petted"));
        // previous_named should be "idle"
        assert_eq!(r.previous_named.as_deref(), Some("idle"));
    }

    fn make_minimal_sm(name: &str, default_state: &str) -> Arc<CompiledSM> {
        let toml_str = format!(r#"
[meta]
name = "{name}"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "{default_state}"

[states.{default_state}]
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
transitions = []

[states.thrown]
required = true
action = "thrown"
transitions = []
"#);
        let file: SmFile = toml::from_str(&toml_str).unwrap();
        compile(&file).unwrap()
    }

    #[test]
    fn replace_sm_resets_state() {
        let mut r = make_runner();
        // Advance into a non-default state
        r.force_state = Some("petted".to_string());
        r.state_time_ms = 999;
        r.step_index = 3;
        r.walk_remaining_px = 150.0;
        r.transition_log.push(TransitionLogEntry {
            from_state: "idle".to_string(),
            to_state: "petted".to_string(),
            reason: "test".to_string(),
        });

        let second_sm = make_minimal_sm("second", "idle");
        r.replace_sm(second_sm.clone());

        assert!(
            matches!(&r.active, ActiveState::Named(n) if *n == second_sm.default_fallback),
            "active should be Named(default_fallback)"
        );
        assert_eq!(r.state_time_ms, 0);
        assert_eq!(r.step_index, 0);
        assert_eq!(r.walk_remaining_px, 0.0);
        assert!(r.transition_log.is_empty());
    }

    #[test]
    fn replace_sm_preserves_facing() {
        let mut r = make_runner();
        r.facing = Facing::Left;
        let second_sm = make_minimal_sm("second", "idle");
        r.replace_sm(second_sm);
        assert_eq!(r.facing, Facing::Left);
    }

    #[test]
    fn replace_sm_preserves_walk_speed() {
        let sm_toml = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/default.petstate"));
        let file: SmFile = toml::from_str(sm_toml).unwrap();
        let compiled = compile(&file).unwrap();
        let mut r = SMRunner::new(compiled, 2.5);

        let second_sm = make_minimal_sm("second", "idle");
        r.replace_sm(second_sm);
        assert_eq!(r.walk_speed, 2.5);
    }

    #[test]
    fn walk_facing_does_not_change_during_single_episode() {
        let sm_toml = r#"
[meta]
name = "WalkTest"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"
transitions = [{ goto = "walk" }]

[states.walk]
action = "walk"
dir = "right"
transitions = []

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
"#;
        let file: SmFile = toml::from_str(sm_toml).unwrap();
        let compiled = compile(&file).unwrap();
        let mut r = SMRunner::new(compiled, 80.0);
        let sheet = mock_sheet();

        let mut x: i32 = 500;
        let mut y: i32 = 800;
        let screen_w = 1920;
        let pet_w = 32;
        let pet_h = 32;
        let floor_y = 800;

        r.tick(16, &mut x, &mut y, screen_w, pet_w, pet_h, floor_y, &sheet);

        assert!(
            matches!(&r.active, ActiveState::Named(n) if n == "walk"),
            "expected walk state after first tick, got: {:?}", r.active
        );

        let initial_facing = r.current_facing();

        for _ in 0..30 {
            r.tick(16, &mut x, &mut y, screen_w, pet_w, pet_h, floor_y, &sheet);
            assert!(
                matches!(&r.active, ActiveState::Named(n) if n == "walk"),
                "runner left walk state unexpectedly"
            );
            assert_eq!(
                r.current_facing(),
                initial_facing,
                "facing changed unexpectedly during a single walk episode"
            );
        }
    }
}
