use ferrite::sprite::sm_runner::{SMRunner, ActiveState};
use ferrite::sprite::sm_compiler::compile;
use ferrite::sprite::sm_format::SmFile;
use ferrite::sprite::sheet::{SpriteSheet, Frame, FrameTag, TagDirection};
use image::RgbaImage;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn mock_sheet() -> SpriteSheet {
    let image = RgbaImage::new(32, 32);
    let frames = vec![Frame { x: 0, y: 0, w: 32, h: 32, duration_ms: 100 }];
    let tags = vec![
        FrameTag { name: "idle".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        FrameTag { name: "sit".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        FrameTag { name: "walk".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        FrameTag { name: "grabbed".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        FrameTag { name: "fall".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        FrameTag { name: "thrown".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
    ];
    SpriteSheet {
        image,
        frames,
        tags,
        sm_mappings: std::collections::HashMap::new(),
        chromakey: ferrite_core::sprite::sheet::ChromakeyConfig::default(),
        baseline_offset: 0,
        tight_bboxes: vec![],
    }
}

fn tick(r: &mut SMRunner, ms: u32) {
    let sheet = mock_sheet();
    r.tick(ms, &mut 100, &mut 100, 1920, 32, 32, 1044, &sheet);
}

/// Build a minimal SM where "idle" transitions to "sit" if the given condition is met
/// (condition is checked after state_time_ms > 0, so any tick will trigger it).
fn sm_with_condition(condition: &str) -> std::sync::Arc<ferrite::sprite::sm_compiler::CompiledSM> {
    let toml_str = format!(
        r#"
[meta]
name = "TestCondSM"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"
transitions = [{{ goto = "sit", condition = {condition:?} }}]

[states.sit]
action = "sit"
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
"#
    );
    let file: SmFile = toml::from_str(&toml_str).unwrap();
    compile(&file).unwrap()
}

fn runner_with_sm(sm: std::sync::Arc<ferrite::sprite::sm_compiler::CompiledSM>) -> SMRunner {
    SMRunner::new(sm, 60.0)
}

// ---------------------------------------------------------------------------
// Test 1: pet_count var reflects update_env_vars and gates transition
// ---------------------------------------------------------------------------

#[test]
fn pet_count_var_threshold() {
    let sm = sm_with_condition("pet_count > 1");
    let mut r = runner_with_sm(sm);

    // With pet_count=1 the condition is false → should stay in idle
    r.update_env_vars(0.0, 0, String::new(), 1080.0, 1, 0.0, 1920.0, String::new());
    tick(&mut r, 100);
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "idle"),
        "pet_count == 1 should NOT trigger transition; got {:?}", r.active
    );

    // With pet_count=2 the condition is true → should transition to sit
    r.update_env_vars(0.0, 0, String::new(), 1080.0, 2, 0.0, 1920.0, String::new());
    tick(&mut r, 100);
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "sit"),
        "pet_count == 2 should transition to 'sit'; got {:?}", r.active
    );
}

// ---------------------------------------------------------------------------
// Test 2: other_pet_dist threshold
// ---------------------------------------------------------------------------

#[test]
fn other_pet_dist_var_threshold() {
    let sm = sm_with_condition("other_pet_dist < 100");
    let mut r = runner_with_sm(sm);

    // Far away — condition false → stays in idle
    r.update_env_vars(0.0, 0, String::new(), 1080.0, 1, 200.0, 1920.0, String::new());
    tick(&mut r, 100);
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "idle"),
        "other_pet_dist=200 should NOT trigger transition; got {:?}", r.active
    );

    // Close — condition true → transitions to sit
    r.update_env_vars(0.0, 0, String::new(), 1080.0, 1, 50.0, 1920.0, String::new());
    tick(&mut r, 100);
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "sit"),
        "other_pet_dist=50 should transition to 'sit'; got {:?}", r.active
    );
}

// ---------------------------------------------------------------------------
// Test 3: surface_label string match
// ---------------------------------------------------------------------------

#[test]
fn surface_label_var_string_match() {
    let sm = sm_with_condition(r#"surface_label == "taskbar""#);
    let mut r = runner_with_sm(sm);

    // Wrong label — condition false → stays in idle
    r.update_env_vars(0.0, 0, String::new(), 1080.0, 1, 0.0, 1920.0, "window".to_string());
    tick(&mut r, 100);
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "idle"),
        "surface_label='window' should NOT trigger transition; got {:?}", r.active
    );

    // Matching label — condition true → transitions to sit
    r.update_env_vars(0.0, 0, String::new(), 1080.0, 1, 0.0, 1920.0, "taskbar".to_string());
    tick(&mut r, 100);
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "sit"),
        "surface_label='taskbar' should transition to 'sit'; got {:?}", r.active
    );
}

// ---------------------------------------------------------------------------
// Test 4: surface_w threshold
// ---------------------------------------------------------------------------

#[test]
fn surface_w_var_threshold() {
    let sm = sm_with_condition("surface_w > 100");
    let mut r = runner_with_sm(sm);

    // Narrow surface — condition false → stays in idle
    r.update_env_vars(0.0, 0, String::new(), 1080.0, 1, 0.0, 50.0, String::new());
    tick(&mut r, 100);
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "idle"),
        "surface_w=50 should NOT trigger transition; got {:?}", r.active
    );

    // Wide surface — condition true → transitions to sit
    r.update_env_vars(0.0, 0, String::new(), 1080.0, 1, 0.0, 200.0, String::new());
    tick(&mut r, 100);
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "sit"),
        "surface_w=200 should transition to 'sit'; got {:?}", r.active
    );
}

// ---------------------------------------------------------------------------
// Test 5: cursor_dist after update_env_vars
// ---------------------------------------------------------------------------

#[test]
fn cursor_dist_var_after_update() {
    let sm = sm_with_condition("cursor_dist < 50");
    let mut r = runner_with_sm(sm);

    // Far cursor — condition false → stays in idle
    r.update_env_vars(100.0, 0, String::new(), 1080.0, 1, 0.0, 1920.0, String::new());
    tick(&mut r, 100);
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "idle"),
        "cursor_dist=100 should NOT trigger transition; got {:?}", r.active
    );

    // Close cursor — condition true → transitions to sit
    r.update_env_vars(30.0, 0, String::new(), 1080.0, 1, 0.0, 1920.0, String::new());
    tick(&mut r, 100);
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "sit"),
        "cursor_dist=30 should transition to 'sit'; got {:?}", r.active
    );
}

// ---------------------------------------------------------------------------
// Test 6: state_time_ms populated after tick
// ---------------------------------------------------------------------------

#[test]
fn state_time_ms_populated_after_tick() {
    // state_time is compared using duration literals (e.g. "100ms"), not raw numbers.
    let sm = sm_with_condition("state_time > 100ms");
    let mut r = runner_with_sm(sm);

    // Tick 200ms — state_time_ms should have accumulated past 100ms and the
    // condition "state_time > 100ms" should fire, transitioning idle → sit.
    tick(&mut r, 200);
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "sit"),
        "after 200ms tick, state_time > 100ms should trigger transition; got {:?}", r.active
    );
}

// ---------------------------------------------------------------------------
// Test 7: distance field — walk stops after distance is exhausted
// ---------------------------------------------------------------------------

#[test]
fn distance_field_walk_stops() {
    // Build a SM where "launch" immediately transitions to "walk" (no condition),
    // walk has distance="50px", and when done auto-transitions to the default
    // fallback "idle".  "idle" has no further transitions so we can detect it.
    // The runner starts in "launch" (set as default_fallback), moves to "walk"
    // via transition_to() (setting walk_remaining_px = 50), then exhausts the
    // distance and returns to "idle".
    let toml_str = r#"
[meta]
name = "TestDistSM"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"
transitions = []

[states.launch]
action = "sit"
transitions = [{ goto = "walk" }]

[states.walk]
action = "walk"
direction = "right"
distance = "50px"
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
    let file: SmFile = toml::from_str(toml_str).unwrap();
    let sm = compile(&file).unwrap();

    // Start directly in "launch" so that the first tick triggers launch → walk
    // via transition_to(), which sets walk_remaining_px = 50px.
    let mut r = runner_with_sm(sm);
    r.force_state = Some("launch".to_string());

    // First tick: apply force_state (→ launch), then launch's unconditional
    // transition fires via transition_to() (→ walk, walk_remaining_px = 50).
    tick(&mut r, 16);
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "walk"),
        "should be in 'walk' after launch transition; got {:?}", r.active
    );

    // Tick repeatedly at 16ms (~60fps) until walk distance is exhausted and
    // the runner auto-transitions to "idle" (the default_fallback).
    let mut transitioned = false;
    for _ in 0..200 {
        tick(&mut r, 16);
        if matches!(&r.active, ActiveState::Named(n) if n == "idle") {
            transitioned = true;
            break;
        }
    }

    assert!(
        transitioned,
        "walk should auto-stop after distance=50px exhausted and return to 'idle'; got {:?}", r.active
    );
}
