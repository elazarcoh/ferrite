use ferrite_core::sprite::{animation::AnimationState, sm_runner::SMRunner, sheet::SpriteSheet};

pub struct PetWebState {
    pub sheet: SpriteSheet,
    pub anim: AnimationState,
    pub runner: SMRunner,
    pub x: i32,
    pub y: i32,
    pub last_ts: f64,
}
