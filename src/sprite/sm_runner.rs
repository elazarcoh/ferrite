#![allow(dead_code)]
use std::sync::Arc;
use crate::sprite::sm_compiler::{CompiledSM, ActionType, Direction};
use crate::sprite::sm_expr::ConditionVars;
use crate::sprite::sheet::SpriteSheet;
use crate::sprite::sm_format::SmFile;
use crate::sprite::sm_compiler::compile;

const GRAVITY: f32 = 980.0;

pub const DEFAULT_SM_TOML: &str = include_str!("../../assets/default.petstate");

pub fn load_default_sm() -> Arc<CompiledSM> {
    let file: SmFile = toml::from_str(DEFAULT_SM_TOML)
        .expect("default.petstate must parse");
    compile(&file).expect("default.petstate must compile")
}

/// Data passed to `on_collide` when two pets begin overlapping.
#[derive(Debug, Clone)]
pub struct CollideData {
    /// Describes the geometry/role of this collision (e.g. "head_on", "fell_on").
    pub collide_type: String,
    /// Relative velocity X (this pet minus other pet), in px/s.
    pub vx: f32,
    /// Relative velocity Y (this pet minus other pet), in px/s.
    pub vy: f32,
    /// Magnitude of the relative velocity vector.
    pub v: f32,
}

#[derive(Debug, Clone)]
pub enum ActiveState {
    Named(String),
    /// Airborne physics (falling or thrown). `vx == 0` is a pure fall;
    /// `vx.abs() > 10` is a throw with horizontal bounce.
    Airborne { vx: f32, vy: f32 },
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
            ActiveState::Airborne { vx, .. } => {
                if vx.abs() > 10.0 { "thrown" } else { "fall" }
            }
            ActiveState::Grabbed { .. } => "grabbed",
        }
    }

    /// Returns the current velocity of this pet in px/s as `(vx, vy)`.
    /// Walk/Run states return the speed in the current facing direction;
    /// Airborne returns physics velocities; Grabbed/Idle return `(0.0, 0.0)`.
    pub fn speed(&self) -> (f32, f32) {
        match &self.active {
            ActiveState::Airborne { vx, vy } => (*vx, *vy),
            ActiveState::Grabbed { .. } => (0.0, 0.0),
            ActiveState::Named(name) => {
                if let Some(state) = self.sm.states.get(name.as_str()) {
                    use crate::sprite::sm_compiler::StateKind;
                    if let StateKind::Atomic { action, params, .. } = &state.kind {
                        let spd = params.speed_override.unwrap_or(self.walk_speed);
                        let eff = if *action == ActionType::Run { spd * 2.0 } else { spd };
                        if *action == ActionType::Walk || *action == ActionType::Run {
                            let sign = match self.facing { Facing::Right => 1.0, Facing::Left => -1.0 };
                            return (eff * sign, 0.0);
                        }
                    }
                }
                (0.0, 0.0)
            }
        }
    }

    /// Called when this pet begins overlapping with another pet (edge-triggered).
    /// Fires a "collide" interrupt and stores the collision data for condition evaluation.
    pub fn on_collide(&mut self, data: CollideData) {
        log::debug!(
            "on_collide: type={} vx={:.1} vy={:.1} v={:.1}",
            data.collide_type, data.vx, data.vy, data.v
        );
        self.last_vars.collide_type = data.collide_type.clone();
        self.last_vars.collide_vx = data.vx;
        self.last_vars.collide_vy = data.vy;
        self.last_vars.collide_v = data.v;
        self.interrupt("collide", None);
        self.last_vars.collide_type = String::new();
        self.last_vars.collide_vx = 0.0;
        self.last_vars.collide_vy = 0.0;
        self.last_vars.collide_v = 0.0;
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

    /// Update externally-computed condition variables.
    /// Called from App::update() each frame after the tick loop.
    #[allow(clippy::too_many_arguments)]
    pub fn update_env_vars(
        &mut self,
        cursor_dist: f32,
        hour: u32,
        focused_app: String,
        screen_h: f32,
        pet_count: u32,
        other_pet_dist: f32,
        surface_w: f32,
        surface_label: String,
    ) {
        self.last_vars.cursor_dist = cursor_dist;
        self.last_vars.hour = hour;
        self.last_vars.focused_app = focused_app;
        self.last_vars.screen_h = screen_h;
        self.last_vars.pet_count = pet_count;
        self.last_vars.other_pet_dist = other_pet_dist;
        self.last_vars.surface_w = surface_w;
        self.last_vars.surface_label = surface_label;
    }

    /// Returns the last up-to-10 transition log entries (oldest first).
    pub fn transition_log(&self) -> &[TransitionLogEntry] {
        &self.transition_log
    }

    /// Handle a named interrupt event (e.g. "grabbed", "petted", "collide").
    /// Checks global interrupts first, then per-state interrupts for the current named state.
    pub fn interrupt(&mut self, event: &str, cursor_offset: Option<(i32, i32)>) {
        // 1. Global interrupts
        if let Some(intr) = self.sm.global_interrupts.iter().find(|i| i.event == event).cloned() {
            self.apply_interrupt_effect(intr.def, event, cursor_offset);
            return;
        }
        // 2. Per-state interrupts (current Named state only)
        if let ActiveState::Named(state_name) = self.active.clone() {
            if let Some(state) = self.sm.states.get(&state_name).cloned() {
                if let Some(intr) = state.per_state_interrupts.iter().find(|i| i.event == event).cloned() {
                    self.apply_interrupt_effect(intr.def, event, cursor_offset);
                    return;
                }
            }
        }
        // 3. Fallback for grabbed with no matching interrupt defined
        if event == "grabbed" {
            let offset = cursor_offset.unwrap_or((0, 0));
            self.grab(offset);
        }
    }

    fn apply_interrupt_effect(
        &mut self,
        effect: crate::sprite::sm_compiler::InterruptEffect,
        event: &str,
        cursor_offset: Option<(i32, i32)>,
    ) {
        use crate::sprite::sm_compiler::InterruptEffect;
        match effect {
            InterruptEffect::Ignore => {}
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
                    self.set_previous_from_current();
                    let from = self.current_state_name().to_string();
                    self.enter_state(&target.clone());
                    self.log_transition(&from, &target, "interrupt");
                }
            }
        }
    }

    pub fn grab(&mut self, cursor_offset: (i32, i32)) {
        self.set_previous_from_current();
        self.active = ActiveState::Grabbed { cursor_offset };
        self.state_time_ms = 0;
    }

    pub fn release(&mut self, velocity: (f32, f32)) {
        let (vx, vy) = velocity;
        let (vx, vy) = if vx.abs() > 10.0 || vy.abs() > 10.0 { (vx, vy) } else { (0.0, 0.0) };
        self.active = ActiveState::Airborne { vx, vy };
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
        self.last_vars.state_time_ms = self.state_time_ms;
        self.last_vars.pet_x = *x as f32;
        self.last_vars.pet_y = *y as f32;
        self.last_vars.screen_w = screen_w as f32;
        self.last_vars.pet_w = pet_w as f32;
        self.last_vars.pet_h = pet_h as f32;

        // 4. Execute action physics
        let (vx, vy) = self.speed();
        self.last_vars.pet_vx = vx;
        self.last_vars.pet_vy = vy;
        self.last_vars.pet_v = (vx * vx + vy * vy).sqrt();
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
                            // Named state with action=fall: normally handled via ActiveState::Airborne
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

            ActiveState::Airborne { vx, vy } => {
                let mut vx = vx;
                let mut vy = vy;
                vy += GRAVITY * dt;
                let new_x = *x + (vx * dt) as i32;
                let new_y = *y + (vy * dt) as i32;

                // Horizontal boundary bounce (no-op when vx == 0).
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
                    let fallback = self.sm.default_fallback.clone();
                    self.transition_to(&fallback, "landed");
                } else {
                    *x = clamped_x;
                    *y = new_y;
                    self.active = ActiveState::Airborne { vx, vy };
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
        let sm_toml = include_str!("../../assets/default.petstate");
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
        SpriteSheet { image, frames, tags, sm_mappings: std::collections::HashMap::new(), chromakey: crate::sprite::sheet::ChromakeyConfig::default(), tight_bboxes: vec![] }
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
        assert!(matches!(&r.active, ActiveState::Airborne { .. }));
        assert_eq!(r.current_state_name(), "thrown");
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

    fn make_collide_sm() -> Arc<crate::sprite::sm_compiler::CompiledSM> {
        let toml = r#"
[meta]
name = "test"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"

[states.idle.interrupts.collide]
goto = "react"

[states.react]
action = "sit"
duration = "500ms"
transitions = [{ goto = "idle" }]
"#;
        let file: SmFile = toml::from_str(toml).unwrap();
        compile(&file).unwrap()
    }

    #[test]
    fn walk_facing_does_not_change_during_single_episode() {
        // Build a minimal SM that transitions directly from idle to walk.
        // walk_remaining_px defaults to 400 px at 80 px/s → ~5 s to complete.
        // We only tick for ~30 frames (≈500ms), so the walk never finishes.
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

        // Place the pet well away from both walls: x=500 on a 1920-wide screen.
        let mut x: i32 = 500;
        let mut y: i32 = 800;
        let screen_w = 1920;
        let pet_w = 32;
        let pet_h = 32;
        let floor_y = 800;

        // Tick once — idle transitions immediately to walk (no after= guard).
        r.tick(16, &mut x, &mut y, screen_w, pet_w, pet_h, floor_y, &sheet);

        // Confirm we are now in walk.
        assert!(
            matches!(&r.active, ActiveState::Named(n) if n == "walk"),
            "expected walk state after first tick, got: {:?}", r.active
        );

        // Record the facing direction that was chosen when entering walk.
        let initial_facing = r.current_facing();

        // Tick for ~30 more frames (≈500ms at 16ms/frame) — still well within
        // the 400 px walk distance at 80 px/s (≈5 s to complete).
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

    #[test]
    fn tick_populates_pet_position_and_dimensions() {
        let mut r = SMRunner::new(make_collide_sm(), 80.0);
        let sheet = mock_sheet();
        let mut x = 100i32;
        let mut y = 200i32;
        r.tick(16, &mut x, &mut y, 1920, 64, 64, 1000, &sheet);
        let v = r.last_condition_vars();
        assert_eq!(v.pet_x, 100.0);
        assert_eq!(v.pet_y, 200.0);
        assert_eq!(v.screen_w, 1920.0);
        assert_eq!(v.pet_w, 64.0);
        assert_eq!(v.pet_h, 64.0);
    }

    #[test]
    fn tick_populates_state_time_ms() {
        let mut r = SMRunner::new(make_collide_sm(), 80.0);
        let sheet = mock_sheet();
        let mut x = 0i32; let mut y = 0i32;
        r.tick(100, &mut x, &mut y, 1920, 64, 64, 1000, &sheet);
        r.tick(150, &mut x, &mut y, 1920, 64, 64, 1000, &sheet);
        // state_time_ms should be 250 (100 + 150)
        assert_eq!(r.last_condition_vars().state_time_ms, 250);
    }

    #[test]
    fn tick_populates_velocity_from_thrown_state() {
        let mut r = SMRunner::new(make_collide_sm(), 80.0);
        r.active = ActiveState::Airborne { vx: 120.0, vy: -80.0 };
        let sheet = mock_sheet();
        let mut x = 0i32; let mut y = 0i32;
        r.tick(16, &mut x, &mut y, 1920, 64, 64, 1000, &sheet);
        let v = r.last_condition_vars();
        assert_eq!(v.pet_vx, 120.0);
        assert_eq!(v.pet_vy, -80.0);
        let expected_v = (120.0f32 * 120.0 + 80.0f32 * 80.0).sqrt();
        assert!((v.pet_v - expected_v).abs() < 0.1);
    }

    #[test]
    fn update_env_vars_sets_all_fields() {
        let mut r = SMRunner::new(make_collide_sm(), 80.0);
        r.update_env_vars(42.5, 14, "MyEditor".to_string(), 1080.0, 3, 250.0, 1920.0, "taskbar".to_string());
        let v = r.last_condition_vars();
        assert_eq!(v.cursor_dist, 42.5);
        assert_eq!(v.hour, 14);
        assert_eq!(v.focused_app, "MyEditor");
        assert_eq!(v.screen_h, 1080.0);
        assert_eq!(v.pet_count, 3);
        assert_eq!(v.other_pet_dist, 250.0);
        assert_eq!(v.surface_w, 1920.0);
        assert_eq!(v.surface_label, "taskbar");
    }
}
