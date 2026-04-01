use ferrite_core::sprite::{animation::AnimationState, sm_runner::SMRunner, sheet::SpriteSheet};
use crate::pet::surfaces::SurfaceCache;

pub struct PetWebState {
    pub sheet: SpriteSheet,
    pub anim: AnimationState,
    pub runner: SMRunner,
    pub x: i32,
    pub y: i32,
    pub last_ts: f64,
    // drag
    pub is_dragging: bool,
    pub drag_offset: (i32, i32),
    pub vel_prev: Option<((i32, i32), f64)>,
    pub vel_cur: Option<((i32, i32), f64)>,
    // surfaces
    pub surfaces: SurfaceCache,
}
