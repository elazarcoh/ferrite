/// E2E: animation advances and renders over time.
use my_pet::{
    app::PetInstance,
    config::schema::PetConfig,
    sprite::{behavior::{BehaviorAi, BehaviorState, Facing}, sheet::load_embedded},
};

fn test_sheet() -> my_pet::sprite::sheet::SpriteSheet {
    load_embedded(
        include_bytes!("../../assets/test_pet.json"),
        include_bytes!("../../assets/test_pet.png"),
    )
    .unwrap()
}

fn make_pet() -> PetInstance {
    PetInstance::new(PetConfig::default(), test_sheet()).unwrap()
}

// ─── Timer-independent animation advancement ──────────────────────────────────

#[test]
fn animation_frame_advances_after_one_frame_duration() {
    use my_pet::sprite::animation::AnimationState;
    let mut pet = make_pet();
    // Pet starts in Fall state; force into Idle + idle animation for this test.
    pet.ai.state = BehaviorState::Idle;
    pet.anim = AnimationState::new("idle");
    let start = pet.anim.absolute_frame(&pet.sheet);
    // Single tick longer than one idle frame duration (200 ms)
    pet.tick(210).unwrap();
    let end = pet.anim.absolute_frame(&pet.sheet);
    assert_ne!(start, end, "frame must advance after 210ms (idle frame dur = 200ms)");
}

#[test]
fn animation_cycles_forward_over_multiple_ticks() {
    let mut pet = make_pet();
    // idle is pingpong with 200 ms frames: 0 → 1 → 0 in 400 ms total.
    // Accumulate 400ms via small ticks.
    for _ in 0..40 {
        pet.tick(10).unwrap();
    }
    // After one full pingpong cycle (400 ms) the animation is back at frame 0.
    assert_eq!(pet.anim.frame_index, 0);
}

#[test]
fn initial_frame_is_zero() {
    let pet = make_pet();
    assert_eq!(pet.anim.frame_index, 0, "animation must start at frame 0");
}

#[test]
fn initial_frame_buffer_is_populated() {
    let pet = make_pet();
    // render_frame is called during new() — frame buffer must be non-empty.
    assert!(
        !pet.window_frame_buf_is_empty(),
        "window frame buffer must be populated immediately after construction"
    );
}

// ─── Position change when walking ────────────────────────────────────────────

#[test]
fn position_advances_when_walking_right() {
    let cfg = PetConfig { walk_speed: 200.0, ..PetConfig::default() };
    let mut pet = PetInstance::new(cfg, test_sheet()).unwrap();
    pet.ai.state = BehaviorState::Walk {
        facing: Facing::Right,
        remaining_px: 400.0,
    };
    let x0 = pet.x;
    pet.tick(500).unwrap(); // 0.5 s × 200 px/s = 100 px
    assert!(pet.x > x0, "pet should move right: x={} → {}", x0, pet.x);
}

#[test]
fn position_retreats_when_walking_left() {
    let cfg = PetConfig { walk_speed: 200.0, ..PetConfig::default() };
    let mut pet = PetInstance::new(cfg, test_sheet()).unwrap();
    pet.x = 500;
    pet.ai.state = BehaviorState::Walk {
        facing: Facing::Left,
        remaining_px: 400.0,
    };
    pet.tick(500).unwrap();
    assert!(pet.x < 500, "pet should move left");
}

// ─── Throw physics end-to-end ────────────────────────────────────────────────

#[test]
fn thrown_pet_lands_and_returns_to_idle() {
    let mut pet = make_pet();
    // Land the pet first so the real window y matches a grounded position.
    for _ in 0..300 {
        if matches!(pet.ai.state, BehaviorState::Idle | BehaviorState::Walk { .. } | BehaviorState::Sit) {
            break;
        }
        pet.tick(20).unwrap();
    }
    // Now throw it with a high downward velocity; it should hit the floor quickly.
    pet.ai.state = BehaviorState::Thrown { vx: 50.0, vy: 1000.0 };
    for _ in 0..100 {
        if matches!(pet.ai.state, BehaviorState::Idle) { break; }
        pet.tick(20).unwrap();
    }
    assert!(
        matches!(pet.ai.state, BehaviorState::Idle),
        "pet should be Idle after landing, got {:?}",
        pet.ai.state
    );
}
