// Integration test: SpriteGallery::delete_installed removes the JSON and PNG files.

use my_pet::window::sprite_gallery::{SpriteGallery, SpriteKey};

/// Write minimal placeholder files to act as a fake installed sprite.
/// Returns (json_path, png_path).
fn write_fake_installed_sprite(dir: &tempfile::TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let json_path = dir.path().join("fake_sprite.json");
    let png_path = dir.path().join("fake_sprite.png");
    std::fs::write(&json_path, b"{}").unwrap();
    std::fs::write(&png_path, b"\x89PNG\r\n").unwrap();
    (json_path, png_path)
}

#[test]
fn delete_installed_removes_json_and_png() {
    let dir = tempfile::tempdir().unwrap();
    let (json_path, png_path) = write_fake_installed_sprite(&dir);

    assert!(json_path.exists(), "JSON must exist before delete");
    assert!(png_path.exists(), "PNG must exist before delete");

    let key = SpriteKey::Installed(json_path.clone());
    SpriteGallery::delete_installed(&key).expect("delete_installed must succeed");

    assert!(!json_path.exists(), "JSON must be gone after delete");
    assert!(!png_path.exists(), "PNG must be gone after delete");
}

#[test]
fn delete_installed_on_embedded_key_returns_error() {
    let key = SpriteKey::Embedded("esheep".to_string());
    let result = SpriteGallery::delete_installed(&key);
    assert!(result.is_err(), "deleting an Embedded key must return an error");
}
