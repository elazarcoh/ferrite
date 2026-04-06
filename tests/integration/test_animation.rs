use ferrite::sprite::{animation::AnimationState, sheet::load_embedded};

fn sheet() -> ferrite::sprite::sheet::SpriteSheet {
    load_embedded(
        include_bytes!("../../assets/test_pet.json"),
        include_bytes!("../../assets/test_pet.png"),
    )
    .unwrap()
}

#[test]
fn no_advance_before_duration() {
    let s = sheet();
    let mut anim = AnimationState::new("idle");
    let changed = anim.tick(&s, 50);
    assert!(!changed);
    assert_eq!(anim.frame_index, 0);
}

#[test]
fn advances_after_duration() {
    let s = sheet();
    let mut anim = AnimationState::new("idle");
    // idle frames are 200 ms each
    assert!(anim.tick(&s, 200));
    assert_eq!(anim.frame_index, 1);
}

#[test]
fn wraps_forward() {
    let s = sheet();
    let mut anim = AnimationState::new("idle");
    // idle is pingpong with 200 ms frames: 0 → 1 → 0
    anim.tick(&s, 200); // → 1
    anim.tick(&s, 200); // → 0 (pingpong bounce)
    assert_eq!(anim.frame_index, 0);
}

#[test]
fn large_delta_advances() {
    let s = sheet();
    let mut anim = AnimationState::new("idle");
    let changed = anim.tick(&s, 500);
    assert!(changed);
}

#[test]
fn absolute_frame_reflects_tag_offset() {
    let s = sheet();
    let mut anim = AnimationState::new("idle");
    assert_eq!(anim.absolute_frame(&s), 0);
    anim.tick(&s, 200); // idle frames are 200 ms
    assert_eq!(anim.absolute_frame(&s), 1);
}

#[test]
fn ping_pong_reversal() {
    use ferrite::sprite::sheet::{Frame, FrameTag, SpriteSheet, TagDirection};
    use image::RgbaImage;

    let image = RgbaImage::new(96, 32);
    let frames = vec![
        Frame { x: 0, y: 0, w: 32, h: 32, duration_ms: 100 },
        Frame { x: 32, y: 0, w: 32, h: 32, duration_ms: 100 },
        Frame { x: 64, y: 0, w: 32, h: 32, duration_ms: 100 },
    ];
    let tags = vec![FrameTag {
        name: "bounce".into(),
        from: 0,
        to: 2,
        direction: TagDirection::PingPong,
        flip_h: false,
    }];
    let sheet = SpriteSheet { image, frames, tags, sm_mappings: std::collections::HashMap::new(), chromakey: ferrite_core::sprite::sheet::ChromakeyConfig::default(), tight_bboxes: vec![], baseline_offset: 0 };

    let mut anim = AnimationState::new("bounce");
    anim.tick(&sheet, 100); // → 1
    assert_eq!(anim.frame_index, 1);
    anim.tick(&sheet, 100); // → 2
    assert_eq!(anim.frame_index, 2);
    anim.tick(&sheet, 100); // bounce: → 1
    assert_eq!(anim.frame_index, 1);
    anim.tick(&sheet, 100); // → 0
    assert_eq!(anim.frame_index, 0);
    anim.tick(&sheet, 100); // bounce: → 1 again
    assert_eq!(anim.frame_index, 1);
}
