use my_pet::sprite::behavior::{AnimTagMap, BehaviorAi, BehaviorState};

fn tick(ai: &mut BehaviorAi, ms: u32) -> BehaviorState {
    // floor_y = 1044 (1080 screen - 4 offset - 32 pet_h)
    ai.tick(ms, &mut 100, &mut 100, 1920, 32, 32, 60.0, 1044, &AnimTagMap::default());
    ai.state.clone()
}

#[test]
fn idle_to_walk() {
    let mut ai = BehaviorAi::new();
    // Advance past the maximum idle→walk threshold (12 s).
    tick(&mut ai, 13_000);
    assert!(!matches!(ai.state, BehaviorState::Idle));
}

#[test]
fn idle_to_sleep_at_30s() {
    let mut ai = BehaviorAi::new();
    tick(&mut ai, 30_001);
    assert!(matches!(ai.state, BehaviorState::Sleep));
}

#[test]
fn walk_to_idle_when_distance_exhausted() {
    let mut ai = BehaviorAi::new();
    ai.state = BehaviorState::Walk {
        facing: my_pet::sprite::behavior::Facing::Right,
        remaining_px: 10.0,
    };
    // 60 px/s * 1 s = 60 px > 10 px remaining
    tick(&mut ai, 1_000);
    assert!(matches!(ai.state, BehaviorState::Idle));
}

#[test]
fn grabbed_then_thrown() {
    let mut ai = BehaviorAi::new();
    ai.grab((5, 5));
    assert!(matches!(ai.state, BehaviorState::Grabbed { .. }));
    ai.release((200.0, -50.0));
    assert!(matches!(ai.state, BehaviorState::Thrown { .. }));
}

#[test]
fn grabbed_slow_release_falls() {
    let mut ai = BehaviorAi::new();
    ai.grab((0, 0));
    ai.release((0.0, 0.0));
    assert!(matches!(ai.state, BehaviorState::Fall { .. }));
}

#[test]
fn thrown_hits_ground_returns_idle() {
    let mut ai = BehaviorAi::new();
    ai.state = BehaviorState::Thrown { vx: 0.0, vy: 1000.0 };
    let mut y = 900i32;
    // vy=1000 + gravity*0.2=196 → new_y=1139 > ground_y=1044
    ai.tick(200, &mut 100, &mut y, 1920, 32, 32, 60.0, 1044, &AnimTagMap::default());
    assert!(matches!(ai.state, BehaviorState::Idle));
}

#[test]
fn petted_one_shot_returns_to_sit() {
    let mut ai = BehaviorAi::new();
    ai.state = BehaviorState::Sit;
    ai.pet();
    assert!(matches!(ai.state, BehaviorState::Petted { .. }));
    tick(&mut ai, 700);
    assert!(matches!(ai.state, BehaviorState::Sit));
}

#[test]
fn react_one_shot_returns_to_idle() {
    let mut ai = BehaviorAi::new();
    ai.react();
    tick(&mut ai, 700);
    assert!(matches!(ai.state, BehaviorState::Idle));
}

#[test]
fn wake_from_sleep() {
    let mut ai = BehaviorAi::new();
    ai.state = BehaviorState::Sleep;
    ai.wake();
    assert!(matches!(ai.state, BehaviorState::Wake));
    tick(&mut ai, 1_000);
    assert!(matches!(ai.state, BehaviorState::Idle));
}
