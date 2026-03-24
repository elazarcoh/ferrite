// Tests for SpriteGallery — gallery load, install, and appdata resolution.
// Uses MY_PET_SPRITES_DIR env var to redirect installs to a tempdir.

use my_pet::window::sprite_gallery::{SourceKind, SpriteGallery};
use std::path::PathBuf;
use std::sync::Mutex;
use tempfile::TempDir;

// Serialise all tests that touch MY_PET_SPRITES_DIR to avoid env-var races.
static SPRITES_DIR_LOCK: Mutex<()> = Mutex::new(());

/// Returns a TempDir and sets MY_PET_SPRITES_DIR to its path.
/// Also returns the lock guard — must be kept alive for the duration of the test.
fn temp_sprites_dir() -> (TempDir, PathBuf, std::sync::MutexGuard<'static, ()>) {
    let guard = SPRITES_DIR_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    unsafe { std::env::set_var("MY_PET_SPRITES_DIR", &path) };
    (dir, path, guard)
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
    let (_sprites_dir, _, _guard) = temp_sprites_dir();
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
fn install_sprite_rejects_missing_png() {
    let (sprites_dir, _, _guard) = temp_sprites_dir();
    let src_dir = tempfile::tempdir().unwrap();
    let (json_path, png_path) = write_test_sprite_source(&src_dir);
    std::fs::remove_file(&png_path).unwrap();

    let result = SpriteGallery::install(&json_path);
    assert!(result.is_err(), "install must fail when PNG is absent");
    drop(sprites_dir);
}

#[test]
fn install_sprite_overwrites_existing() {
    let (sprites_dir, _, _guard) = temp_sprites_dir();
    let src_dir = tempfile::tempdir().unwrap();
    let (json_path, _) = write_test_sprite_source(&src_dir);

    SpriteGallery::install(&json_path).unwrap();
    // Second install must not error
    SpriteGallery::install(&json_path).expect("second install of same stem must succeed");
    drop(sprites_dir);
}
