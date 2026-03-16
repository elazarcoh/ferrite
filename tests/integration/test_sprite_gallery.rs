// Tests for SpriteGallery — gallery load, install, and appdata resolution.
// Uses MY_PET_SPRITES_DIR env var to redirect installs to a tempdir.

use my_pet::window::sprite_gallery::{SourceKind, SpriteGallery};
use std::path::PathBuf;
use tempfile::TempDir;

/// Returns a TempDir and sets MY_PET_SPRITES_DIR to its path.
/// The TempDir must be kept alive for the duration of the test.
fn temp_sprites_dir() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    // SAFETY: tests run single-threaded (cargo test -- --test-threads=1 if needed).
    unsafe { std::env::set_var("MY_PET_SPRITES_DIR", &path) };
    (dir, path)
}

/// Copy test_pet assets from the embedded store to a temp dir to act as a
/// source for install() tests. Returns paths to the .json and .png files.
fn write_test_sprite_source(dir: &TempDir) -> (PathBuf, PathBuf) {
    use my_pet::assets::Assets;
    use rust_embed::Embed;
    let json_bytes = Assets::get("test_pet.json").unwrap();
    let png_bytes = Assets::get("test_pet.png").unwrap();
    let json_path = dir.path().join("test_pet.json");
    let png_path = dir.path().join("test_pet.png");
    std::fs::write(&json_path, json_bytes.data.as_ref()).unwrap();
    std::fs::write(&png_path, png_bytes.data.as_ref()).unwrap();
    (json_path, png_path)
}

#[test]
fn gallery_load_shows_arrows_not_test_pet() {
    let _sprites_dir = temp_sprites_dir();
    let gallery = SpriteGallery::load();
    let names: Vec<&str> = gallery.entries.iter().map(|e| e.display_name.as_str()).collect();
    // "test_pet" must not appear as a display name — it's remapped to "arrows"
    assert!(!names.contains(&"test_pet"), "test_pet must not appear as display name");
    // "arrows" (the renamed test_pet) must be present
    assert!(names.contains(&"arrows"), "arrows must appear in user-visible gallery");
    // eSheep is embedded and should appear
    assert!(names.iter().any(|n| n.eq_ignore_ascii_case("esheep")));
}

#[test]
fn gallery_load_finds_installed() {
    let (sprites_dir, sprites_path) = temp_sprites_dir();
    // Write a custom sprite JSON into the sprites dir (simulates a prior install).
    // SpriteGallery::load() only scans for *.json files — it does NOT validate
    // or open the PNG, so we only need the JSON file for this test.
    let json = r#"{"frames":[{"frame":{"x":0,"y":0,"w":32,"h":32},"duration":100}],"meta":{"size":{"w":32,"h":32},"frameTags":[]}}"#;
    std::fs::write(sprites_path.join("my_cat.json"), json).unwrap();

    let gallery = SpriteGallery::load();
    let entry = gallery.entries.iter().find(|e| e.display_name == "my_cat");
    assert!(entry.is_some(), "installed sprite must appear in gallery");
    assert!(matches!(entry.unwrap().source, SourceKind::Custom));
    drop(sprites_dir);
}

#[test]
fn install_sprite_copies_files() {
    let (sprites_dir, sprites_path) = temp_sprites_dir();
    let src_dir = tempfile::tempdir().unwrap();
    let (json_path, _png_path) = write_test_sprite_source(&src_dir);

    let entry = SpriteGallery::install(&json_path).expect("install must succeed");
    assert_eq!(entry.display_name, "test_pet");
    assert!(sprites_path.join("test_pet.json").exists());
    assert!(sprites_path.join("test_pet.png").exists());
    assert!(matches!(entry.source, SourceKind::Custom));
    drop(sprites_dir);
}

#[test]
fn install_sprite_rejects_missing_png() {
    let (sprites_dir, _) = temp_sprites_dir();
    let src_dir = tempfile::tempdir().unwrap();
    let (json_path, png_path) = write_test_sprite_source(&src_dir);
    std::fs::remove_file(&png_path).unwrap();

    let result = SpriteGallery::install(&json_path);
    assert!(result.is_err(), "install must fail when PNG is absent");
    drop(sprites_dir);
}

#[test]
fn install_sprite_overwrites_existing() {
    let (sprites_dir, _) = temp_sprites_dir();
    let src_dir = tempfile::tempdir().unwrap();
    let (json_path, _) = write_test_sprite_source(&src_dir);

    SpriteGallery::install(&json_path).unwrap();
    // Second install must not error
    SpriteGallery::install(&json_path).expect("second install of same stem must succeed");
    drop(sprites_dir);
}
