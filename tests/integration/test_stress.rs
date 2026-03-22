// Stress and timing tests for pet tick and render performance.
// Uses real Win32 windows — no mocking needed for UpdateLayeredWindow/SetWindowPos/GetWindowRect.
// These are smoke tests that verify correctness (no panics, no errors) while catching catastrophic regressions.

use my_pet::{
    app::PetInstance,
    config::schema::PetConfig,
    sprite::{
        behavior::{AnimTagMap, BehaviorState, Facing},
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
        scale: 2,
        x: 100,
        y: 100,
        walk_speed: 100.0,
        tag_map: AnimTagMap {
            idle: "idle".into(),
            walk: "walk".into(),
            run: None,
            sit: None,
            sleep: None,
            wake: None,
            grabbed: None,
            petted: None,
            react: None,
            fall: None,
            thrown: None,
        },
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
        scale: 1,
        x: 100,
        y: 100,
        walk_speed: 100.0,
        tag_map: AnimTagMap {
            idle: "idle".into(),
            walk: "walk".into(),
            run: None, sit: None, sleep: None, wake: None,
            grabbed: None, petted: None, react: None, fall: None, thrown: None,
        },
    };
    PetInstance::new(cfg, sheet).expect("create flip PetInstance")
}

#[test]
fn compute_flip_false_when_idle() {
    let pet = make_pet_with_flip_sheet();
    // Default state is Fall (spawning), which is not Walk → no flip.
    assert!(!pet.compute_flip(), "flip must be false when not walking");
}

#[test]
fn compute_flip_false_when_walking_right() {
    let mut pet = make_pet_with_flip_sheet();
    pet.ai.state = BehaviorState::Walk { facing: Facing::Right, remaining_px: 500.0 };
    pet.anim.set_tag("walk");
    // tag has flip_h=true but facing is Right → no flip
    assert!(!pet.compute_flip(), "flip must be false when facing right");
}

#[test]
fn compute_flip_true_when_walking_left_with_flip_h_tag() {
    let mut pet = make_pet_with_flip_sheet();
    pet.ai.state = BehaviorState::Walk { facing: Facing::Left, remaining_px: 500.0 };
    pet.anim.set_tag("walk");
    assert!(pet.compute_flip(), "flip must be true when facing left with flip_h tag");
}

#[test]
fn compute_flip_false_when_walking_left_without_flip_h_tag() {
    let mut pet = make_pet_with_flip_sheet();
    pet.ai.state = BehaviorState::Walk { facing: Facing::Left, remaining_px: 500.0 };
    // Idle tag has flip_h=false
    pet.anim.set_tag("idle");
    assert!(!pet.compute_flip(), "flip must be false when tag has flip_h=false");
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
