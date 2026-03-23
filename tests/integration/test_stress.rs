// Stress and timing tests for pet tick and render performance.
// Uses real Win32 windows — no mocking needed for UpdateLayeredWindow/SetWindowPos/GetWindowRect.
// These are smoke tests that verify correctness (no panics, no errors) while catching catastrophic regressions.

use my_pet::{
    app::PetInstance,
    config::schema::PetConfig,
    sprite::{
        sm_runner::{ActiveState, Facing},
        sheet::load_embedded,
    },
    window::{pet_window::PetWindow, surfaces::SurfaceCache},
};
use std::time::Instant;

fn make_pet() -> PetInstance {
    let sheet = load_embedded(
        include_bytes!("../../assets/test_pet.json"),
        include_bytes!("../../assets/test_pet.png"),
    )
    .unwrap();
    let cfg = PetConfig {
        id: "stress_pet".into(),
        sheet_path: "embedded://test_pet".into(),
        state_machine: "embedded://default".into(),
        scale: 2,
        x: 100,
        y: 100,
        walk_speed: 100.0,
    };
    PetInstance::new(cfg, sheet).expect("create PetInstance")
}

/// Build a sheet whose "walk" tag has flip_h=true, "idle" has flip_h=false.
fn make_flip_sheet() -> my_pet::sprite::sheet::SpriteSheet {
    let json = r#"{
        "frames": [
            {"frame":{"x":0,"y":0,"w":32,"h":32},"duration":100},
            {"frame":{"x":32,"y":0,"w":32,"h":32},"duration":100},
            {"frame":{"x":64,"y":0,"w":32,"h":32},"duration":100},
            {"frame":{"x":96,"y":0,"w":32,"h":32},"duration":100}
        ],
        "meta": {
            "frameTags": [
                {"name":"idle","from":0,"to":1,"direction":"forward"},
                {"name":"walk","from":2,"to":3,"direction":"forward","flipH":true}
            ]
        }
    }"#;
    let png = include_bytes!("../../assets/test_pet.png");
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .unwrap()
        .into_rgba8();
    my_pet::sprite::sheet::SpriteSheet::from_json_and_image(json.as_bytes(), image).unwrap()
}

fn make_pet_with_flip_sheet() -> PetInstance {
    let sheet = make_flip_sheet();
    let cfg = PetConfig {
        id: "flip_pet".into(),
        sheet_path: "embedded://test".into(),
        state_machine: "embedded://default".into(),
        scale: 1,
        x: 100,
        y: 100,
        walk_speed: 100.0,
    };
    PetInstance::new(cfg, sheet).expect("create flip PetInstance")
}

#[test]
fn compute_flip_false_when_idle() {
    let pet = make_pet_with_flip_sheet();
    // Default state is Fall (spawning), which is not Walk → no flip.
    // The compute_flip now uses runner.current_facing() and the current tag name.
    // When in Fall state, current_state_name()="fall" which isn't in tags, so flip_h=false.
    // Facing is Right by default, so flip_h=false → no flip.
    assert!(!pet.compute_flip(), "flip must be false when not walking");
}

#[test]
fn compute_flip_true_when_walking_right_with_flip_h_tag() {
    // flip_h=true means "sprite faces LEFT" — mirror when going RIGHT
    let mut pet = make_pet_with_flip_sheet();
    // Set runner to walk state facing right
    pet.runner.active = ActiveState::Named("walk".to_string());
    pet.runner.facing = Facing::Right;
    pet.anim.set_tag("walk");
    assert!(pet.compute_flip(), "flip must be true when facing right with flip_h tag");
}

#[test]
fn compute_flip_false_when_walking_left_with_flip_h_tag() {
    // Sprite faces LEFT naturally — no flip needed when going left
    let mut pet = make_pet_with_flip_sheet();
    pet.runner.active = ActiveState::Named("walk".to_string());
    pet.runner.facing = Facing::Left;
    pet.anim.set_tag("walk");
    assert!(!pet.compute_flip(), "flip must be false when facing left with flip_h tag (sprite already faces left)");
}

#[test]
fn compute_flip_false_when_walking_right_without_flip_h_tag() {
    let mut pet = make_pet_with_flip_sheet();
    pet.runner.active = ActiveState::Named("idle".to_string());
    pet.runner.facing = Facing::Right;
    // Idle tag has flip_h=false (sprite faces right) — no flip when going right
    pet.anim.set_tag("idle");
    assert!(!pet.compute_flip(), "flip must be false when tag has flip_h=false and going right");
}

#[test]
fn compute_flip_true_when_walking_left_without_flip_h_tag() {
    let mut pet = make_pet_with_flip_sheet();
    pet.runner.active = ActiveState::Named("idle".to_string());
    pet.runner.facing = Facing::Left;
    // Idle tag has flip_h=false (sprite faces right) — must flip when going left
    pet.anim.set_tag("idle");
    assert!(pet.compute_flip(), "flip must be true when tag has flip_h=false and going left (arrows case)");
}

#[test]
fn tick_1000_frames_10_pets() {
    let mut pets: Vec<PetInstance> = (0..10).map(|_| make_pet()).collect();
    let mut cache = SurfaceCache::default();

    let start = Instant::now();
    for _ in 0..1000 {
        for pet in &mut pets {
            pet.tick(16, &mut cache).expect("tick must not error");
        }
    }
    let elapsed = start.elapsed();

    let budget_ms = 30000;
    assert!(
        elapsed.as_millis() < budget_ms,
        "10 pets × 1000 ticks (10,000 SetWindowPos calls) took {}ms — must be under {}ms",
        elapsed.as_millis(),
        budget_ms,
    );
    println!("tick_1000_frames_10_pets: {}ms", elapsed.as_millis());
}

#[test]
fn render_frame_100_times() {
    let mut win = PetWindow::create(0, 0, 64, 64).expect("create window");
    let sheet = load_embedded(
        include_bytes!("../../assets/test_pet.json"),
        include_bytes!("../../assets/test_pet.png"),
    )
    .unwrap();
    let f = &sheet.frames[0];

    let start = Instant::now();
    for _ in 0..100 {
        win.render_frame(&sheet.image, f.x, f.y, f.w, f.h, 2, false)
            .expect("render must not error");
    }
    let elapsed = start.elapsed();

    let budget_ms = 1000;
    assert!(
        elapsed.as_millis() < budget_ms,
        "100 render_frame calls took {}ms — must be under {}ms",
        elapsed.as_millis(),
        budget_ms,
    );
    println!("render_frame_100_times: {}ms", elapsed.as_millis());
}
