// Integration tests for SpriteEditorState — pure Rust, no Win32.

use my_pet::sprite::editor_state::{EditorTag, SpriteEditorState};
use my_pet::sprite::sheet::TagDirection;
use tempfile::{tempdir, TempDir};

fn test_png_bytes() -> &'static [u8] {
    include_bytes!("../../assets/test_pet.png")
}

fn make_state() -> (SpriteEditorState, TempDir) {
    let tmp = tempdir().unwrap();
    let png_path = tmp.path().join("test_pet.png");
    std::fs::write(&png_path, test_png_bytes()).unwrap();
    let image = image::load_from_memory_with_format(test_png_bytes(), image::ImageFormat::Png)
        .unwrap()
        .into_rgba8();
    let state = SpriteEditorState::new(png_path, image);
    (state, tmp)
}

#[test]
fn frame_rect_uniform_grid() {
    let (mut state, _tmp) = make_state();
    // test_pet.png is 256×32 (2 frames wide, 1 row)
    state.rows = 1;
    state.cols = 2;
    assert_eq!(state.frame_rect(0), (0, 0, 128, 32));
    assert_eq!(state.frame_rect(1), (128, 0, 128, 32));
}

#[test]
fn to_json_produces_valid_aseprite() {
    let (mut state, _tmp) = make_state();
    state.rows = 1;
    state.cols = 2;
    state.tags.push(EditorTag {
        name: "idle".into(),
        from: 0,
        to: 1,
        direction: TagDirection::PingPong,
        flip_h: false,
        color: 0,
    });
    state.tag_map.idle = "idle".into();
    state.tag_map.walk = "walk".into();

    let json = state.to_json();
    // Must parse via from_json_and_image without error
    let image = image::load_from_memory_with_format(test_png_bytes(), image::ImageFormat::Png)
        .unwrap()
        .into_rgba8();
    my_pet::sprite::sheet::SpriteSheet::from_json_and_image(&json, image)
        .expect("to_json must produce valid Aseprite JSON");
    // Must also embed myPetTagMap in JSON
    let parsed: serde_json::Value = serde_json::from_slice(&json).unwrap();
    let tm = parsed.pointer("/meta/myPetTagMap").expect("to_json must embed myPetTagMap");
    assert_eq!(tm["idle"], "idle");
    assert_eq!(tm["walk"], "walk");
}

#[test]
fn clean_json_strips_tag_map() {
    let (mut state, _tmp) = make_state();
    state.rows = 1;
    state.cols = 2;
    state.tag_map.idle = "idle".into();
    state.tag_map.walk = "walk".into();

    let json = state.to_clean_json();
    // Must parse cleanly
    let image = image::load_from_memory_with_format(test_png_bytes(), image::ImageFormat::Png)
        .unwrap()
        .into_rgba8();
    my_pet::sprite::sheet::SpriteSheet::from_json_and_image(&json, image)
        .expect("to_clean_json must produce valid Aseprite JSON");
    // Must NOT contain myPetTagMap
    let text = std::str::from_utf8(&json).unwrap();
    assert!(!text.contains("myPetTagMap"), "clean export must not contain myPetTagMap");
}

#[test]
fn direction_round_trip() {
    use my_pet::sprite::sheet::TagDirection;
    let cases = [
        (TagDirection::Forward,         "forward"),
        (TagDirection::Reverse,         "reverse"),
        (TagDirection::PingPong,        "pingpong"),
        (TagDirection::PingPongReverse, "pingpong_reverse"),
    ];
    for (dir, expected_str) in cases {
        let (mut state, _tmp) = make_state();
        state.rows = 1;
        state.cols = 2;
        state.tags.push(EditorTag { name: "t".into(), from: 0, to: 1, direction: dir.clone(), flip_h: false, color: 0 });
        state.tag_map.idle = "idle".into();
        state.tag_map.walk = "walk".into();
        let json = state.to_json();
        let text = std::str::from_utf8(&json).unwrap();
        assert!(text.contains(expected_str),
            "direction {:?} must serialize to \"{}\"", dir, expected_str);
        // Must round-trip: parse back, tag direction must match
        let image = image::load_from_memory_with_format(test_png_bytes(), image::ImageFormat::Png)
            .unwrap()
            .into_rgba8();
        let sheet = my_pet::sprite::sheet::SpriteSheet::from_json_and_image(&json, image).unwrap();
        assert_eq!(sheet.tags[0].direction, state.tags[0].direction);
    }
}

#[test]
fn is_saveable_requires_idle_and_walk() {
    let (mut state, _tmp) = make_state();
    assert!(!state.is_saveable(), "empty idle+walk → not saveable");
    state.tag_map.idle = "idle".into();
    assert!(!state.is_saveable(), "missing walk → not saveable");
    state.tag_map.walk = "walk".into();
    assert!(state.is_saveable(), "both set → saveable");
    state.tag_map.idle = String::new();
    assert!(!state.is_saveable(), "empty idle → not saveable");
}

#[test]
fn tag_color_assignment() {
    // 10 tags → all get distinct colors without panic
    let colors: Vec<u32> = (0..10).map(SpriteEditorState::assign_color).collect();
    // At least within the 8-color palette they cycle — just verify no panic and
    // that adjacent indices that are < 8 apart get different values
    for i in 0..8 {
        assert_ne!(colors[i], colors[(i + 1) % 8], "adjacent tag colors should differ");
    }
}

#[test]
fn save_to_dir_writes_json_and_png() {
    let (mut state, _state_dir) = make_state();
    let tmp = tempdir().unwrap();
    state.rows = 1;
    state.cols = 2;
    state.tags.push(EditorTag {
        name: "idle".into(), from: 0, to: 1,
        direction: TagDirection::PingPong, flip_h: false, color: 0,
    });
    state.tags.push(EditorTag {
        name: "walk".into(), from: 0, to: 1,
        direction: TagDirection::Forward, flip_h: true, color: 1,
    });
    state.tag_map.idle = "idle".into();
    state.tag_map.walk = "walk".into();

    state.save_to_dir(tmp.path()).expect("save_to_dir must succeed");

    let json_path = tmp.path().join("test_pet.json");
    let png_path = tmp.path().join("test_pet.png");
    assert!(json_path.exists(), "JSON must be written");
    assert!(png_path.exists(), "PNG must be copied");

    // Reload and verify myPetTagMap is present in saved JSON
    let json_bytes = std::fs::read(&json_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
    let tm = parsed.pointer("/meta/myPetTagMap").expect("saved JSON must contain myPetTagMap");
    assert_eq!(tm["idle"], "idle");
    assert_eq!(tm["walk"], "walk");
}

#[test]
fn flip_h_true_round_trips_through_json() {
    let (mut state, _tmp) = make_state();
    state.rows = 1;
    state.cols = 2;
    state.tags.push(EditorTag {
        name: "walk".into(),
        from: 0, to: 1,
        direction: TagDirection::Forward,
        flip_h: true,
        color: 0,
    });
    state.tag_map.idle = "idle".into();
    state.tag_map.walk = "walk".into();

    let json = state.to_json();
    // "flipH" must appear in the serialised JSON
    let text = std::str::from_utf8(&json).unwrap();
    assert!(text.contains("\"flipH\": true"), "flip_h=true must be emitted as \"flipH\": true");

    // Parse back: FrameTag.flip_h must be true
    let image = image::load_from_memory_with_format(test_png_bytes(), image::ImageFormat::Png)
        .unwrap().into_rgba8();
    let sheet = my_pet::sprite::sheet::SpriteSheet::from_json_and_image(&json, image).unwrap();
    let walk_tag = sheet.tags.iter().find(|t| t.name == "walk").expect("walk tag present");
    assert!(walk_tag.flip_h, "flip_h must survive JSON round-trip");
}

#[test]
fn flip_h_false_omits_field_from_json() {
    let (mut state, _tmp) = make_state();
    state.rows = 1;
    state.cols = 2;
    state.tags.push(EditorTag {
        name: "idle".into(),
        from: 0, to: 1,
        direction: TagDirection::Forward,
        flip_h: false,
        color: 0,
    });
    state.tag_map.idle = "idle".into();
    state.tag_map.walk = "walk".into();

    let json = state.to_json();
    let text = std::str::from_utf8(&json).unwrap();
    assert!(!text.contains("\"flipH\""), "flip_h=false must not emit \"flipH\"");
}

#[test]
fn esheep_walk_and_run_tags_have_flip_h() {
    let json_bytes = include_bytes!("../../assets/esheep.json");
    let png_bytes = include_bytes!("../../assets/esheep.png");
    let sheet = my_pet::sprite::sheet::load_embedded(json_bytes, png_bytes)
        .expect("esheep sheet must load");

    let walk = sheet.tags.iter().find(|t| t.name == "walk").expect("walk tag");
    let run  = sheet.tags.iter().find(|t| t.name == "run").expect("run tag");
    assert!(walk.flip_h, "esheep walk must have flip_h=true");
    assert!(run.flip_h,  "esheep run must have flip_h=true");

    let idle = sheet.tags.iter().find(|t| t.name == "idle").expect("idle tag");
    assert!(!idle.flip_h, "esheep idle must have flip_h=false");
}
