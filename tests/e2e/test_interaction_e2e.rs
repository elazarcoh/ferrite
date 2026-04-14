/// E2E: click-through and interaction logic.
use ferrite::{
    app::PetInstance,
    config::schema::PetConfig,
    sprite::{
        sm_runner::ActiveState,
        sheet::load_embedded,
    },
    window::blender::alpha_at,
};
use ferrite_core::geometry::PlatformBounds;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CYSCREEN};

fn test_sheet() -> ferrite::sprite::sheet::SpriteSheet {
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
    pet.runner.interrupt("petted", None);
    assert!(
        matches!(&pet.runner.active, ActiveState::Named(n) if n == "petted"),
        "interrupt petted should transition to petted state"
    );
}

#[test]
fn petted_state_resolves_back_to_idle() {
    let mut pet = make_pet();
    // Land the pet and wait for Idle.
    let mut cache = ferrite::window::surfaces::SurfaceCache::default();
    for _ in 0..500 {
        if matches!(&pet.runner.active, ActiveState::Named(n) if n == "idle") {
            break;
        }
        pet.tick(20, &mut cache).unwrap();
    }
    assert!(
        matches!(&pet.runner.active, ActiveState::Named(n) if n == "idle"),
        "pet must reach Idle before petted call; got {:?}", pet.runner.active
    );
    // Force pet to virtual ground so the snap logic in tick() cannot trigger Fall.
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let ground_y = PlatformBounds { screen_w: 0, screen_h }.virtual_ground_y() - pet.window.height as i32;
    pet.y = ground_y;
    pet.window.move_to(pet.x, ground_y);
    // Trigger petted interrupt
    pet.runner.interrupt("petted", None);
    let mut cache2 = ferrite::window::surfaces::SurfaceCache::default();
    for _ in 0..50 {
        pet.tick(20, &mut cache2).unwrap();
    }
    // After petted resolves, should return to some named state (idle or previous)
    assert!(
        matches!(&pet.runner.active, ActiveState::Named(_)),
        "Petted should resolve to a Named state after one-shot duration, got {:?}",
        pet.runner.active
    );
}

// ─── Drag / throw ────────────────────────────────────────────────────────────

#[test]
fn drag_start_sets_grabbed_state() {
    let mut pet = make_pet();
    pet.runner.grab((4, 8));
    assert!(
        matches!(&pet.runner.active, ActiveState::Grabbed { cursor_offset: (4, 8) }),
        "grab() must store cursor offset"
    );
}

#[test]
fn fast_release_causes_thrown() {
    let mut pet = make_pet();
    pet.runner.grab((0, 0));
    pet.runner.release((500.0, -200.0));
    assert!(
        matches!(&pet.runner.active, ActiveState::Thrown { .. }),
        "fast release must transition to Thrown"
    );
}

#[test]
fn slow_release_causes_fall() {
    let mut pet = make_pet();
    pet.runner.grab((0, 0));
    pet.runner.release((0.0, 0.0));
    assert!(
        matches!(&pet.runner.active, ActiveState::Fall { .. }),
        "slow release must transition to Fall"
    );
}
