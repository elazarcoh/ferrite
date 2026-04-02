// Tests for float scale support in blit_frame and config roundtrip.
use ferrite::window::blender::blit_frame;
use ferrite::config::{load, save, schema::{Config, PetConfig}};
use image::RgbaImage;
use tempfile::tempdir;

fn make_solid_image(w: u32, h: u32) -> RgbaImage {
    let mut img = RgbaImage::new(w, h);
    for p in img.pixels_mut() {
        *p = image::Rgba([200, 150, 100, 255]);
    }
    img
}

#[test]
fn blit_scale_1_5() {
    let img = make_solid_image(4, 4);
    // 4 * 1.5 = 6, so 6×6 destination
    let mut dst = vec![0u8; 6 * 6 * 4];
    blit_frame(&img, 0, 0, 4, 4, &mut dst, 1.5, false);
    assert!(dst.iter().any(|&b| b != 0), "destination buffer should not be all zeros");
    assert_eq!(dst.len(), 6 * 6 * 4, "buffer should be 6×6 BGRA");
}

#[test]
fn blit_scale_0_5() {
    let img = make_solid_image(4, 4);
    // 4 * 0.5 = 2, so 2×2 destination
    let expected_len = 2 * 2 * 4;
    let mut dst = vec![0u8; expected_len];
    blit_frame(&img, 0, 0, 4, 4, &mut dst, 0.5, false);
    assert!(dst.iter().any(|&b| b != 0), "destination buffer should not be all zeros");
    assert_eq!(dst.len(), expected_len);
}

#[test]
fn blit_scale_integer_2_same_as_before() {
    let img = make_solid_image(2, 2);
    let mut dst = Vec::new();
    blit_frame(&img, 0, 0, 2, 2, &mut dst, 2.0, false);
    // 2*2 = 4 pixels → 4×4 output = 64 bytes
    assert_eq!(dst.len(), 4 * 4 * 4);
}

#[test]
fn config_roundtrip_float_scale() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let cfg = Config {
        pets: vec![PetConfig { scale: 0.75, ..PetConfig::default() }],
    };
    save(&path, &cfg).expect("save");
    let loaded = load(&path).expect("load");
    assert!((loaded.pets[0].scale - 0.75_f32).abs() < 1e-4,
        "scale 0.75 should round-trip, got {}", loaded.pets[0].scale);
}

#[test]
fn config_backward_compat_integer_scale() {
    // Write a TOML with integer scale = 2 (no decimal point)
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let toml_text = r#"
[[pets]]
id = "esheep"
sheet_path = "embedded://esheep"
state_machine = "embedded://default"
x = 100
y = 800
scale = 2
walk_speed = 80.0
"#;
    std::fs::write(&path, toml_text).unwrap();
    let loaded = load(&path).expect("load");
    assert!((loaded.pets[0].scale - 2.0_f32).abs() < 1e-4,
        "integer scale=2 in TOML should parse as 2.0f32, got {}", loaded.pets[0].scale);
}
