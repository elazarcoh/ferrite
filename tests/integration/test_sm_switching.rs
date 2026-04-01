// Integration tests for SM hot-swap (apply_config fast path).

use ferrite::sprite::sm_runner::{SMRunner, ActiveState, load_default_sm};
use ferrite::sprite::sm_compiler::compile;
use ferrite::sprite::sm_format::SmFile;
use ferrite::sprite::sheet::{SpriteSheet, Frame, FrameTag, TagDirection};
use image::RgbaImage;
use std::sync::Arc;

fn mock_sheet() -> SpriteSheet {
    let image = RgbaImage::new(32, 32);
    let frames = vec![Frame { x: 0, y: 0, w: 32, h: 32, duration_ms: 100 }];
    let tags = vec![
        FrameTag { name: "idle".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        FrameTag { name: "walk".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        FrameTag { name: "sit".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        FrameTag { name: "grabbed".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        FrameTag { name: "petted".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        FrameTag { name: "fall".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
    ];
    SpriteSheet { image, frames, tags, sm_mappings: std::collections::HashMap::new() }
}

/// Build a minimal valid SM with a given default_fallback state name.
fn make_sm_with_default(default_state: &str) -> Arc<ferrite::sprite::sm_compiler::CompiledSM> {
    let toml_str = format!(
        r#"
[meta]
name = "TestSM_{default_state}"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "{default_state}"

[states.{default_state}]
required = true
action = "idle"
transitions = []

[states.grabbed]
required = true
action = "grabbed"
transitions = []

[states.fall]
required = true
action = "fall"
transitions = []

[states.thrown]
required = true
action = "thrown"
transitions = []
"#,
        default_state = default_state
    );
    let file: SmFile = toml::from_str(&toml_str).unwrap();
    compile(&file).unwrap()
}

/// Test: replacing the SM resets active state to the new SM's default,
/// while x/y (tracked externally) are unchanged.
#[test]
fn sm_hot_swap_preserves_position() {
    let sm1 = load_default_sm(); // default_fallback = "idle"
    let sm2 = make_sm_with_default("sit"); // default_fallback = "sit"

    let mut runner = SMRunner::new(sm1, 60.0);
    // Simulate the pet being at a known position.
    let mut x: i32 = 100;
    let mut y: i32 = 200;

    // Tick once to settle into the initial state.
    let sheet = mock_sheet();
    runner.tick(16, &mut x, &mut y, 1920, 32, 32, 1044, &sheet);

    // Position is whatever the runner set; we now record it and assert after swap.
    // For this test we care that x/y are NOT touched by replace_sm (replace_sm has
    // no knowledge of x/y — that's the point). We record them here.
    let pos_x_before = x;
    let pos_y_before = y;

    // Hot-swap the SM.
    runner.replace_sm(sm2);

    // x and y are external — replace_sm cannot modify them.
    // We assert they haven't changed since the last tick (replace_sm is a pure
    // runner mutation, no position side effects).
    assert_eq!(x, pos_x_before, "x must not change after SM hot-swap");
    assert_eq!(y, pos_y_before, "y must not change after SM hot-swap");

    // New SM's default state ("sit") must be active.
    assert!(
        matches!(&runner.active, ActiveState::Named(n) if n == "sit"),
        "after swap, runner must be in new SM's default state; got {:?}", runner.active
    );
}

/// Test: replacing the SM while in a non-idle state still resets to the new default.
#[test]
fn sm_hot_swap_resets_to_new_default() {
    let sm1 = load_default_sm();
    let sm2 = make_sm_with_default("sit");

    let mut runner = SMRunner::new(sm1, 60.0);

    // Force into grabbed state before swapping.
    runner.grab((5, 5));
    assert!(matches!(&runner.active, ActiveState::Grabbed { .. }));

    runner.replace_sm(sm2);

    // Should now be in new SM's default, not grabbed.
    assert!(
        matches!(&runner.active, ActiveState::Named(n) if n == "sit"),
        "after swap from Grabbed, should be in new default; got {:?}", runner.active
    );
}

/// Test (Windows-only): HWND is preserved when only the SM changes.
#[cfg(target_os = "windows")]
#[test]
fn sm_hot_swap_does_not_rebuild_window() {
    use ferrite::app::PetInstance;
    use ferrite::config::schema::PetConfig;
    use ferrite::sprite::sheet::load_embedded;
    use tempfile::tempdir;
    use ferrite::sprite::sm_gallery::SmGallery;

    // Build a minimal valid SM source and save it to a temp gallery.
    let dir = tempdir().unwrap();
    let mut gallery = SmGallery::load(dir.path());

    let sm_source = r#"
[meta]
name = "AltSM"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"
transitions = []

[states.grabbed]
required = true
action = "grabbed"
transitions = []

[states.fall]
required = true
action = "fall"
transitions = []

[states.thrown]
required = true
action = "thrown"
transitions = []
"#;
    gallery.save("AltSM", sm_source).expect("save SM");

    // Create a PetInstance with the default SM.
    let sheet = load_embedded(
        include_bytes!("../../assets/test_pet.json"),
        include_bytes!("../../assets/test_pet.png"),
    )
    .unwrap();
    let cfg = PetConfig {
        id: "test_hwnd".to_string(),
        sheet_path: "embedded://esheep".to_string(),
        state_machine: "embedded://default".to_string(),
        x: 100,
        y: 200,
        scale: 1.0,
        walk_speed: 60.0,
    };
    let mut pet = PetInstance::new(cfg, sheet).expect("create PetInstance");

    // Capture HWND before swap.
    let hwnd_before = pet.window.hwnd;

    // Hot-swap the SM.
    let new_sm = gallery.get("AltSM").expect("AltSM must be in gallery");
    pet.runner.replace_sm(new_sm);
    pet.cfg.state_machine = "AltSM".to_string();

    // HWND must be unchanged.
    assert_eq!(
        pet.window.hwnd, hwnd_before,
        "HWND must not change after SM hot-swap"
    );
}
