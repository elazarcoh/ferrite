// Stress and timing tests for pet tick and render performance.
// Uses real Win32 windows — no mocking needed for UpdateLayeredWindow/SetWindowPos/GetWindowRect.
// These are smoke tests that verify correctness (no panics, no errors) while catching catastrophic regressions.

use my_pet::{
    app::PetInstance,
    config::schema::PetConfig,
    sprite::{behavior::AnimTagMap, sheet::load_embedded},
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
        flip_walk_left: false,
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
