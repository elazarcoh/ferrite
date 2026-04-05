use ferrite::sprite::sm_runner::{SMRunner, ActiveState, CollideData};
use ferrite::sprite::sm_compiler::compile;
use ferrite::sprite::sm_format::SmFile;
use ferrite::sprite::sheet::{SpriteSheet, Frame, FrameTag, TagDirection};
use image::RgbaImage;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn opaque_sheet() -> SpriteSheet {
    let mut image = RgbaImage::new(32, 32);
    // Fill every pixel fully opaque so hit-test regions compute correctly.
    for px in image.pixels_mut() {
        *px = image::Rgba([255, 255, 255, 255]);
    }
    let frames = vec![Frame { x: 0, y: 0, w: 32, h: 32, duration_ms: 100 }];
    let tags = vec![
        FrameTag { name: "idle".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        FrameTag { name: "sit".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        FrameTag { name: "react".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        FrameTag { name: "nudged".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        FrameTag { name: "fell_react".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
        FrameTag { name: "landed_react".to_string(), from: 0, to: 0, direction: TagDirection::Forward, flip_h: false },
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
        tight_bboxes: vec![],
        baseline_offset: 0,
    }
}

/// Build an SM with a plain "collide" interrupt (no condition on collide_type).
fn sm_with_collide_interrupt(target: &str) -> Arc<ferrite::sprite::sm_compiler::CompiledSM> {
    let toml_str = format!(
        r#"
[meta]
name = "TestCollideSM"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"

[states.idle.interrupts.collide]
goto = "{target}"

[states.{target}]
action = "sit"
duration = "500ms"
transitions = [{{ goto = "idle" }}]

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
        target = target
    );
    let file: SmFile = toml::from_str(&toml_str).unwrap();
    compile(&file).unwrap()
}

/// Build an SM whose collide interrupt is guarded by `collide_type == "<ct>"`.
fn sm_with_typed_collide_interrupt(
    collide_type: &str,
    target: &str,
) -> Arc<ferrite::sprite::sm_compiler::CompiledSM> {
    let toml_str = format!(
        r#"
[meta]
name = "TestTypedCollideSM"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"

[states.idle.interrupts.collide]
goto = "{target}"
condition = 'collide_type == "{ct}"'

[states.{target}]
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
"#,
        ct = collide_type,
        target = target
    );
    let file: SmFile = toml::from_str(&toml_str).unwrap();
    compile(&file).unwrap()
}

/// Build an SMRunner from a compiled SM.
fn runner_with_sm(sm: Arc<ferrite::sprite::sm_compiler::CompiledSM>) -> SMRunner {
    SMRunner::new(sm, 60.0)
}

/// Tick the runner forward by `ms` milliseconds.
fn tick(r: &mut SMRunner, ms: u32) {
    let sheet = opaque_sheet();
    r.tick(ms, &mut 100, &mut 100, 1920, 32, 32, 1044, &sheet);
}

// ---------------------------------------------------------------------------
// Test 1: head-on collision fires interrupt on both runners
// ---------------------------------------------------------------------------

#[test]
fn head_on_collision_fires_interrupt_on_both_runners() {
    let sm = sm_with_typed_collide_interrupt("head_on", "react");
    let mut r1 = runner_with_sm(sm.clone());
    let mut r2 = runner_with_sm(sm);

    r1.on_collide(CollideData {
        collide_type: "head_on".to_string(),
        vx: 100.0,
        vy: 0.0,
        v: 100.0,
    });
    r2.on_collide(CollideData {
        collide_type: "head_on".to_string(),
        vx: -100.0,
        vy: 0.0,
        v: 100.0,
    });

    assert!(
        matches!(&r1.active, ActiveState::Named(n) if n == "react"),
        "r1 should be in 'react' after head_on collision; got {:?}", r1.active
    );
    assert!(
        matches!(&r2.active, ActiveState::Named(n) if n == "react"),
        "r2 should be in 'react' after head_on collision; got {:?}", r2.active
    );
}

// ---------------------------------------------------------------------------
// Test 2: same-dir collision fires interrupt
// ---------------------------------------------------------------------------

#[test]
fn same_dir_collision_fires_interrupt() {
    let sm = sm_with_typed_collide_interrupt("same_dir", "nudged");
    let mut r = runner_with_sm(sm);

    r.on_collide(CollideData {
        collide_type: "same_dir".to_string(),
        vx: 20.0,
        vy: 0.0,
        v: 20.0,
    });

    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "nudged"),
        "runner should be in 'nudged' after same_dir collision; got {:?}", r.active
    );
}

// ---------------------------------------------------------------------------
// Test 3: fell_on and landed_on types assigned correctly
// ---------------------------------------------------------------------------

#[test]
fn fell_on_and_landed_on_types_assigned_correctly() {
    let sm_fell = sm_with_typed_collide_interrupt("fell_on", "fell_react");
    let sm_land = sm_with_typed_collide_interrupt("landed_on", "landed_react");

    let mut r_fell = runner_with_sm(sm_fell);
    let mut r_land = runner_with_sm(sm_land);

    r_fell.on_collide(CollideData {
        collide_type: "fell_on".to_string(),
        vx: 0.0,
        vy: 200.0,
        v: 200.0,
    });
    r_land.on_collide(CollideData {
        collide_type: "landed_on".to_string(),
        vx: 0.0,
        vy: 200.0,
        v: 200.0,
    });

    assert!(
        matches!(&r_fell.active, ActiveState::Named(n) if n == "fell_react"),
        "r_fell should be in 'fell_react'; got {:?}", r_fell.active
    );
    assert!(
        matches!(&r_land.active, ActiveState::Named(n) if n == "landed_react"),
        "r_land should be in 'landed_react'; got {:?}", r_land.active
    );
}

// ---------------------------------------------------------------------------
// Test 4: edge-trigger — interrupt fires exactly once per idle visit
// ---------------------------------------------------------------------------

#[test]
fn edge_trigger_interrupt_fires_exactly_once() {
    let sm = sm_with_collide_interrupt("react");
    let mut r = runner_with_sm(sm);

    // Step 1: first on_collide → should transition to "react"
    r.on_collide(CollideData {
        collide_type: "head_on".to_string(),
        vx: 0.0,
        vy: 0.0,
        v: 50.0,
    });
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "react"),
        "first collide should move to 'react'; got {:?}", r.active
    );

    // Step 2: while in "react", another on_collide fires but react has no
    // collide interrupt → should remain in "react"
    r.on_collide(CollideData {
        collide_type: "head_on".to_string(),
        vx: 0.0,
        vy: 0.0,
        v: 50.0,
    });
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "react"),
        "second collide while in react should stay in 'react'; got {:?}", r.active
    );

    // Step 3: tick until react's duration (500ms) elapses → back to "idle"
    // 60 ticks × 16ms = 960ms >> 500ms
    for _ in 0..60 {
        tick(&mut r, 16);
        if matches!(&r.active, ActiveState::Named(n) if n == "idle") {
            break;
        }
    }
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "idle"),
        "runner should return to 'idle' after react duration; got {:?}", r.active
    );

    // Step 4: new on_collide after returning to idle → should fire again
    r.on_collide(CollideData {
        collide_type: "head_on".to_string(),
        vx: 0.0,
        vy: 0.0,
        v: 50.0,
    });
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "react"),
        "collide after returning to idle should move to 'react' again; got {:?}", r.active
    );
}

// ---------------------------------------------------------------------------
// Test 5: collide_v condition gates the interrupt
// ---------------------------------------------------------------------------

#[test]
fn collide_v_condition_gates_interrupt() {
    let toml_str = r#"
[meta]
name = "TestCondSM"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[states.idle]
required = true
action = "idle"

[states.idle.interrupts.collide]
goto = "react"
condition = "collide_v > 100"

[states.react]
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
"#;
    let file: SmFile = toml::from_str(toml_str).unwrap();
    let sm = compile(&file).unwrap();
    let mut r = runner_with_sm(sm);

    // Low speed — must NOT transition
    r.on_collide(CollideData {
        collide_type: "head_on".to_string(),
        vx: 0.0,
        vy: 0.0,
        v: 50.0,
    });
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "idle"),
        "low-speed collide should NOT transition; got {:?}", r.active
    );

    // High speed — must transition
    r.on_collide(CollideData {
        collide_type: "head_on".to_string(),
        vx: 0.0,
        vy: 0.0,
        v: 150.0,
    });
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "react"),
        "high-speed collide should transition to 'react'; got {:?}", r.active
    );
}

// ---------------------------------------------------------------------------
// Test 6: no interrupt defined — on_collide is silently ignored
// ---------------------------------------------------------------------------

#[test]
fn no_interrupt_defined_ignores_collide() {
    // Build an SM with NO collide interrupt in any state.
    let toml_str = r#"
[meta]
name = "TestNoCollideSM"
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
    let file: SmFile = toml::from_str(toml_str).unwrap();
    let sm = compile(&file).unwrap();
    let mut r = runner_with_sm(sm);

    r.on_collide(CollideData {
        collide_type: "head_on".to_string(),
        vx: 0.0,
        vy: 0.0,
        v: 50.0,
    });

    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "idle"),
        "state must not change when no collide interrupt is defined; got {:?}", r.active
    );
}

// ---------------------------------------------------------------------------
// Test 7: collide vars are cleared after on_collide completes
// ---------------------------------------------------------------------------

#[test]
fn collide_vars_cleared_after_on_collide() {
    let sm = sm_with_collide_interrupt("react");
    let mut r = runner_with_sm(sm);

    r.on_collide(CollideData {
        collide_type: "head_on".to_string(),
        vx: 50.0,
        vy: -30.0,
        v: 58.31,
    });

    // The runner should have transitioned to "react"
    assert!(
        matches!(&r.active, ActiveState::Named(n) if n == "react"),
        "should be in 'react' after on_collide; got {:?}", r.active
    );

    // All four collide vars must be reset to their zero values
    let vars = r.last_condition_vars();
    assert_eq!(vars.collide_type, "", "collide_type must be cleared after on_collide");
    assert_eq!(vars.collide_vx, 0.0, "collide_vx must be cleared after on_collide");
    assert_eq!(vars.collide_vy, 0.0, "collide_vy must be cleared after on_collide");
    assert_eq!(vars.collide_v, 0.0, "collide_v must be cleared after on_collide");
}
