use serde::{Deserialize, Serialize};

// ─── Physics constants ────────────────────────────────────────────────────────

pub const GRAVITY: f32 = 980.0; // px/s²
pub const GROUND_Y_OFFSET: i32 = -4; // px above bottom of screen
pub const THROW_VX_SCALE: f32 = 2.0;
pub const THROW_VY_SCALE: f32 = 1.5;

// ─── Tag map ─────────────────────────────────────────────────────────────────

/// Maps behavior states to spritesheet tag names.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnimTagMap {
    pub idle: String,
    pub walk: String,
    pub run: Option<String>,
    pub sit: Option<String>,
    pub sleep: Option<String>,
    pub wake: Option<String>,
    pub grabbed: Option<String>,
    pub petted: Option<String>,
    pub react: Option<String>,
    pub fall: Option<String>,
    pub thrown: Option<String>,
}

impl Default for AnimTagMap {
    fn default() -> Self {
        AnimTagMap {
            idle: "idle".into(),
            walk: "walk".into(),
            run: None,
            sit: Some("sit".into()),
            sleep: Some("sleep".into()),
            wake: None,
            grabbed: Some("grabbed".into()),
            // Reuse grabbed (orange) for petted — visually distinct from idle.
            petted: Some("grabbed".into()),
            // Reuse fall (red/speedlines) for react — startled look.
            react: Some("fall".into()),
            fall: Some("fall".into()),
            thrown: Some("fall".into()),
        }
    }
}

impl AnimTagMap {
    pub fn tag_for(&self, state: &BehaviorState) -> &str {
        match state {
            BehaviorState::Idle => &self.idle,
            BehaviorState::Walk { .. } => &self.walk,
            BehaviorState::Run { .. } => self.run.as_deref().unwrap_or(&self.walk),
            BehaviorState::Sit => self.sit.as_deref().unwrap_or(&self.idle),
            BehaviorState::Sleep => self.sleep.as_deref().unwrap_or(&self.idle),
            BehaviorState::Wake => self.wake.as_deref().unwrap_or(&self.idle),
            BehaviorState::Fall { .. } => self.fall.as_deref().unwrap_or(&self.idle),
            BehaviorState::Thrown { .. } => self.thrown.as_deref().unwrap_or(&self.idle),
            BehaviorState::Grabbed { .. } => self.grabbed.as_deref().unwrap_or(&self.idle),
            BehaviorState::Petted { .. } => self.petted.as_deref().unwrap_or(&self.idle),
            BehaviorState::React { .. } => self.react.as_deref().unwrap_or(&self.idle),
        }
    }
}

// ─── State machine ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Facing {
    Left,
    Right,
}

#[derive(Debug, Clone)]
pub enum BehaviorState {
    Idle,
    Walk { facing: Facing, remaining_px: f32 },
    Run { facing: Facing, remaining_px: f32 },
    Sit,
    Sleep,
    Wake,
    Fall { vy: f32 },
    Thrown { vx: f32, vy: f32 },
    Grabbed { cursor_offset: (i32, i32) },
    Petted { previous: Box<BehaviorState> },
    React { previous: Box<BehaviorState> },
}

impl PartialEq for BehaviorState {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

// ─── AI ───────────────────────────────────────────────────────────────────────

pub struct BehaviorAi {
    pub state: BehaviorState,
    idle_timer_ms: u32,
    /// Pre-computed ms until next idle action (0 = needs computing).
    idle_next_ms: u32,
    sit_timer_ms: u32,
    /// Pre-computed ms until sit ends (0 = needs computing).
    sit_next_ms: u32,
    one_shot_done: bool,
    /// Random seed — simple LCG.
    rng: u64,
}

impl BehaviorAi {
    pub fn new() -> Self {
        BehaviorAi {
            state: BehaviorState::Idle,
            idle_timer_ms: 0,
            idle_next_ms: 0,
            sit_timer_ms: 0,
            sit_next_ms: 0,
            one_shot_done: false,
            rng: 12345,
        }
    }

    fn rand_f32(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.rng >> 33) as f32 / u32::MAX as f32
    }

    fn rand_range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.rand_f32() * (hi - lo)
    }

    /// Advance the behavior state machine.
    ///
    /// - `delta_ms`: time since last tick
    /// - `x`, `y`: current pet window top-left position (pixels)
    /// - `screen_w`, `screen_h`: screen dimensions
    /// - `walk_speed`: pixels per second
    ///
    /// Returns the animation tag the pet should be playing.
    pub fn tick<'a>(
        &mut self,
        delta_ms: u32,
        x: &mut i32,
        y: &mut i32,
        screen_w: i32,
        pet_w: i32,
        pet_h: i32,
        walk_speed: f32,
        floor_y: i32,
        tag_map: &'a AnimTagMap,
    ) -> &'a str {
        let dt_s = delta_ms as f32 / 1000.0;

        let state_before = std::mem::discriminant(&self.state);
        match &self.state.clone() {
            BehaviorState::Idle => {
                self.idle_timer_ms += delta_ms;

                // Hard sleep cutoff after ~15 s of continuous idling.
                if self.idle_timer_ms >= 15_000 {
                    self.idle_timer_ms = 0;
                    self.idle_next_ms = 0;
                    self.state = BehaviorState::Sleep;
                } else {
                    // Compute next-action threshold once per Idle entry (1–3 s).
                    if self.idle_next_ms == 0 {
                        self.idle_next_ms = self.rand_range(1_000.0, 3_000.0) as u32;
                    }
                    if self.idle_timer_ms >= self.idle_next_ms {
                        self.idle_timer_ms = 0;
                        self.idle_next_ms = 0;
                        // Action weights: 45 % walk, 20 % sit, 20 % hop, 15 % sleep.
                        let r = self.rand_f32();
                        if r < 0.45 {
                            let facing = if self.rand_f32() > 0.5 { Facing::Right } else { Facing::Left };
                            let dist = self.rand_range(200.0, 800.0);
                            self.state = BehaviorState::Walk { facing, remaining_px: dist };
                        } else if r < 0.65 {
                            self.sit_timer_ms = 0;
                            self.sit_next_ms = 0;
                            self.state = BehaviorState::Sit;
                        } else if r < 0.85 {
                            // Hop straight up.
                            self.state = BehaviorState::Thrown { vx: 0.0, vy: -300.0 };
                        } else {
                            self.state = BehaviorState::Sleep;
                        }
                    }
                }
            }

            BehaviorState::Walk { facing, remaining_px } => {
                let facing = facing.clone();
                let mut rem = *remaining_px;
                let dx = walk_speed * dt_s;
                rem -= dx;

                let new_x = match facing {
                    Facing::Right => *x + dx as i32,
                    Facing::Left => *x - dx as i32,
                };

                if new_x <= 0 {
                    *x = 0;
                    self.state = BehaviorState::Walk {
                        facing: Facing::Right,
                        remaining_px: rem.abs(),
                    };
                } else if new_x + pet_w >= screen_w {
                    *x = screen_w - pet_w;
                    self.state =
                        BehaviorState::Walk { facing: Facing::Left, remaining_px: rem.abs() };
                } else if rem <= 0.0 {
                    *x = new_x;
                    self.state = BehaviorState::Idle;
                    self.idle_timer_ms = 0;
                } else {
                    *x = new_x;
                    self.state = BehaviorState::Walk { facing, remaining_px: rem };
                }
            }

            BehaviorState::Run { facing, remaining_px } => {
                // Run behaves like Walk but uses walk_speed * 2.
                let facing = facing.clone();
                let mut rem = *remaining_px;
                let dx = walk_speed * 2.0 * dt_s;
                rem -= dx;
                let new_x = match facing {
                    Facing::Right => *x + dx as i32,
                    Facing::Left => *x - dx as i32,
                };
                if new_x <= 0 {
                    *x = 0;
                    self.state = BehaviorState::Run { facing: Facing::Right, remaining_px: rem.abs() };
                } else if new_x + pet_w >= screen_w {
                    *x = screen_w - pet_w;
                    self.state = BehaviorState::Run { facing: Facing::Left, remaining_px: rem.abs() };
                } else if rem <= 0.0 {
                    *x = new_x;
                    self.state = BehaviorState::Idle;
                    self.idle_timer_ms = 0;
                } else {
                    *x = new_x;
                    self.state = BehaviorState::Run { facing, remaining_px: rem };
                }
            }

            BehaviorState::Sit => {
                self.sit_timer_ms += delta_ms;
                if self.sit_next_ms == 0 {
                    self.sit_next_ms = self.rand_range(1_500.0, 4_000.0) as u32;
                }
                if self.sit_timer_ms >= self.sit_next_ms {
                    self.sit_timer_ms = 0;
                    self.sit_next_ms = 0;
                    self.state = BehaviorState::Idle;
                    self.idle_timer_ms = 0;
                    self.idle_next_ms = 0;
                }
            }

            BehaviorState::Sleep => {
                // Stays asleep until user clicks (handled externally via wake()).
            }

            BehaviorState::Wake => {
                // One-shot: handled by animation completion signal.
                // For simplicity, resolve after a fixed time.
                self.idle_timer_ms += delta_ms;
                if self.idle_timer_ms >= 800 {
                    self.idle_timer_ms = 0;
                    self.state = BehaviorState::Idle;
                }
            }

            BehaviorState::Fall { vy } => {
                let mut vy = *vy;
                vy += GRAVITY * dt_s;
                let new_y = *y + (vy * dt_s) as i32;
                if new_y >= floor_y {
                    *y = floor_y;
                    self.state = BehaviorState::Idle;
                    self.idle_timer_ms = 0;
                } else {
                    *y = new_y;
                    self.state = BehaviorState::Fall { vy };
                }
            }

            BehaviorState::Thrown { vx, vy } => {
                let mut vx = *vx;
                let mut vy = *vy;
                vy += GRAVITY * dt_s;
                let new_x = *x + (vx * dt_s) as i32;
                let new_y = *y + (vy * dt_s) as i32;

                // Horizontal bounce.
                let (clamped_x, new_vx) = if new_x <= 0 {
                    (0, vx.abs())
                } else if new_x + pet_w >= screen_w {
                    (screen_w - pet_w, -vx.abs())
                } else {
                    (new_x, vx)
                };

                if new_y >= floor_y {
                    *x = clamped_x;
                    *y = floor_y;
                    self.state = BehaviorState::Idle;
                    self.idle_timer_ms = 0;
                } else {
                    *x = clamped_x;
                    *y = new_y;
                    self.state = BehaviorState::Thrown { vx: new_vx, vy };
                }
            }

            BehaviorState::Grabbed { .. } => {
                // Position is driven externally (cursor). No AI action.
            }

            BehaviorState::Petted { previous } | BehaviorState::React { previous } => {
                // One-shot: resolved after fixed time, returns to previous state.
                let prev = (**previous).clone();
                self.idle_timer_ms += delta_ms;
                if self.idle_timer_ms >= 600 {
                    self.idle_timer_ms = 0;
                    self.state = prev;
                }
            }
        }

        if std::mem::discriminant(&self.state) != state_before {
            log::debug!("AI → {:?} (x={x}, y={y})", self.state);
        }

        tag_map.tag_for(&self.state)
    }

    // ─── External trigger methods ─────────────────────────────────────────────

    pub fn grab(&mut self, cursor_offset: (i32, i32)) {
        self.state = BehaviorState::Grabbed { cursor_offset };
    }

    pub fn release(&mut self, velocity: (f32, f32)) {
        const THROW_THRESHOLD: f32 = 50.0; // px/s
        let speed = (velocity.0 * velocity.0 + velocity.1 * velocity.1).sqrt();
        if speed >= THROW_THRESHOLD {
            self.state = BehaviorState::Thrown {
                vx: velocity.0 * THROW_VX_SCALE,
                vy: velocity.1 * THROW_VY_SCALE,
            };
        } else {
            self.state = BehaviorState::Fall { vy: 0.0 };
        }
    }

    pub fn pet(&mut self) {
        if !matches!(self.state, BehaviorState::Grabbed { .. }) {
            let prev = self.state.clone();
            self.state = BehaviorState::Petted { previous: Box::new(prev) };
            self.idle_timer_ms = 0;
        }
    }

    pub fn react(&mut self) {
        let prev = self.state.clone();
        self.state = BehaviorState::React { previous: Box::new(prev) };
        self.idle_timer_ms = 0;
    }

    pub fn wake(&mut self) {
        if matches!(self.state, BehaviorState::Sleep) {
            self.state = BehaviorState::Wake;
            self.idle_timer_ms = 0;
        }
    }

    /// Reset idle state (called when forced out of idle by external event).
    pub fn reset_idle(&mut self) {
        self.idle_timer_ms = 0;
        self.idle_next_ms = 0;
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_tick(ai: &mut BehaviorAi, ms: u32) -> BehaviorState {
        ai.tick(ms, &mut 100, &mut 100, 1920, 32, 32, 60.0, 1044, &AnimTagMap::default());
        ai.state.clone()
    }

    #[test]
    fn idle_to_walk_after_timer() {
        let mut ai = BehaviorAi::new();
        // Force a short idle threshold by advancing well past max idle time.
        dummy_tick(&mut ai, 13_000);
        // Should now be in Walk or Sit or Sleep.
        assert!(!matches!(ai.state, BehaviorState::Idle));
    }

    #[test]
    fn idle_to_sleep_after_15s() {
        let mut ai = BehaviorAi::new();
        dummy_tick(&mut ai, 15_001);
        assert!(matches!(ai.state, BehaviorState::Sleep));
    }

    #[test]
    fn grab_then_throw() {
        let mut ai = BehaviorAi::new();
        ai.grab((5, 5));
        assert!(matches!(ai.state, BehaviorState::Grabbed { .. }));
        ai.release((200.0, -100.0));
        assert!(matches!(ai.state, BehaviorState::Thrown { .. }));
    }

    #[test]
    fn grab_then_fall() {
        let mut ai = BehaviorAi::new();
        ai.grab((0, 0));
        ai.release((0.0, 0.0));
        assert!(matches!(ai.state, BehaviorState::Fall { .. }));
    }

    #[test]
    fn thrown_hits_ground() {
        let mut ai = BehaviorAi::new();
        ai.state = BehaviorState::Thrown { vx: 0.0, vy: 1000.0 };
        let mut y = 900i32;
        // vy=1000 + gravity*0.2=196 → new_y = 900 + 1196*0.2 = 1139 > floor_y(1044)
        ai.tick(200, &mut 100, &mut y, 1920, 32, 32, 60.0, 1044, &AnimTagMap::default());
        assert!(matches!(ai.state, BehaviorState::Idle));
    }

    #[test]
    fn petted_returns_to_previous() {
        let mut ai = BehaviorAi::new();
        ai.state = BehaviorState::Sit;
        ai.pet();
        assert!(matches!(ai.state, BehaviorState::Petted { .. }));
        dummy_tick(&mut ai, 700);
        assert!(matches!(ai.state, BehaviorState::Sit));
    }

    #[test]
    fn react_returns_to_previous() {
        let mut ai = BehaviorAi::new();
        ai.react();
        assert!(matches!(ai.state, BehaviorState::React { .. }));
        dummy_tick(&mut ai, 700);
        assert!(matches!(ai.state, BehaviorState::Idle));
    }

    #[test]
    fn wake_from_sleep() {
        let mut ai = BehaviorAi::new();
        ai.state = BehaviorState::Sleep;
        ai.wake();
        assert!(matches!(ai.state, BehaviorState::Wake));
    }
}
