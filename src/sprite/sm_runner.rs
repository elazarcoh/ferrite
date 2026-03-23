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

#[derive(Debug, Clone)]
pub struct TransitionLogEntry {
    pub from_state: String,
    pub to_state: String,
    pub reason: String,
}

pub struct SMRunner {
    pub sm: Arc<CompiledSM>,
    active: ActiveState,
    previous_named: Option<String>,
    state_time_ms: u32,
    step_index: usize,
    walk_remaining_px: f32,
    facing: Facing,
    walk_speed: f32,
    rng: u64,
    next_transition_ms: u32,
    last_vars: ConditionVars,
    transition_log: Vec<TransitionLogEntry>,
    // Debug tools
    pub force_state: Option<String>,
    pub release_force: bool,
    pub step_mode: bool,
    pub step_advance: bool,
}

impl SMRunner {
    pub fn new(sm: Arc<CompiledSM>, walk_speed: f32) -> Self {
        let mut runner = Self {
            sm: sm.clone(),
            active: ActiveState::Named("idle".to_string()),
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
            force_state: None,
            release_force: false,
            step_mode: false,
            step_advance: false,
        };
        runner.enter_state("idle");
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
        if event == "grabbed" {
            let offset = cursor_offset.unwrap_or((0, 0));
            self.grab(offset);
        }
        // Other interrupt handling will be added in tick()
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

    fn enter_state(&mut self, name: &str) {
        self.state_time_ms = 0;
        self.step_index = 0;
        self.next_transition_ms = 0;
        self.active = ActiveState::Named(name.to_string());
    }

    fn set_previous_from_current(&mut self) {
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

    /// Placeholder tick — will be implemented in Task 8.
    /// Returns the tag name to use for the current animation frame.
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
        // TODO(Task-8): Implement atomic state tick
        self.state_time_ms = self.state_time_ms.saturating_add(delta_ms);
        self.current_state_name()
    }
}
