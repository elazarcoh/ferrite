/// E2E: click-through and interaction logic.
use my_pet::{
    app::PetInstance,
    config::schema::PetConfig,
    sprite::{
        behavior::{BehaviorAi, BehaviorState},
        sheet::load_embedded,
    },
    window::blender::alpha_at,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CYSCREEN};

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

// ─── Per-pixel hit testing (logic layer, no WM_NCHITTEST) ────────────────────

#[test]
fn opaque_pixel_has_nonzero_alpha() {
    let pet = make_pet();
    // Frame 0 of test_pet.png is solid green (alpha=255). After initial render
    // the frame buffer must have alpha=255 at interior pixels.
    let buf = pet.window_frame_buf();
    let w = pet.window_width();
    // Centre pixel of the 64×64 rendered window (scale 2, so all pixels are
    // copies of the source — centre is inside the green solid square).
    let a = alpha_at(buf, w, w / 2, w / 2);
    assert_eq!(a, 255, "centre pixel of green square must be fully opaque");
}

// ─── Interaction: petted / react ────────────────────────────────────────────

#[test]
fn pet_click_triggers_petted_state() {
    let mut pet = make_pet();
    pet.ai.pet();
    assert!(
        matches!(pet.ai.state, BehaviorState::Petted { .. }),
        "pet() should transition to Petted"
    );
}

#[test]
fn petted_state_resolves_back_to_idle() {
    let mut pet = make_pet();
    // Land the pet and wait for Idle specifically — if we stop at Walk,
    // pet() stores Walk as previous state and the pet can fall off the screen
    // edge after Petted resolves, producing Fall instead of Idle.
    for _ in 0..500 {
        if matches!(pet.ai.state, BehaviorState::Idle) {
            break;
        }
        pet.tick(20).unwrap();
    }
    assert!(
        matches!(pet.ai.state, BehaviorState::Idle),
        "pet must reach Idle before pet() call; got {:?}", pet.ai.state
    );
    // Force pet to virtual ground so the snap logic in tick() cannot trigger
    // Fall if the pet happened to land on a user app window that then closes.
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let ground_y = screen_h - 4 - pet.window.height as i32;
    pet.y = ground_y;
    pet.window.move_to(pet.x, ground_y);
    pet.ai.state = BehaviorState::Idle;
    pet.ai.reset_idle();
    pet.ai.pet();
    for _ in 0..50 {
        pet.tick(20).unwrap();
    }
    assert!(
        matches!(pet.ai.state, BehaviorState::Idle),
        "Petted should resolve to Idle after one-shot duration, got {:?}",
        pet.ai.state
    );
}

// ─── Drag / throw ────────────────────────────────────────────────────────────

#[test]
fn drag_start_sets_grabbed_state() {
    let mut pet = make_pet();
    pet.ai.grab((4, 8));
    assert!(
        matches!(pet.ai.state, BehaviorState::Grabbed { cursor_offset: (4, 8) }),
        "grab() must store cursor offset"
    );
}

#[test]
fn fast_release_causes_thrown() {
    let mut pet = make_pet();
    pet.ai.grab((0, 0));
    pet.ai.release((500.0, -200.0));
    assert!(
        matches!(pet.ai.state, BehaviorState::Thrown { .. }),
        "fast release must transition to Thrown"
    );
}

#[test]
fn slow_release_causes_fall() {
    let mut pet = make_pet();
    pet.ai.grab((0, 0));
    pet.ai.release((0.0, 0.0));
    assert!(
        matches!(pet.ai.state, BehaviorState::Fall { .. }),
        "slow release must transition to Fall"
    );
}
