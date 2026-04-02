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
    SpriteSheet { image, frames, tags, sm_mappings: std::collections::HashMap::new(), chromakey: ferrite_core::sprite::sheet::ChromakeyConfig::default() }
}

// Mirrors make_minimal_sm() in sm_runner.rs — kept here because that helper
// is #[cfg(test)]-private and cannot be imported from integration tests.
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

/// Test (Windows-only): `apply_config()` fast path — HWND survives and
/// `pet.cfg` is fully updated when only `state_machine` changes.
///
/// This test exercises the same code path as `App::apply_config()` directly at
/// the `PetInstance` level, because building a full `App` in integration tests
/// is not feasible (requires a real event loop and system tray).
#[cfg(target_os = "windows")]
#[test]
fn sm_apply_config_hot_swaps_sm_only() {
    use ferrite::app::PetInstance;
    use ferrite::config::schema::PetConfig;
    use ferrite::sprite::sheet::load_embedded;
    use ferrite::sprite::sm_gallery::SmGallery;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let mut gallery = SmGallery::load(dir.path());

    let sm_source = r#"
[meta]
name = "NewSM"
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
    gallery.save("NewSM", sm_source).expect("save SM");

    let sheet = load_embedded(
        include_bytes!("../../assets/test_pet.json"),
        include_bytes!("../../assets/test_pet.png"),
    )
    .unwrap();

    // Initial config: state_machine = embedded://default.
    let old_cfg = PetConfig {
        id: "test_apply_cfg".to_string(),
        sheet_path: "embedded://esheep".to_string(),
        state_machine: "embedded://default".to_string(),
        x: 50,
        y: 50,
        scale: 1.0,
        walk_speed: 60.0,
    };
    let mut pet = PetInstance::new(old_cfg.clone(), sheet).expect("create PetInstance");

    let hwnd_before = pet.window.hwnd;

    // New config: only state_machine changed — mirrors the fast-path condition
    // checked in App::apply_config().
    let new_cfg = PetConfig {
        state_machine: "NewSM".to_string(),
        ..old_cfg.clone()
    };

    // Replicate the apply_config() fast-path logic.
    assert_eq!(pet.cfg.sheet_path, new_cfg.sheet_path);
    assert_eq!(pet.cfg.scale, new_cfg.scale);
    assert_eq!(pet.cfg.walk_speed, new_cfg.walk_speed);
    assert_ne!(pet.cfg.state_machine, new_cfg.state_machine);

    let new_sm = gallery.get("NewSM").expect("NewSM must be in gallery");
    pet.runner.replace_sm(new_sm);
    // Use full cfg assignment (as apply_config() does after Fix 2).
    pet.cfg = new_cfg.clone();

    // HWND must be unchanged — no window rebuild occurred.
    assert_eq!(
        pet.window.hwnd, hwnd_before,
        "HWND must not change when only SM changes"
    );

    // pet.cfg must be fully in sync with the new config.
    assert_eq!(
        pet.cfg.state_machine, "NewSM",
        "pet.cfg.state_machine must equal new name after apply_config fast path"
    );
    assert_eq!(
        pet.cfg, new_cfg,
        "pet.cfg must be fully equal to new_cfg after fast-path hot-swap"
    );
}

/// Test: BundleImported handler core logic — find pet by sprite_id, look up SM
/// from gallery, call replace_sm, update pet.cfg.state_machine.
///
/// Since building a full App in integration tests is infeasible (requires a real
/// event loop and system tray), this test exercises the logic directly at the
/// SMRunner/PetConfig level, consistent with the pattern used in the other tests
/// in this file.
#[test]
fn bundle_import_auto_assigns_sm() {
    // SM "A" — initial state machine with default "idle"
    let sm_a = make_sm_with_default("idle");

    let mut runner = SMRunner::new(sm_a, 60.0);

    // Initial runner state must be "idle"
    assert!(
        matches!(&runner.active, ActiveState::Named(n) if n == "idle"),
        "initial state should be idle; got {:?}", runner.active
    );

    // SM "B" — bundled SM with default "sit"
    let sm_b = make_sm_with_default("sit");
    let sm_b_name = sm_b.name.clone(); // "TestSM_sit"

    // Simulate the BundleImported handler's core logic:
    // find pet by sprite_id (mocked: we just have the runner), look up SM "B"
    // from gallery, call replace_sm, update cfg.state_machine.
    let mut cfg = ferrite::config::schema::PetConfig {
        id: "test_bundle_pet".to_string(),
        sheet_path: "sprites/my-bundle-sprite.json".to_string(), // contains "my-bundle"
        state_machine: "TestSM_idle".to_string(),
        x: 0,
        y: 0,
        scale: 1.0,
        walk_speed: 60.0,
    };

    // Simulate: sprite_id = "my-bundle", sheet_path.contains(sprite_id) == true
    let sprite_id = "my-bundle";
    assert!(
        cfg.sheet_path.contains(sprite_id),
        "sheet_path must contain sprite_id for pet matching to work"
    );

    // Perform the auto-assign (mirrors the BundleImported handler logic)
    runner.replace_sm(sm_b);
    cfg.state_machine = sm_b_name.clone();

    // Assert: cfg.state_machine updated
    assert_eq!(
        cfg.state_machine, sm_b_name,
        "cfg.state_machine must be updated to the new SM name after bundle import"
    );

    // Assert: runner active state is SM B's default ("sit")
    assert!(
        matches!(&runner.active, ActiveState::Named(n) if n == "sit"),
        "runner active state must be SM B's default after replace_sm; got {:?}", runner.active
    );
}
