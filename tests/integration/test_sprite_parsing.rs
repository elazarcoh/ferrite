use ferrite::sprite::sheet::{load_embedded, TagDirection};

fn test_json() -> &'static [u8] {
    include_bytes!("../../assets/test_pet.json")
}
fn test_png() -> &'static [u8] {
    include_bytes!("../../assets/test_pet.png")
}

#[test]
fn hash_format_two_frames() {
    let sheet = load_embedded(test_json(), test_png()).unwrap();
    assert_eq!(sheet.frames.len(), 8);
}

#[test]
fn hash_format_frame_dimensions() {
    let sheet = load_embedded(test_json(), test_png()).unwrap();
    assert_eq!(sheet.frames[0].w, 32);
    assert_eq!(sheet.frames[0].h, 32);
    assert_eq!(sheet.frames[1].x, 32);
}

#[test]
fn hash_format_tag_idle() {
    let sheet = load_embedded(test_json(), test_png()).unwrap();
    let tag = sheet.tag("idle").expect("idle tag");
    assert_eq!(tag.from, 0);
    assert_eq!(tag.to, 1);
    assert_eq!(tag.direction, TagDirection::PingPong);
}

#[test]
fn array_format_parsed() {
    let json = r#"{
        "frames": [
            { "frame":{"x":0,"y":0,"w":32,"h":32},"rotated":false,"trimmed":false,
              "spriteSourceSize":{"x":0,"y":0,"w":32,"h":32},"sourceSize":{"w":32,"h":32},"duration":150 },
            { "frame":{"x":32,"y":0,"w":32,"h":32},"rotated":false,"trimmed":false,
              "spriteSourceSize":{"x":0,"y":0,"w":32,"h":32},"sourceSize":{"w":32,"h":32},"duration":200 }
        ],
        "meta": {
            "frameTags": [{"name":"walk","from":0,"to":1,"direction":"reverse"}]
        }
    }"#;
    let image = image::load_from_memory_with_format(test_png(), image::ImageFormat::Png)
        .unwrap()
        .into_rgba8();
    let sheet = ferrite::sprite::sheet::SpriteSheet::from_json_and_image(json.as_bytes(), image).unwrap();
    assert_eq!(sheet.frames.len(), 2);
    assert_eq!(sheet.frames[1].duration_ms, 200);
    let tag = sheet.tag("walk").unwrap();
    assert_eq!(tag.direction, TagDirection::Reverse);
}

#[test]
fn image_correct_dimensions() {
    let sheet = load_embedded(test_json(), test_png()).unwrap();
    assert_eq!(sheet.image.width(), 256);
    assert_eq!(sheet.image.height(), 32);
}
